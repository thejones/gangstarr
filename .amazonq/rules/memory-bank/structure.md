# Gangstarr — Project Structure

## Directory Layout

```
gangstarr/
├── src/                          # Rust native extension (PyO3)
│   ├── lib.rs                    # PyO3 module definition — exposes normalize_sql, fingerprint_sql, analyze_events, camel_to_snake, scan_resolvers
│   ├── normalize.rs              # SQL normalization via pg_query (replaces literals with $N placeholders)
│   ├── fingerprint.rs            # Deterministic SQL fingerprinting via pg_query parser
│   ├── group.rs                  # Groups query events by fingerprint into GroupedQuery structs
│   ├── detect.rs                 # Pattern detection: G001 duplicates, G002 N+1, G003 hot callsites
│   ├── models.rs                 # Shared data types: QueryEvent, GroupedQuery, Finding, AnalysisResult (serde + PyO3)
│   └── resolver_index.rs         # Static analysis: scans Python files for GraphQL type classes and resolvers; camel_to_snake conversion
│
├── python/gangstarr/             # Python package (Django integration layer)
│   ├── __init__.py               # Public API exports: full_clip, reporting options, default_base_dir; re-exports Rust functions
│   ├── engine.py                 # Python bridge to Rust: analyze() serializes QueryEvents to JSON, calls Rust analyze_events
│   ├── context_manager.py        # full_clip — ContextDecorator wrapping Django's connection.execute_wrapper with Premier
│   ├── premier.py                # Premier — Django execute_wrapper hook; captures stack traces, builds QueryEvent list + legacy Module/Line dicts
│   ├── reporting.py              # Guru reporter hierarchy + options dataclasses + _format_report console formatter
│   ├── middleware.py             # MomentOfTruthMiddleware (request-level profiling) + YouKnowMySteezeMiddleware (file tracer)
│   ├── graphene.py               # DWYCKMiddleware — Graphene middleware for resolver-level query attribution via thread-local
│   ├── resolver_index.py         # ResolverIndex — cached Python wrapper around Rust scan_resolvers; singleton pattern
│   ├── schemas.py                # Data classes: QueryEvent, RequestContext
│   └── testapp/                  # Minimal Django app for testing
│       ├── manage.py
│       ├── settings.py
│       ├── urls.py
│       ├── views.py
│       ├── models.py
│       ├── schema.py             # Graphene schema with DjangoObjectType classes
│       ├── api_views.py          # DRF API views
│       ├── serializers.py
│       ├── my_module.py          # Helper functions used by tests
│       └── migrations/
│
├── tests/
│   └── test_gangstarr.py         # pytest test suite: unit tests, Rust engine tests, integration tests with Django client
│
├── notes/                        # Design documents and reference material
│   ├── about-gangstarr.md        # Gang Starr discography reference for naming
│   ├── gangstarr_reqs.md         # Full product requirements and roadmap
│   ├── chatgpt-suggestions.md
│   └── rust_libs.md
│
├── Cargo.toml                    # Rust crate config: PyO3, pg_query, regex, serde
├── pyproject.toml                # Python project config: maturin build, Django/pytest deps, ruff linting
├── Makefile                      # Dev commands: build, test, install, dev, lint, migrate, runserver
├── dev.py                        # Dev server with watchfiles auto-rebuild on .rs changes
├── AGENTS.md                     # AI agent guidelines for the project
└── .github/workflows/CI.yml     # GitHub Actions: maturin wheel builds for Linux/macOS/Windows + PyPI publish
```

## Core Architecture

### Data Flow

1. **Capture**: `Premier.__call__` intercepts Django SQL execution via `connection.execute_wrapper`
2. **Attribution**: Stack trace inspection finds the application frame; GraphQL resolver path read from `DWYCKMiddleware` thread-local
3. **Resolver Remapping**: If a resolver path exists, `ResolverIndex` (backed by Rust `scan_resolvers`) remaps the source location from middleware to the actual schema file
4. **Collection**: `QueryEvent` dataclass instances accumulated on `Premier.events`
5. **Analysis**: `engine.analyze()` serializes events to JSON → Rust `analyze_events` normalizes, fingerprints, groups, and detects patterns
6. **Reporting**: `Guru.create()` factory selects the appropriate reporter subclass based on options type; reporter calls `_run_analysis()` and formats output

### Rust ↔ Python Boundary

- Python serializes `QueryEvent` list to JSON string
- Rust deserializes via serde, processes, serializes `AnalysisResult` back to JSON
- PyO3 converts JSON string to Python dict via `json.loads` in the `IntoPyObject` impl
- Standalone functions (`normalize_sql`, `fingerprint_sql`, `camel_to_snake`, `scan_resolvers`) are direct PyO3 `#[pyfunction]` exports

### Key Design Patterns

- **Factory pattern**: `Guru.create()` dispatches to the correct reporter subclass based on options type
- **Strategy pattern**: Reporting options dataclasses (`PrintingOptions`, `LoggingOptions`, `RaisingOptions`, `JsonOptions`) control behavior
- **Singleton**: `ResolverIndex` uses a module-level `_singleton` with lazy initialization
- **Thread-local**: `DWYCKMiddleware` stores resolver path in `threading.local()` for cross-middleware communication
- **Context manager + decorator**: `full_clip` extends `contextlib.ContextDecorator`
- **Dual-mode operation**: `Premier` collects both new structured events and legacy `Module`/`Line` dicts for backward compatibility

### Django Integration Points

- `GANGSTAR_BASE_DIR` setting (required) — determines what counts as application code vs library code
- `GANGSTAR_REPORTING_OPTIONS` setting (optional) — default reporting options
- `GANGSTARR_EXCLUDE_PATHS` setting (optional) — paths to skip in middleware
- `GANGSTARR_ALWAYS_ON` setting (optional) — profile even when `DEBUG=False`
- `MIDDLEWARE` — add `gangstarr.middleware.MomentOfTruthMiddleware`
- `GRAPHENE['MIDDLEWARE']` — add `gangstarr.graphene.DWYCKMiddleware`
