Here's what would make gangstarr more actionable for this kind of optimization work:

1. Show the actual SQL query (or a fingerprint) per finding — The truncated SQL in "Most repeated SQL" is helpful but the per-finding entries only show the callsite. Knowing which table/model is being hit per G001/G002 would let you immediately know whether it's a missing select_related vs prefetch_related.

2. Group findings by query fingerprint, not just callsite — model_units_mixin_serializer.py:38 appears 6 separate times with different repeat counts (15x, 4x, 3x, 3x, 2x, 2x). These are likely different SQL queries all triggered from the same line. Grouping by normalized SQL fingerprint and showing the callsite as a sub-detail would be much clearer.

3. Show the full call stack (or at least 2-3 frames) — Knowing that model_units_mixin_serializer.py:38 is the trigger isn't enough. Was it called from HarvestCompletionActivitySerializer or TillageActivitySerializer? A short stack trace like serializers.py:1853 → HarvestCompletionActivitySerializer.to_representation → model_units_mixin_serializer.py:38 would make it immediately actionable.

4. Identify shared-instance opportunities — When the same table is queried N times with the same PK (e.g., the same farms_field row 15 times), flag it as "same row fetched N times — consider sharing the instance or using select_related".

5. Show query time per-finding — You already show total time for G001 duplicates, but adding per-query p50/p99 latency would help prioritize. The get_previous_episode queries at 14.7ms each are way more impactful than the 1ms unit mixin queries.

6. Suggest the specific Django fix — For G002 N+1 patterns, if you can detect the model and relation name from the SQL, you could suggest the exact select_related('field') or prefetch_related('mix_components') call needed.

7. When you see a log entry like
```
[16/Mar/2026 20:52:18] "GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/event-evidences/?field_id=f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4&download=false HTTP/1.1" 200 7426
```

After finding and fixing you might ONLY care about that entry to see if it is better/worse. You want all of them but the color could change to something different or a Title that says something like (20% improvement over the last 5 runs) - something to prove the AI fixes are real and validated when using the application. 

Again, cache info but WITHOUT changing the ability to quickly run. No migrations, no user input needed, etc.

Before:
```
QUERY REPORT  GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/timeline/f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4/  →  MFRFieldStoryTimelineView
TOTAL         120 queries in 2.3133s
══════════════════════════════════════════════════════════════════════════════
| Scope   | Database | Reads | Writes | Total | Dupes |
|---------|----------|-------|--------|-------|-------|
| RESP    | default  |   120 |      0 |   120 |    57 |

```

After:
```
Entire report could be in mint green text...

[16/Mar/2026 21:34:00] "GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/event-evidences/?field_id=f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4&download=false HTTP/1.1" 200 7426
══════════════════════════════════════════════════════════════════════════════
⬇ 21 Queries - saved .76 seconds <- something like this
QUERY REPORT  GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/timeline/f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4/  →  MFRFieldStoryTimelineView
TOTAL         99 queries in 1.6795s
══════════════════════════════════════════════════════════════════════════════
| Scope   | Database | Reads | Writes | Total | Dupes |
|---------|----------|-------|--------|-------|-------|
| RESP    | default  |    99 |      0 |    99 |    35 |
```
