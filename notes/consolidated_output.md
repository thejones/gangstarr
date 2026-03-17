══════════════════════════════════════════════════════════════════════════════
QUERY REPORT  GET /field-story/v3/users/444e9c43-d1fb-45bc-b25d-8c535480ffd4/timeline/f1eb89eb-fbeb-4bab-b64d-5c273dba9ce4/
VIEW          MFRFieldStoryTimelineView
TOTAL         120 queries in 2.3133s
══════════════════════════════════════════════════════════════════════════════

| Scope | Database | Reads | Writes | Total | Dupes |
|-------|----------|------:|-------:|------:|------:|
| RESP  | default  |   120 |      0 |   120 |    57 |

Consolidated findings by callsite
──────────────────────────────────────────────────────────────────────────────

| File:Line                                                     | Total Q | Dup Groups | Worst Rep | Dup Time | Flags    |
|---------------------------------------------------------------|--------:|-----------:|----------:|---------:|----------|
| core/v3/serializers/model_units_mixin_serializer.py:38        |      25 |          5 |       15x |   20.9ms | HOT, N+1 |
| apps/fieldstory/v3/serializers/serializers.py:1853           |      18 |          3 |        3x |   29.9ms | HOT, N+1 |
| apps/fieldstory/models.py:2578                               |      10 |          1 |       10x |  146.7ms | HOT, N+1 |
| core/v3/serializers/model_representation_primary_key_field_serializer.py:35 | 8 | 3 | 3x | 5.1ms | HOT, N+1 |
| apps/fieldstory/models.py:1899                               |       5 |          1 |        5x |   51.5ms | HOT, N+1 |
| apps/farms/models.py:1461                                    |       5 |          1 |        5x |    6.0ms | HOT, N+1 |
| apps/farms/models.py:1443                                    |       5 |          1 |        5x |   12.8ms | HOT, N+1 |
| apps/fieldstory/models.py:1769                               |       3 |          1 |        3x |   26.1ms | N+1      |
| apps/fieldstory/models.py:2496                               |       3 |          1 |        3x |    2.4ms | N+1      |
| apps/fieldstory/models.py:1746                               |       3 |          1 |        3x |    4.2ms | N+1      |
| apps/fieldstory/models.py:2395                               |       2 |          1 |        2x |    2.1ms | N+1      |

Top repeated query groups
──────────────────────────────────────────────────────────────────────────────

[15x | 14.4ms total] core/v3/serializers/model_units_mixin_serializer.py:38
SELECT "farms_field"."id", "farms_field"."uuid", ...

[10x | 146.7ms total] apps/fieldstory/models.py:2578
SELECT "fieldstory_croppingepisode"."id", ...

[5x | 51.5ms total] apps/fieldstory/models.py:1899
SELECT "fieldstory_croppingepisode"."id", ...

[5x | 12.8ms total] apps/farms/models.py:1443
SELECT "fieldstory_soiltestactivity"."id", ...

[5x | 6.0ms total] apps/farms/models.py:1461
SELECT "fieldstory_soiltestactivity"."id", ...
