[16/Mar/2026 20:52:18] "GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/event-evidences/?field_id=f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4&download=false HTTP/1.1" 200 7426
══════════════════════════════════════════════════════════════════════════════
QUERY REPORT  GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/timeline/f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4/  →  MFRFieldStoryTimelineView
TOTAL         120 queries in 2.3133s
══════════════════════════════════════════════════════════════════════════════
| Scope   | Database | Reads | Writes | Total | Dupes |
|---------|----------|-------|--------|-------|-------|
| RESP    | default  |   120 |      0 |   120 |    57 |

Findings
──────────────────────────────────────────────────────────────────────────────
[G001] Duplicate queries  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 15 times (total 14.4ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 14 times from core/v3/serializers/model_units_mixin_serializer.py:38
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/models.py:2578
  Query executed 10 times (total 146.7ms)
[G002] Likely N+1 query pattern  apps/fieldstory/models.py:2578
  Query executed 10 times from apps/fieldstory/models.py:2578
  → Consider select_related() or prefetch_related()
[G003] Hot callsite  core/v3/serializers/model_units_mixin_serializer.py:38
  core/v3/serializers/model_units_mixin_serializer.py:38 issued 25 total queries
  → Review this code path for query optimization opportunities
[G001] Duplicate queries  apps/fieldstory/models.py:1899
  Query executed 5 times (total 51.5ms)
[G002] Likely N+1 query pattern  apps/fieldstory/models.py:1899
  Query executed 5 times from apps/fieldstory/models.py:1899
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/farms/models.py:1461
  Query executed 5 times (total 6.0ms)
[G002] Likely N+1 query pattern  apps/farms/models.py:1461
  Query executed 5 times from apps/farms/models.py:1461
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/farms/models.py:1443
  Query executed 5 times (total 12.8ms)
[G002] Likely N+1 query pattern  apps/farms/models.py:1443
  Query executed 5 times from apps/farms/models.py:1443
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 4 times (total 2.9ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 3 times from core/v3/serializers/model_units_mixin_serializer.py:38
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/models.py:1769
  Query executed 3 times (total 26.1ms)
[G002] Likely N+1 query pattern  apps/fieldstory/models.py:1769
  Query executed 3 times from apps/fieldstory/models.py:1769
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/models.py:2496
  Query executed 3 times (total 2.4ms)
[G002] Likely N+1 query pattern  apps/fieldstory/models.py:2496
  Query executed 3 times from apps/fieldstory/models.py:2496
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  Query executed 3 times (total 1.7ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  Query executed 3 times from core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/v3/serializers/serializers.py:1853
  Query executed 3 times (total 13.6ms)
[G001] Duplicate queries  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 3 times (total 1.1ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 3 times from core/v3/serializers/model_units_mixin_serializer.py:38
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/models.py:1746
  Query executed 3 times (total 4.2ms)
[G002] Likely N+1 query pattern  apps/fieldstory/models.py:1746
  Query executed 3 times from apps/fieldstory/models.py:1746
  → Consider select_related() or prefetch_related()
[G003] Hot callsite  apps/farms/models.py:1443
  apps/farms/models.py:1443 issued 5 total queries
  → Review this code path for query optimization opportunities
[G003] Hot callsite  apps/fieldstory/v3/serializers/serializers.py:1853
  apps/fieldstory/v3/serializers/serializers.py:1853 issued 18 total queries
  → Review this code path for query optimization opportunities
[G003] Hot callsite  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  core/v3/serializers/model_representation_primary_key_field_serializer.py:35 issued 8 total queries
  → Review this code path for query optimization opportunities
[G003] Hot callsite  apps/fieldstory/models.py:1899
  apps/fieldstory/models.py:1899 issued 5 total queries
  → Review this code path for query optimization opportunities
[G003] Hot callsite  apps/farms/models.py:1461
  apps/farms/models.py:1461 issued 5 total queries
  → Review this code path for query optimization opportunities
[G003] Hot callsite  apps/fieldstory/models.py:2578
  apps/fieldstory/models.py:2578 issued 10 total queries
  → Review this code path for query optimization opportunities
[G002] Likely N+1 query pattern  apps/fieldstory/v3/serializers/serializers.py:1853
  Query executed 2 times from apps/fieldstory/v3/serializers/serializers.py:1853
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/v3/serializers/serializers.py:1853
  Query executed 2 times (total 13.4ms)
[G002] Likely N+1 query pattern  apps/fieldstory/v3/serializers/serializers.py:1853
  Query executed 2 times from apps/fieldstory/v3/serializers/serializers.py:1853
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 2 times (total 1.2ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 2 times from core/v3/serializers/model_units_mixin_serializer.py:38
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  Query executed 2 times (total 1.4ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  Query executed 2 times from core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  Query executed 2 times (total 2.0ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  Query executed 2 times from core/v3/serializers/model_representation_primary_key_field_serializer.py:35
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/models.py:2395
  Query executed 2 times (total 2.1ms)
[G002] Likely N+1 query pattern  apps/fieldstory/models.py:2395
  Query executed 2 times from apps/fieldstory/models.py:2395
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  apps/fieldstory/v3/serializers/serializers.py:1853
  Query executed 2 times (total 2.9ms)
[G002] Likely N+1 query pattern  apps/fieldstory/v3/serializers/serializers.py:1853
  Query executed 2 times from apps/fieldstory/v3/serializers/serializers.py:1853
  → Consider select_related() or prefetch_related()
[G001] Duplicate queries  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 2 times (total 1.3ms)
[G002] Likely N+1 query pattern  core/v3/serializers/model_units_mixin_serializer.py:38
  Query executed 2 times from core/v3/serializers/model_units_mixin_serializer.py:38
  → Consider select_related() or prefetch_related()

Most repeated SQL
──────────────────────────────────────────────────────────────────────────────

[15x] core/v3/serializers/model_units_mixin_serializer.py:38
SELECT "farms_field"."id", "farms_field"."uuid", "farms_field"."created", "farms_field"."modified", "farms_field"."name", "farms_field"."farm_id", "farms_field"."poly", "farms_field"."validated_poly",

[10x] apps/fieldstory/models.py:2578
SELECT "fieldstory_croppingepisode"."id", "fieldstory_croppingepisode"."source_object_id", "fieldstory_croppingepisode"."is_cover_crop", "fieldstory_croppingepisode"."is_fallow", "fieldstory_croppinge

[5x] apps/fieldstory/models.py:1899
SELECT "fieldstory_croppingepisode"."id", "fieldstory_croppingepisode"."source_object_id", "fieldstory_croppingepisode"."is_cover_crop", "fieldstory_croppingepisode"."is_fallow", "fieldstory_croppinge

[5x] apps/farms/models.py:1461
SELECT "fieldstory_soiltestactivity"."id", "fieldstory_soiltestactivity"."activity_datetime", "fieldstory_soiltestactivity"."create_event_datetime", "fieldstory_soiltestactivity"."edit_event_datetime"

[5x] apps/farms/models.py:1443
SELECT "fieldstory_soiltestactivity"."id", "fieldstory_soiltestactivity"."activity_datetime", "fieldstory_soiltestactivity"."create_event_datetime", "fieldstory_soiltestactivity"."edit_event_datetime"

[4x] core/v3/serializers/model_units_mixin_serializer.py:38
SELECT "auth_user"."id", "auth_user"."password", "auth_user"."last_login", "auth_user"."is_superuser", "auth_user"."username", "auth_user"."first_name", "auth_user"."last_name", "auth_user"."email", "

[3x] apps/fieldstory/models.py:1769
SELECT "fieldstory_croppingepisode"."id", "fieldstory_croppingepisode"."source_object_id", "fieldstory_croppingepisode"."is_cover_crop", "fieldstory_croppingepisode"."is_fallow", "fieldstory_croppinge

[3x] apps/fieldstory/models.py:2496
SELECT "fieldstory_harvestcompletionactivity"."id", "fieldstory_harvestcompletionactivity"."activity_datetime", "fieldstory_harvestcompletionactivity"."create_event_datetime", "fieldstory_harvestcompl

[3x] core/v3/serializers/model_representation_primary_key_field_serializer.py:35
SELECT "fscatalog_productbrand"."id", "fscatalog_productbrand"."source_object_id", "fscatalog_productbrand"."source", "fscatalog_productbrand"."name", "fscatalog_productbrand"."nutrien_esb_id" FROM "f

[3x] apps/fieldstory/v3/serializers/serializers.py:1853
SELECT "community_communitycommitment"."id", "community_communitycommitment"."uuid", "community_communitycommitment"."created", "community_communitycommitment"."modified", "community_communitycommitme
