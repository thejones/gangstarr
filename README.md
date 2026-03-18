# gangstarr

> *"Make it dead simple to write Django applications that are not only pleasant to build, but actually performant at scale."*

Gangstarr is a developer-first performance tool for Django + PostgreSQL. It combines:

- **Runtime SQL profiling** — captures every query executed during a request or code block, attributes it to the exact source line, and reports patterns like N+1 queries and repeated SQL
- **Static analysis CLI** — scans Python files for ORM anti-patterns before they reach production
- **Cross-referencing** — correlates static findings against runtime evidence stored in a local SQLite database, escalating confirmed problems
- **Field usage tracking** — records which serializer fields are actually returned per endpoint, enabling precise `.only()` recommendations

Named after the hip-hop duo Gang Starr. Class names and concepts map to their discography.

---

## Installation

Gangstarr is a mixed Rust/Python project built with [maturin](https://github.com/PyO3/maturin).

### From source (development)

```bash
# Install build dependencies
pip install maturin uv

# Clone and set up the project
git clone https://github.com/your-org/gangstarr
cd gangstarr
uv sync --extra dev

# Build and install the Rust extension
source .venv/bin/activate
maturin develop
```

### From PyPI (once published)

```bash
pip install gangstarr
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
# Reporting options instance to use when full_clip is called without arguments.
# Defaults to PrintingOptions() if not set.
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
# Default: ['/static/', '/favicon.ico', '/media/', '/__debug__/']
GANGSTAR_EXCLUDE_PATHS = ['/static/', '/healthz/']

# Profile even when DEBUG=False. Default: False.
GANGSTAR_ALWAYS_ON = False
```

---

## Runtime Profiling

### Context manager — `full_clip`

Wrap any block of code to capture all SQL executed within it:

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

Pass custom reporting options per call:

```python
from gangstarr.reporting import RaisingOptions

with full_clip(reporting_options=RaisingOptions(count_threshold=3)):
    ...
```

Pass metadata for richer output (used internally by `MomentOfTruthMiddleware`):

```python
with full_clip(meta_data={"url": "/api/books/", "method": "GET"}):
    ...
```

### Middleware — `MomentOfTruthMiddleware`

Add automatic request-level profiling with zero code changes in your views:

```python
# settings.py
MIDDLEWARE = [
    ...
    "gangstarr.middleware.MomentOfTruthMiddleware",
    ...
]
```

Active only when `DEBUG=True` (or `GANGSTAR_ALWAYS_ON=True`). Automatically skips paths in `GANGSTAR_EXCLUDE_PATHS`.

---

## Reporting Options

All options inherit from `ReportingOptions`. Import from `gangstarr.reporting`.

| Class | Behaviour |
|---|---|
| `PrintingOptions` | Prints a colour-coded query report to stdout (default) |
| `LoggingOptions` | Emits structured findings to a Python logger |
| `RaisingOptions` | Raises `MassAppealException` when thresholds are exceeded |
| `JsonOptions` | Appends NDJSON records to `.gangstarr/logs/` |

### `PrintingOptions`

```python
from gangstarr.reporting import PrintingOptions

PrintingOptions(
    sort_by="count",                     # Sort queries by field
    count_threshold=1,                   # Minimum query count to report
    duration_threshold=0.0,              # Minimum total duration (seconds) to report
    count_highlighting_threshold=5,      # Red highlight above this count
    duration_highlighting_threshold=0.5, # Red highlight above this duration
)
```

### `LoggingOptions`

```python
from gangstarr.reporting import LoggingOptions

LoggingOptions(
    logger_name="gangstarr",  # Python logger name
    count_threshold=1,
)
```

### `RaisingOptions`

Raises `gangstarr.reporting.MassAppealException` in CI or test suites:

```python
from gangstarr.reporting import RaisingOptions

RaisingOptions(
    count_threshold=5,      # Raise if any callsite executes >= 5 queries
    duration_threshold=0.5, # Raise if any callsite takes >= 0.5s
)
```

### `JsonOptions`

```python
from gangstarr.reporting import JsonOptions

JsonOptions(
    output_dir=".gangstarr",  # Writes NDJSON to .gangstarr/logs/query_report_<ts>.ndjson
)
```

---

## Static Analysis CLI

Scan Python files for ORM anti-patterns without running the application:

```bash
# Scan an entire project
gangstarr check path/to/myproject/

# Scan a single file
gangstarr check path/to/views.py

# Skip test directories and files
gangstarr check path/to/myproject/ --exclude tests --exclude test_

# Custom output directory
gangstarr check path/to/myproject/ --output-dir /tmp/analysis
```

Findings are printed in Ruff-style format and saved to `.gangstarr/static/findings.json`.
Each run is also stored in `.gangstarr/gangstarr.db` for historical tracking and cross-referencing.

### Excluding paths from static analysis

`--exclude <pattern>` can be repeated and strips leading/trailing slashes, so `'/tests/'` and `'tests'` are equivalent.

Patterns are matched against:
1. **Directory names** — exact component match (e.g. `tests` skips any directory named `tests`)
2. **File names** — substring match (e.g. `test_` skips `test_views.py`, `test_models.py`, etc.)

For persistent project-level exclusions, add a `[tool.gangstarr]` section to your `pyproject.toml` in the project root. The CLI reads it automatically:

```toml
[tool.gangstarr]
exclude = [
    "tests",       # skip any directory named 'tests'
    "test_",       # skip any file whose name contains 'test_'
    "conftest.py", # skip conftest files
]
```

> **Note:** `GANGSTARR_EXCLUDE_PATHS` is a *separate* setting used by `MomentOfTruthMiddleware` to skip **HTTP request URL paths** (e.g. `/static/`, `/healthz/`). It has no effect on `gangstarr check`.

### View run history

```bash
gangstarr history path/to/myproject/
gangstarr history path/to/myproject/ --limit 10
```

```
Run ID              Type      When                         Static   Runtime
───────────────────────────────────────────────────────────────────────────
0000019cfde44749    static    2026-03-17T22:22:02.057Z          5         0
```

### Exit codes

| Code | Meaning |
|---|---|
| `0` | No issues found |
| `1` | Issues found |
| `2` | Usage error |

Useful in CI:

```yaml
# .github/workflows/ci.yml
- run: gangstarr check src/
```

### Rules

| Code | Description | Fix |
|---|---|---|
| G101 | Possible N+1 — related field accessed in loop | Add `select_related()` / `prefetch_related()` |
| G102 | `.all()` without `.only()` / `.values()` — over-fetching | Use `.only("id", "name")` or `.values(...)` |
| G103 | Python-side filtering — list comprehension over queryset | Use `.filter(...)` |
| G104 | `len(queryset)` — loads all rows | Use `.count()` |
| G105 | Queryset truthiness check — evaluates entire queryset | Use `.exists()` |
| G106 | Python-side aggregation — `sum(...)` over queryset | Use `.aggregate()` or `.annotate()` |
| G107 | `.save()` in a loop | Use `bulk_create()` or `bulk_update()` |

---

## Field Usage Tracking (DRF)

Track which serializer fields are actually returned per endpoint. This feeds concrete field lists into G102 cross-reference hints, enabling precise `.only()` recommendations.

### 1. Add `FieldUsageMiddleware`

```python
# settings.py
MIDDLEWARE = [
    ...
    "gangstarr.field_tracker.FieldUsageMiddleware",
    "gangstarr.middleware.MomentOfTruthMiddleware",
    ...
]
```

### 2. Add `FieldUsageTrackerMixin` to serializers

```python
from gangstarr.field_tracker import FieldUsageTrackerMixin
from rest_framework import serializers

class BookSerializer(FieldUsageTrackerMixin, serializers.ModelSerializer):
    class Meta:
        model = Book
        fields = "__all__"
```

### 3. Flush and persist

```python
from gangstarr.field_tracker import flush_field_usage
from gangstarr import engine

# At the end of a request, test, or management command:
records = flush_field_usage()
engine.store_field_usage_records(records, project_root=str(BASE_DIR), run_id=run_id)
```

---

## Storage & Cross-referencing

Gangstarr stores findings in a local SQLite database at `<project_root>/.gangstarr/gangstarr.db`. Use the `engine` module to persist and query data programmatically.

### Persist runtime findings

```python
from gangstarr import engine
from gangstarr.context_manager import full_clip

with full_clip() as fc:
    # ... run your code ...
    pass

analysis = engine.analyze(fc._premier.events)
run_id = engine.store_runtime_findings(
    events=fc._premier.events,
    analysis=analysis,
    project_root=str(BASE_DIR),
)
```

### Correlate static + runtime findings

After a static scan has been stored (auto-done by the CLI) and runtime findings exist in the DB:

```python
correlations = engine.correlate(run_id=run_id, project_root=str(BASE_DIR))

for c in correlations:
    print(c["kind"], c["static_rule"], c["file"], c["line"])
    print(c["message"])
    if c["escalated"]:
        print(f"  ↑ Escalated: {c['original_severity']} → {c['escalated_severity']}")
```

Correlation kinds:

| Kind | Description |
|---|---|
| `n1_confirmed` | G101 finding confirmed by runtime repeated-query evidence. Escalated to `error` at ≥ 10 executions. |
| `runtime_confirmed` | Any static finding whose callsite shows high query activity at runtime. |
| `field_narrowing` | G102 finding with concrete field usage data — includes a `.only()` hint. |

### Retrieve history

```python
runs = engine.get_history(project_root=str(BASE_DIR), limit=20)
for run in runs:
    print(run["run_id"], run["run_type"], run["created_at"],
          run["static_count"], run["runtime_count"])
```

---

## Development

### Prerequisites

- Rust (1.70+, install via [rustup](https://rustup.rs))
- Python 3.12+
- [uv](https://github.com/astral-sh/uv)

### Setup

```bash
uv sync --extra dev
source .venv/bin/activate
maturin develop       # Build Rust extension in dev mode
```

### Run tests

```bash
pytest
```

### Lint & format

```bash
ruff check python/ tests/
ruff format python/ tests/
```

### Rebuild after Rust changes

```bash
maturin develop
```

### Run Rust tests

```bash
cargo test
```

---

## Architecture

```
gangstarr/
├── src/                    Rust extension (PyO3 / maturin)
│   ├── lib.rs              PyO3 module + all exported functions
│   ├── static_analysis/    StepInTheArena file walker + G101–G107 rules
│   ├── storage.rs          TakeItPersonal — SQLite schema + insert/fetch
│   ├── correlate.rs        AboveTheClouds — static × runtime cross-referencing
│   ├── cli.rs              gangstarr check / history subcommands
│   ├── reporter.rs         Ruff-style console + JSON reporter
│   ├── normalize.rs        SQL normalisation ($N placeholder substitution)
│   ├── fingerprint.rs      Deterministic query fingerprinting
│   ├── detect.rs           Runtime pattern detection
│   ├── group.rs            Group events by fingerprint
│   └── consolidate.rs      Consolidate findings by callsite
└── python/gangstarr/       Django integration layer
    ├── context_manager.py  full_clip — primary user-facing API
    ├── premier.py          Premier — Django execute_wrapper hook
    ├── reporting.py        Guru reporter hierarchy
    ├── middleware.py        MomentOfTruthMiddleware
    ├── engine.py           Python wrappers over Rust analysis + storage
    ├── field_tracker.py    FieldUsageTrackerMixin + FieldUsageMiddleware
    └── schemas.py          QueryEvent, RequestContext dataclasses
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
