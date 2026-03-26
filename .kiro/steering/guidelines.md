# Gangstarr — Development Guidelines

## Naming Convention (CRITICAL)

All public API names MUST reference Gang Starr songs, albums, or members. This is a core project identity rule.

Examples from the codebase:
- `full_clip` — context manager (song: "Full Clip")
- `Premier` — query interceptor class (DJ Premier)
- `Guru` — reporter base class (rapper Guru)
- `MomentOfTruthMiddleware` — Django middleware (album: "Moment of Truth")
- `MassAppealException` — exception class (song: "Mass Appeal")
- `DWYCKMiddleware` — Graphene middleware (song: "DWYCK")

Reference `notes/about-gangstarr.md` for available song/album names when adding new features.

## Python Code Standards

### Imports
- Use `from __future__ import annotations` at the top of modules that use modern type syntax (seen in `premier.py`, `reporting.py`, `middleware.py`, `engine.py`, `schemas.py`, `graphene.py`, `resolver_index.py`)
- Use `TYPE_CHECKING` guard for imports only needed for type hints:
  ```python
  from typing import TYPE_CHECKING
  if TYPE_CHECKING:
      from gangstarr import ReportingOptions
  ```
- Explicit `__all__` exports in `__init__.py` — every public symbol is listed
- Rust extension re-exported via `from .gangstarr import *  # noqa: F403`

### Type Annotations
- Use modern Python type syntax: `list[str]`, `dict[str, Any]`, `str | None` (not `Optional[str]`)
- Dataclasses for all configuration/data objects (`QueryEvent`, `RequestContext`, `ReportingOptions` and subclasses, `Line`, `Module`)
- Return type annotations on public functions

### Dataclass Patterns
- Options hierarchy uses dataclass inheritance: `PrintingOptions(ReportingOptions)`, `LoggingOptions(ReportingOptions)`, etc.
- Validation in `__post_init__` (see `ReportingOptions.sort_by` validation)
- Default values on all fields for zero-config usage
- `@dataclass(frozen=True)` for immutable value objects (`ResolvedLocation`)

### Error Handling
- Raise `ValueError` with descriptive messages for configuration errors (e.g., missing `GANGSTAR_BASE_DIR`)
- `MassAppealException` for threshold violations (custom exception following naming convention)
- Fail-open pattern for optional features: `except Exception: pass` with comment explaining fallback behavior (see resolver path lookup in `premier.py`)

### Django Integration Patterns
- Access settings via `django.conf.settings` with `getattr()` for optional settings with defaults
- `hasattr(settings, 'GANGSTAR_BASE_DIR')` check before proceeding
- `connection.execute_wrapper()` for SQL interception
- Middleware follows Django's `__init__`/`__call__` pattern with `get_response` chain

## Rust Code Standards

### Module Organization
- One concern per file: `normalize.rs`, `fingerprint.rs`, `group.rs`, `detect.rs`, `models.rs`, `resolver_index.rs`
- All modules declared in `lib.rs` with `mod` statements
- PyO3 functions defined inside `#[pymodule] mod gangstarr { }` block in `lib.rs`

### Data Serialization
- All data structures use `serde::Serialize` and/or `serde::Deserialize` derive macros
- JSON is the interchange format between Python and Rust
- `AnalysisResult` implements `IntoPyObject` by serializing to JSON and calling Python's `json.loads`
- Use `#[serde(default)]` for optional fields, `#[serde(rename_all = "lowercase")]` for enum variants

### Error Handling
- Map Rust errors to `pyo3::exceptions::PyValueError` via `.map_err()`
- `pg_query` parse failures fall back gracefully (return original SQL or hash-based fingerprint)

### Testing
- Inline `#[cfg(test)] mod tests` in each module
- Test both happy path and edge cases (e.g., unparseable SQL in `normalize.rs`)

## Architecture Rules

### Rust vs Python Division
- **Rust**: Performance-critical processing — SQL normalization, fingerprinting, grouping, pattern detection, file scanning
- **Python**: Django framework integration, middleware, configuration, reporting/formatting, test infrastructure
- Keep Rust code minimal — heavy logic belongs in Python (per AGENTS.md)
- Python serializes to JSON → Rust processes → returns JSON → Python deserializes

### Backward Compatibility
- `Premier` maintains dual-mode: new `events` list AND legacy `query_info` dict of `Module`/`Line` objects
- Reporters fall back to legacy output when no events are collected
- `sum_as_string` function kept in Rust module for backward compat

### Factory Pattern for Reporters
```python
# Guru.create() dispatches based on options type
@classmethod
def create(cls, premier: Premier) -> PrintingGuru | LoggingGuru | RaisingGuru | JsonGuru:
    reporting_options = premier.reporting_options
    if isinstance(reporting_options, PrintingOptions):
        return PrintingGuru(premier)
    elif isinstance(reporting_options, LoggingOptions):
        return LoggingGuru(premier)
    # ... etc
```

### Thread-Local for Cross-Middleware Communication
```python
# graphene.py — DWYCKMiddleware stores resolver path
_resolver_context = threading.local()

def get_resolver_path() -> str:
    return getattr(_resolver_context, 'path', '')
```
Premier reads this during SQL capture to attribute queries to GraphQL resolvers.

### Singleton with Lazy Init
```python
# resolver_index.py
_singleton: ResolverIndex | None = None

def get_index() -> ResolverIndex:
    global _singleton
    if _singleton is None:
        from django.conf import settings
        _singleton = ResolverIndex(settings.GANGSTAR_BASE_DIR)
    return _singleton
```

## Testing Patterns

- Use `@pytest.mark.django_db(transaction=True)` for tests that touch the database
- `capture_events` fixture patches `Guru._run_analysis` to intercept events from middleware-profiled requests
- Set `settings.DEBUG = True` in fixtures since Django test framework defaults to `DEBUG=False`
- Reset singleton state in fixtures: `ri._singleton = None`
- Integration tests use Django's `Client` to make real HTTP requests through the middleware stack
- Rust engine tests use pure `QueryEvent` objects without Django database

## Finding Codes

| Code | Pattern | Severity Thresholds |
|------|---------|-------------------|
| G001 | Duplicate queries (same fingerprint) | ≥10 error, ≥3 warning, else info |
| G002 | N+1 pattern (same callsite, repeated) | ≥10 error, ≥3 warning, else info |
| G003 | Hot callsite (single line, many queries) | ≥20 error, ≥5 warning |

## Development Workflow

1. `make install` — Set up environment with dev dependencies
2. `make build` — Rebuild Rust extension after `.rs` changes
3. `make test` — Run full test suite
4. `make dev` — Auto-rebuilding dev server (watches `.rs` files)
5. `make lint` / `make lint-fix` — Check/fix Python code style
6. `cargo test` — Run Rust unit tests independently

## Key Rules (from AGENTS.md)

- `GANGSTAR_BASE_DIR` Django setting is **required** — determines application vs library code boundary
- Don't modify test files unless explicitly asked
- Keep Rust code minimal — heavy logic belongs in Python
- All reporting options are dataclasses inheriting from `ReportingOptions`
