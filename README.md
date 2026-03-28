# gangstarr

> *"Make it dead simple to write Django applications that are not only pleasant to build, but actually performant at scale."*

Gangstarr is a developer-first performance tool for Django + PostgreSQL. It combines:

- **Runtime SQL profiling** — captures every query during a request or code block, attributes it to the exact source line, and detects patterns like N+1 queries and repeated SQL
- **Static analysis CLI** — scans Python files for ORM anti-patterns (G101–G111) before they reach production
- **Postgres introspection** — analyzes a live database for missing indexes, unused indexes, unstable query plans, sequential scans, table bloat, and cache misses (G201–G207)
- **Cross-referencing** — correlates static findings against runtime evidence stored in a local SQLite database, escalating confirmed problems
- **Query → code path mapping** — connects the most expensive Postgres queries to Django models and static findings
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

No required settings. Gangstarr auto-discovers your project root from Django's `BASE_DIR`.

### Optional

```python
from gangstarr.reporting import PrintingOptions

GANGSTAR_REPORTING_OPTIONS = PrintingOptions(
    sort_by="count",                    # 'line_no' | '-line_no' | 'count' | '-count' | 'duration' | '-duration'
    count_threshold=4,                  # Only report lines with >= N queries (default: 4)
    duration_threshold=0.0,             # Only report lines with >= N seconds total
    count_highlighting_threshold=5,     # Highlight lines with >= N queries in red
    duration_highlighting_threshold=0.5,# Highlight lines with >= 0.5s in red
    max_sql_length=None,                # Truncate SQL in output (None = no limit)
    modules=None,                       # Restrict to specific relative file paths (None = all)
    output_mode="compact",              # 'compact' (default) | 'full'
)

# Override project root (optional — defaults to settings.BASE_DIR)
GANGSTAR_BASE_DIR = str(BASE_DIR)

# Paths to skip in MomentOfTruthMiddleware.
GANGSTAR_EXCLUDE_PATHS = ['/static/', '/healthz/']

# Profile even when DEBUG=False. Default: False.
GANGSTAR_ALWAYS_ON = False
```

### Output Modes

- **`compact`** (default) — Header, summary table, and top repeated query groups (no SQL shown). Clean and fast.
- **`full`** — Everything: header, summary, file locations, consolidated findings table, repeated query groups with SQL.

---

## CLI

### Full Clip — `gangstarr fullclip`

Run static analysis + Postgres introspection in one command:

```bash
gangstarr fullclip                     # scan cwd + auto-detect Postgres DB
gangstarr fullclip path/to/project/    # specify project path
gangstarr fullclip . --include tests   # include test files
```

### Static analysis — `gangstarr check`

```bash
gangstarr check path/to/myproject/
gangstarr check path/to/views.py
gangstarr check . --include tests      # include test files (excluded by default)
gangstarr check . --output-dir /tmp/analysis
```

Findings are printed in Ruff-style format and stored in `.gangstarr/gangstarr.db`.

Test files (`tests/`, `test_*`, `conftest*`) are excluded by default. Use `--include tests` to scan them.

Persistent exclusions via `pyproject.toml`:

```toml
[tool.gangstarr]
exclude = ["migrations"]
```

### Postgres introspection — `gangstarr pg-royalty`

Analyzes a live Postgres database for schema and query-plan issues:

```bash
gangstarr pg-royalty                       # auto-discovers DB URL from Django settings
gangstarr pg-royalty --db-url postgresql://user:pass@host/db
```

Always runs both schema review and stat findings. Results stored in `.gangstarr/gangstarr.db`.

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

With `--kiro`, gangstarr copies its embedded AI agent templates into the project root's `.kiro/` directory and launches `kiro-cli --agent steeze`. The agent reads the stored briefing from SQLite and produces actionable fixes.

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
| G103 | Python-side filtering over queryset | `.filter(...)` |
| G104 | `len(queryset)` loads all rows | `.count()` |
| G105 | Queryset truthiness check | `.exists()` |
| G106 | Python-side aggregation | `.aggregate()` / `.annotate()` |
| G107 | `.save()` in a loop | `bulk_create()` / `bulk_update()` |
| G108 | GraphQL N+1 — implicit resolver without DataLoader | Add DataLoader or `select_related` |
| G109 | Queryset re-evaluation — same qs consumed twice | Cache with `list()` first |
| G110 | `select_related()` incompleteness — nested relation | Add `'field__nested'` to select_related |
| G111 | `count()` + iterate — two SQL queries | Iterate first, `len()` the result |

## Postgres Rules (G2xx)

| Code | Description |
|---|---|
| G201 | Missing index on FK/filter column, missing PK, or wide table |
| G202 | High rows/call ratio — possible `.all()` or missing LIMIT |
| G203 | Unused index |
| G204 | Unstable query plan — high stddev/mean execution time |
| G205 | Sequential scans on large tables — likely missing index |
| G206 | Table bloat — high dead tuple ratio, needs VACUUM |
| G207 | Cache miss rate — table not fitting in shared_buffers |

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
from gangstarr.reporting import PrintingOptions, RaisingOptions

# Compact output (default)
with full_clip():
    ...

# Full verbose output
with full_clip(reporting_options=PrintingOptions(output_mode="full")):
    ...

# Raise on threshold
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
│   ├── cli.rs                  gangstarr fullclip / check / history / steeze / pg-royalty
│   ├── static_analysis/        G101–G111 rules (AST-based)
│   ├── storage.rs              TakeItPersonal — SQLite schema + insert/fetch
│   ├── correlate.rs            AboveTheClouds — static × runtime cross-referencing
│   ├── steeze.rs               AI briefing builder + template installer
│   ├── pg_royalty.rs           Postgres introspection CLI
│   ├── pg_schema.rs            Schema analysis (G201, G203, G205–G207)
│   ├── pg_stats.rs             pg_stat_statements analysis (G202–G204) + query→code mapping
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
│   ├── middleware.py            MomentOfTruthMiddleware
│   ├── engine.py               Python wrappers over Rust analysis + storage
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
