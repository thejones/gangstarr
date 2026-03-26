# gangstarr

> *"Make it dead simple to write Django applications that are not only pleasant to build, but actually performant at scale."*

Gangstarr is a developer-first performance tool for Django + PostgreSQL. It combines:

- **Runtime SQL profiling** — captures every query during a request or code block, attributes it to the exact source line, and detects patterns like N+1 queries and repeated SQL
- **Static analysis CLI** — scans Python files for ORM anti-patterns (G101–G108) before they reach production
- **Postgres introspection** — analyzes a live database for missing indexes, unused indexes, unstable query plans, and over-fetching (G201–G204)
- **Cross-referencing** — correlates static findings against runtime evidence stored in a local SQLite database, escalating confirmed problems
- **Field usage tracking** — records which serializer fields are actually returned per endpoint, enabling precise `.only()` recommendations
- **AI briefing** — builds a prioritized JSON briefing from all findings and optionally hands it to an AI agent (Kiro, Warp, etc.)

Named after the hip-hop duo Gang Starr. Class names and concepts map to their discography.

---

## Installation

Gangstarr is a mixed Rust/Python project built with [maturin](https://github.com/PyO3/maturin).

### From PyPI

```bash
pip install gangstarr
```

### From source (development)

```bash
git clone https://github.com/thejones/gangstarr
cd gangstarr
uv sync --extra dev
source .venv/bin/activate
maturin develop
```

> **Note:** Gangstarr bundles SQLite via the `rusqlite` `bundled` feature — no system `libsqlite3-dev` is required.

---

## Django Settings

### Required

```python
# settings.py

# Absolute path to your project root — used to distinguish application code
# from library code in stack traces.
GANGSTAR_BASE_DIR = str(BASE_DIR)
```

### Optional

```python
from gangstarr.reporting import PrintingOptions

GANGSTAR_REPORTING_OPTIONS = PrintingOptions(
    sort_by="count",                    # 'line_no' | '-line_no' | 'count' | '-count' | 'duration' | '-duration'
    count_threshold=1,                  # Only report lines with >= N queries
    duration_threshold=0.0,             # Only report lines with >= N seconds total
    count_highlighting_threshold=5,     # Highlight lines with >= N queries in red
    duration_highlighting_threshold=0.5,# Highlight lines with >= 0.5s in red
    max_sql_length=None,                # Truncate SQL in output (None = no limit)
    modules=None,                       # Restrict to specific relative file paths (None = all)
)

# Paths to skip in MomentOfTruthMiddleware.
GANGSTAR_EXCLUDE_PATHS = ['/static/', '/healthz/']

# Profile even when DEBUG=False. Default: False.
GANGSTAR_ALWAYS_ON = False
```

---

## CLI

### Static analysis — `gangstarr check`

```bash
gangstarr check path/to/myproject/
gangstarr check path/to/views.py
gangstarr check . --exclude tests --exclude test_
gangstarr check . --output-dir /tmp/analysis
```

Findings are printed in Ruff-style format and stored in `.gangstarr/gangstarr.db`.

Persistent exclusions via `pyproject.toml`:

```toml
[tool.gangstarr]
exclude = ["tests", "migrations"]
```

### Postgres introspection — `gangstarr pg-royalty`

Analyzes a live Postgres database for schema and query-plan issues:

```bash
gangstarr pg-royalty                       # auto-discovers DB URL from Django settings
gangstarr pg-royalty --db-url postgresql://user:pass@host/db
gangstarr pg-royalty --review              # interactive review mode
gangstarr pg-royalty --stat-findings       # show pg_stat findings only
```

Findings are stored in the same `.gangstarr/gangstarr.db` alongside static and runtime data.

### Run history — `gangstarr history`

```bash
gangstarr history                          # show recent runs
gangstarr history --findings               # show per-finding detail across all sources
gangstarr history --limit 10
```

### AI briefing — `gangstarr steeze`

Builds a prioritized briefing from all findings in the database:

```bash
gangstarr steeze                           # print briefing summary
gangstarr steeze --kiro                    # install .kiro/ templates, store briefing, launch kiro-cli
```

With `--kiro`, gangstarr copies its embedded AI agent templates into the project root’s `.kiro/` directory and launches `kiro-cli --agent steeze`. The agent reads the stored briefing from SQLite and produces actionable fixes.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | No issues found |
| `1` | Issues found |
| `2` | Usage error |

---

## Static Rules (G1xx)

| Code | Description | Fix |
|---|---|---|
| G101 | Possible N+1 — related field accessed in loop | `select_related()` / `prefetch_related()` |
| G102 | `.all()` without `.only()` / `.values()` | `.only("id", "name")` or `.values(...)` |
| G103 | Python-side filtering over queryset | `.filter(...)` |
| G104 | `len(queryset)` loads all rows | `.count()` |
| G105 | Queryset truthiness check | `.exists()` |
| G106 | Python-side aggregation | `.aggregate()` / `.annotate()` |
| G107 | `.save()` in a loop | `bulk_create()` / `bulk_update()` |
| G108 | GraphQL N+1 — implicit resolver without DataLoader | Add DataLoader or `select_related` |

## Postgres Rules (G2xx)

| Code | Description |
|---|---|
| G201 | Missing index on FK/filter column, missing PK, or wide table |
| G202 | High rows/call ratio — possible `.all()` or missing LIMIT |
| G203 | Unused index |
| G204 | Unstable query plan — high stddev/mean execution time |

---

## Runtime Profiling

### Context manager — `full_clip`

```python
from gangstarr.context_manager import full_clip

with full_clip() as fc:
    books = Book.objects.all()
    for book in books:
        print(book.author.name)
# Report is printed on __exit__
```

As a decorator:

```python
@full_clip()
def my_view(request):
    ...
```

Custom reporting options:

```python
from gangstarr.reporting import RaisingOptions

with full_clip(reporting_options=RaisingOptions(count_threshold=3)):
    ...
```

### Middleware — `MomentOfTruthMiddleware`

```python
MIDDLEWARE = [
    ...
    "gangstarr.middleware.MomentOfTruthMiddleware",
]
```

Active when `DEBUG=True` or `GANGSTAR_ALWAYS_ON=True`. Skips paths in `GANGSTAR_EXCLUDE_PATHS`.

### GraphQL — `DWYCKMiddleware`

Graphene middleware for resolver-level query attribution:

```python
GRAPHENE = {
    'MIDDLEWARE': ['gangstarr.graphene.DWYCKMiddleware'],
}
```

When active, every SQL query captured by Premier includes the GraphQL resolver path (e.g. `ArtistType.albums`).

---

## Reporting Options

All options inherit from `ReportingOptions`. Import from `gangstarr.reporting`.

| Class | Behaviour |
|---|---|
| `PrintingOptions` | Colour-coded query report to stdout (default) |
| `LoggingOptions` | Structured findings to a Python logger |
| `RaisingOptions` | Raises `MassAppealException` when thresholds exceeded |
| `JsonOptions` | NDJSON records to `.gangstarr/logs/` |

---

## Field Usage Tracking (DRF)

Track which serializer fields are actually used per endpoint for precise `.only()` hints.

```python
# settings.py
MIDDLEWARE = [
    ...
    "gangstarr.field_tracker.FieldUsageMiddleware",
    "gangstarr.middleware.MomentOfTruthMiddleware",
]
```

```python
from gangstarr.field_tracker import FieldUsageTrackerMixin
from rest_framework import serializers

class BookSerializer(FieldUsageTrackerMixin, serializers.ModelSerializer):
    class Meta:
        model = Book
        fields = "__all__"
```

---

## Development

### Prerequisites

- Rust (1.70+, install via [rustup](https://rustup.rs))
- Python 3.12+
- [uv](https://github.com/astral-sh/uv)
- Docker (for the test Postgres database)

### Quick start

```bash
make install          # uv sync --extra dev
source .venv/bin/activate
maturin develop       # build Rust extension
make dev              # start Postgres, migrate, load fixtures, run dev server
```

### Common commands

```bash
make test             # pytest (uses Postgres on localhost:5433)
make lint             # ruff check
make check            # gangstarr check .
make pg-royalty       # gangstarr pg-royalty
make history          # gangstarr history
make db-reset         # destroy + recreate Postgres + migrate + load fixtures
```

---

## Architecture

```
gangstarr/
├── src/                        Rust extension (PyO3 / maturin)
│   ├── lib.rs                  PyO3 module + all exported functions
│   ├── cli.rs                  gangstarr check / history / steeze / pg-royalty dispatch
│   ├── static_analysis.rs      StepInTheArena file walker + G101–G108 rules
│   ├── storage.rs              TakeItPersonal — SQLite schema + insert/fetch
│   ├── correlate.rs            AboveTheClouds — static × runtime cross-referencing
│   ├── steeze.rs               AI briefing builder + template installer
│   ├── pg_royalty.rs           Postgres introspection CLI
│   ├── pg_schema.rs            Schema analysis (G201)
│   ├── pg_stats.rs             pg_stat_statements analysis (G202–G204)
│   ├── reporter.rs             Ruff-style console + JSON reporter
│   ├── normalize.rs            SQL normalisation ($N placeholder substitution)
│   ├── fingerprint.rs          Deterministic query fingerprinting
│   ├── resolver_index.rs       GraphQL resolver scanner
│   ├── detect.rs               Runtime pattern detection
│   ├── group.rs                Group events by fingerprint
│   ├── models.rs               Shared data structures
│   └── consolidate.rs          Consolidate findings by callsite
├── python/gangstarr/           Django integration layer
│   ├── context_manager.py      full_clip — primary user-facing API
│   ├── premier.py              Premier — Django execute_wrapper hook
│   ├── reporting.py            Guru reporter hierarchy
│   ├── middleware.py           MomentOfTruthMiddleware
│   ├── engine.py               Python wrappers over Rust analysis + storage
│   ├── field_tracker.py        FieldUsageTrackerMixin + FieldUsageMiddleware
│   ├── graphene.py             DWYCKMiddleware — Graphene resolver attribution
│   ├── pg_royalty.py           Django DB URL discovery for pg-royalty
│   ├── resolver_index.py       GraphQL resolver scanning helpers
│   └── schemas.py              QueryEvent, RequestContext dataclasses
└── ai_templates/               Embedded AI agent templates
    └── kiro/                   Kiro agent config + steering prompts
```

### Naming convention

All public API names reference Gang Starr songs or concepts:

| Name | Reference |
|---|---|
| `full_clip` | Song: "Full Clip" |
| `Premier` | DJ Premier |
| `Guru` | Rapper Guru |
| `MomentOfTruthMiddleware` | Album: "Moment of Truth" |
| `MassAppealException` | Song: "Mass Appeal" |
| `step_in_the_arena` | Song: "Step in the Arena" |
| `TakeItPersonal` (storage) | Song: "Take It Personal" |
| `AboveTheClouds` (correlate) | Song: "Above the Clouds" |
| `DWYCKMiddleware` | Song: "DWYCK" |
| `steeze` | Hip-hop slang: effortless style |
| `pg-royalty` | Song: "Royalty" |
