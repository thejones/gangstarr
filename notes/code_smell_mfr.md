```
def get_all_baseline_required_episodes_for_fields(
        self, *,
        enrolled_field_ids: list[int],
        target_filter: dict
    ) -> dict[str, list]:
        """
        Return baseline required episode information

        :param enrolled_field_list: list of all membership enrolled_fields
        :param target_filter: Dict with target episodes

        :return: Dict with fields_errors, baseline_episodes
        """
        community_protocol_reqs = self.protocol_requirements
        episode_ids_lt_max = self.get_baseline_episode_ids(
            enrolled_field_ids=enrolled_field_ids
        ) if len(enrolled_field_ids) > 0 else []

        episodes = CroppingEpisode.objects.filter(
            id__in=episode_ids_lt_max
        ).select_related(
            "field",
            "crop",
            "specific_crop",
            "fs_planting_completion_activity",
            "fs_harvest_completion_activity",
            "fs_planting_completion_activity__edit_event_profile__user",
            "fs_harvest_completion_activity__edit_event_profile__user",
            "create_episode_profile__user",
            "edit_episode_profile__user",
            "fs_best_practices",
            "fs_best_practices__edit_event_profile",
            "fs_best_practices__create_event_profile",
        ).prefetch_related(
            "fs_fertilizer_application_activities__application_method",
            'fs_best_practices__practices'
        ) if len(episode_ids_lt_max) > 0 else []

        episodes_by_field = {field_id: [] for field_id in target_filter}
        for ep in episodes:
            episodes_by_field[ep.field_id].append(ep)

        return_values = {
            "fields_errors": [],
            "baseline_required_grouped_by_field": {},
            "baseline_required_grouped_by_field_and_target_episode": {}
        }

        for field_id, annotated_episodes in episodes_by_field.items():
            target_episodes = target_filter.get(field_id, [])
            all_episodes = set()
            returned_errors = []

            episodes_grouped_by_target = {te.id: [] for te in target_episodes}

            for target_episode in target_episodes:
                try:
                    episodes = CropRotation.get_baseline_required_cropping_episodes(
                        annotated_episodes,
                        target_episode,
                        community_protocol_reqs,
                        include_cover_crops=False,
                        validate=True
                    )
                    # store per-target list and accumulate globally
                    episodes_grouped_by_target[target_episode.id] = episodes
                    all_episodes.update(episodes)

                except MissingRotationSeasonsError as e:
                    returned_errors.append(e)
                    episodes_grouped_by_target[target_episode.id] = []

            if any(returned_errors):
                return_values['fields_errors'].append(
                    FieldMissingRotationSeasonsError(
                        field=target_episodes[0].field,
                        seasons_errors=returned_errors
                    )
                )

            return_values['baseline_required_grouped_by_field'][field_id] = sorted(
                all_episodes,
                key=lambda x: x.calculated_end_date,
                reverse=True
            )

            return_values['baseline_required_grouped_by_field_and_target_episode'][field_id] = (
                episodes_grouped_by_target
            )

        return return_values
```
Yes. There are several code smells here, and a few of them are classic Django performance traps.

The biggest pattern is: **this method is doing a lot of work in Python after a broad fetch, and it is mixing querying, grouping, validation, and result shaping all in one place.** That usually turns into hard-to-see N+1s and expensive per-target repetition.

## Main code smells

### 1. Repeated work per target episode

This is the first thing I would stare at:

```python
for target_episode in target_episodes:
    episodes = CropRotation.get_baseline_required_cropping_episodes(
        annotated_episodes,
        target_episode,
        community_protocol_reqs,
        include_cover_crops=False,
        validate=True
    )
```

If `target_episodes` is large, you are repeatedly scanning the same `annotated_episodes` list for the same field.

That smells like:

* O(fields × target_episodes × episodes_per_field)
* repeated filtering / sorting / date comparisons in Python
* possible repeated access to related objects inside `get_baseline_required_cropping_episodes()`

### Potential fix

Precompute indexes once per field before looping target episodes.

For example:

* sort `annotated_episodes` once
* build lookup maps by year / season / crop / date range if the rotation logic uses them
* move repeated derivation into a helper object

Something like:

```python
rotation_helper = CropRotationIndex(
    episodes=annotated_episodes,
    protocol_requirements=community_protocol_reqs,
)
for target_episode in target_episodes:
    episodes = rotation_helper.get_baseline_required(target_episode)
```

If that inner method currently linearly scans the same list every time, this could help a lot.

---

### 2. Possible hidden N+1 inside `get_baseline_required_cropping_episodes`

This method call is the biggest unknown.

Even though you already use `select_related` and `prefetch_related`, code smell says:

* if that method touches relations not covered here, you still have N+1
* serializer/model properties may trigger more queries
* computed properties on `CroppingEpisode` may query lazily

Given your earlier logs, I would strongly suspect this method or downstream properties are still hitting DB.

### Potential fix

Audit everything touched inside:

* `CropRotation.get_baseline_required_cropping_episodes`
* `calculated_end_date`
* any model `@property`
* any queryset access like `.all()`, `.exists()`, `.first()`, `.last()`

Then make prefetch/select match actual access patterns, not guessed ones.

---

### 3. Broad eager loading may be overfetching

This is another common smell:

```python
.select_related(
    "field",
    "crop",
    "specific_crop",
    ...
    "fs_best_practices",
    ...
).prefetch_related(
    "fs_fertilizer_application_activities__application_method",
    "fs_best_practices__practices"
)
```

This may be too much data for all episodes, especially if only some relations are needed for some targets.

Symptoms:

* high memory use
* slow initial fetch
* expensive object construction
* lots of related rows brought in but unused

### Potential fix

Only load what the rotation logic actually needs.

A good test:

* comment out one relation at a time
* profile query count and total time
* verify correctness

Also consider `Prefetch()` with narrowed querysets instead of raw prefetches.

Example:

```python
from django.db.models import Prefetch

.prefetch_related(
    Prefetch(
        "fs_fertilizer_application_activities",
        queryset=FertilizerApplicationActivity.objects.select_related("application_method")
    )
)
```

That gives you tighter control.

---

### 4. Returning `[]` instead of a queryset

This part is not terrible, but it is messy:

```python
episodes = CroppingEpisode.objects.filter(...).select_related(...).prefetch_related(...) if len(episode_ids_lt_max) > 0 else []
```

Now `episodes` is either:

* a queryset, or
* a list

That kind of type shape drift makes code harder to reason about and harder to optimize.

### Potential fix

Keep it a queryset-shaped flow:

```python
episodes_qs = CroppingEpisode.objects.none()
if episode_ids_lt_max:
    episodes_qs = (
        CroppingEpisode.objects
        .filter(id__in=episode_ids_lt_max)
        .select_related(...)
        .prefetch_related(...)
    )
```

Then iterate over `episodes_qs`.

Cleaner and more predictable.

---

### 5. `len(x) > 0` everywhere

Not a performance killer, but it is noisy:

```python
if len(enrolled_field_ids) > 0
if len(episode_ids_lt_max) > 0
```

### Potential fix

Use truthiness:

```python
if enrolled_field_ids
if episode_ids_lt_max
```

That is the idiomatic version.

---

### 6. Grouping in Python after fetching by ID list

This part:

```python
episodes_by_field = {field_id: [] for field_id in target_filter}
for ep in episodes:
    episodes_by_field[ep.field_id].append(ep)
```

is fine if the dataset is modest, but it can become expensive if `episode_ids_lt_max` is large.

Also, `id__in=episode_ids_lt_max` can be ugly if the list is huge.

### Potential fixes

First, make sure `get_baseline_episode_ids()` is not returning a giant list unnecessarily.

Better options:

* use a subquery if possible instead of materializing Python IDs
* order results for grouping efficiency
* potentially fetch already ordered by `field_id`

Example:

```python
episodes_qs = (
    CroppingEpisode.objects
    .filter(id__in=episode_ids_lt_max)
    .order_by("field_id", "-end_date")
    ...
)
```

Then grouping is cheaper and more predictable.

Even better, if `get_baseline_episode_ids()` can return a queryset/subquery, do that.

---

### 7. Using `set()` of model instances

This line is worth scrutiny:

```python
all_episodes = set()
...
all_episodes.update(episodes)
```

Using Django model instances in sets works by object identity / hash behavior, but it can be fragile and confusing.

If the same database row appears as different Python instances, dedupe behavior may not match your intent.

### Potential fix

Deduplicate by primary key, not object instance.

Example:

```python
episodes_by_id = {}
for ep in episodes:
    episodes_by_id[ep.id] = ep
```

Then:

```python
all_episodes_by_id = {}
for target_episode in target_episodes:
    result = ...
    for ep in result:
        all_episodes_by_id[ep.id] = ep
```

And later:

```python
sorted(all_episodes_by_id.values(), key=lambda x: x.calculated_end_date, reverse=True)
```

That is much safer.

---

### 8. Sorting by a computed property may be expensive

This line worries me:

```python
key=lambda x: x.calculated_end_date
```

If `calculated_end_date` is:

* a property
* computed from related objects
* doing date fallback logic
* lazily triggering access

then sorting can become surprisingly expensive.

### Potential fix

Check whether `calculated_end_date` can be:

* annotated in SQL
* cached on the instance
* precomputed once

At minimum:

```python
episodes_with_dates = [
    (ep, ep.calculated_end_date)
    for ep in all_episodes_by_id.values()
]
episodes_with_dates.sort(key=lambda pair: pair[1], reverse=True)
```

That avoids recomputing the property multiple times during sorting.

---

### 9. Exception-driven control flow inside a hot loop

This bit:

```python
try:
    episodes = CropRotation.get_baseline_required_cropping_episodes(...)
except MissingRotationSeasonsError as e:
    returned_errors.append(e)
```

is okay if errors are rare.

It is bad if errors are common, because exceptions are expensive and make flow harder to optimize mentally.

### Potential fix

If missing seasons is expected business logic, prefer a result object over exceptions.

Example:

```python
result = CropRotation.get_baseline_required_cropping_episodes(...)
if result.error:
    returned_errors.append(result.error)
else:
    ...
```

Not mandatory, but cleaner if this is normal behavior.

---

### 10. `target_episodes[0]` assumes non-empty list

This is a correctness smell:

```python
field=target_episodes[0].field,
```

You are safe only because it is under:

```python
if any(returned_errors):
```

and that only happens inside the loop over `target_episodes`.

So it is probably okay in practice, but it is still brittle.

### Potential fix

Make it explicit:

```python
if returned_errors and target_episodes:
```

or use `field_id` if possible rather than relying on the first episode object.

---

### 11. Method is doing too many jobs

This function:

* gets protocol requirements
* resolves episode ids
* fetches database records
* preloads relations
* groups by field
* computes baseline requirements
* captures errors
* shapes two output dictionaries

That is too much for one method.

### Potential fix

Split it into smaller stages:

* `_get_baseline_episode_queryset(...)`
* `_group_episodes_by_field(...)`
* `_build_field_baseline_results(...)`

That makes optimization much easier because you can benchmark each stage.

---

## Most likely real performance culprits

If I were betting, the hottest issues are probably these:

### A. `get_baseline_required_cropping_episodes()` rescans episode lists repeatedly

That is the biggest algorithmic smell.

### B. Some relation/property accessed inside that method is not prefetched

Classic hidden N+1.

### C. Serializer/model property access on `calculated_end_date` or related objects

Especially if this eventually feeds serializers.

### D. `get_baseline_episode_ids()` may itself be inefficient

You did not show that method, but returning a Python list of IDs often hides an avoidable two-step query pattern.

---

## Concrete refactor direction

Here is the kind of cleanup I would do first.

```python
def get_all_baseline_required_episodes_for_fields(
    self,
    *,
    enrolled_field_ids: list[int],
    target_filter: dict,
) -> dict[str, list]:
    community_protocol_reqs = self.protocol_requirements

    if not enrolled_field_ids:
        return {
            "fields_errors": [],
            "baseline_required_grouped_by_field": {},
            "baseline_required_grouped_by_field_and_target_episode": {},
        }

    episode_ids = self.get_baseline_episode_ids(enrolled_field_ids=enrolled_field_ids)
    if not episode_ids:
        return {
            "fields_errors": [],
            "baseline_required_grouped_by_field": {field_id: [] for field_id in target_filter},
            "baseline_required_grouped_by_field_and_target_episode": {
                field_id: {te.id: [] for te in target_filter.get(field_id, [])}
                for field_id in target_filter
            },
        }

    episodes_qs = (
        CroppingEpisode.objects
        .filter(id__in=episode_ids)
        .select_related(
            "field",
            "crop",
            "specific_crop",
            "fs_planting_completion_activity",
            "fs_harvest_completion_activity",
            "fs_planting_completion_activity__edit_event_profile__user",
            "fs_harvest_completion_activity__edit_event_profile__user",
            "create_episode_profile__user",
            "edit_episode_profile__user",
            "fs_best_practices",
            "fs_best_practices__edit_event_profile",
            "fs_best_practices__create_event_profile",
        )
        .prefetch_related(
            "fs_fertilizer_application_activities__application_method",
            "fs_best_practices__practices",
        )
        .order_by("field_id")
    )

    episodes_by_field = {field_id: [] for field_id in target_filter}
    for ep in episodes_qs:
        episodes_by_field.setdefault(ep.field_id, []).append(ep)

    return_values = {
        "fields_errors": [],
        "baseline_required_grouped_by_field": {},
        "baseline_required_grouped_by_field_and_target_episode": {},
    }

    for field_id, annotated_episodes in episodes_by_field.items():
        target_episodes = target_filter.get(field_id, [])
        returned_errors = []
        grouped_by_target = {}
        deduped = {}

        # good place for a precomputed index/helper
        for target_episode in target_episodes:
            try:
                required = CropRotation.get_baseline_required_cropping_episodes(
                    annotated_episodes,
                    target_episode,
                    community_protocol_reqs,
                    include_cover_crops=False,
                    validate=True,
                )
                grouped_by_target[target_episode.id] = required
                for ep in required:
                    deduped[ep.id] = ep
            except MissingRotationSeasonsError as e:
                returned_errors.append(e)
                grouped_by_target[target_episode.id] = []

        if returned_errors and target_episodes:
            return_values["fields_errors"].append(
                FieldMissingRotationSeasonsError(
                    field=target_episodes[0].field,
                    seasons_errors=returned_errors,
                )
            )

        deduped_values = list(deduped.values())
        dated = [(ep, ep.calculated_end_date) for ep in deduped_values]
        dated.sort(key=lambda pair: pair[1], reverse=True)

        return_values["baseline_required_grouped_by_field"][field_id] = [
            ep for ep, _ in dated
        ]
        return_values["baseline_required_grouped_by_field_and_target_episode"][field_id] = grouped_by_target

    return return_values
```

This is not a magic speedup, but it removes some obvious slop:

* no queryset/list switching
* dedupe by ID instead of object
* explicit early returns
* one-time date evaluation before sort

---

## What I would inspect next

In order:

1. `get_baseline_episode_ids()`

   * does it do extra queries?
   * can it return a queryset/subquery instead of Python IDs?

2. `CropRotation.get_baseline_required_cropping_episodes()`

   * does it scan full lists repeatedly?
   * does it touch lazy relations?
   * does it sort/filter repeatedly?

3. `CroppingEpisode.calculated_end_date`

   * property or DB field?
   * any hidden relation access?

4. exact usage of:

   * `fs_best_practices`
   * `fs_fertilizer_application_activities`
   * nested profile/user relations

A lot of Django slowness comes from prefetching the wrong things and missing the one thing that is actually accessed.

## Fastest likely wins

If you want the most practical shortlist:

* **dedupe by `ep.id`, not model instance**
* **cache or precompute `calculated_end_date` before sorting**
* **audit `get_baseline_required_cropping_episodes()` for repeated list scans**
* **make prefetch/select match actual access in that method**
* **replace large Python `id__in` lists with subqueries if possible**

That is where I’d start. If you paste `get_baseline_required_cropping_episodes()` next, I can tell you much more precisely where the real performance risk is.
