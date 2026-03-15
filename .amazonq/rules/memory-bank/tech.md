# Gangstarr — Technology Details

## Languages

- **Rust** (edition 2024) — Native extension for SQL analysis, fingerprinting, normalization, and GraphQL resolver scanning
- **Python** (>=3.8, targeting 3.12) — Django integration layer, middleware, reporting, test suite

## Build System

- **maturin** (>=1.12, <2.0) — Builds Rust → Python wheel via PyO3; configured in `pyproject.toml` under `[tool.maturin]`
- **Cargo** — Rust dependency management (`Cargo.toml`)
- **uv** — Python package manager and virtual environment tool (`uv.lock`)

## Rust Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| pyo3 | 0.27.0 | Python ↔ Rust bindings (with `extension-module` feature) |
| pg_query | 6.1 | PostgreSQL parser for SQL normalization and fingerprinting |
| regex | 1 | Pattern matching in resolver index scanner |
| serde | 1 (with `derive`) | Serialization/deserialization of data structures |
| serde_json | 1 | JSON serialization for Python ↔ Rust data exchange |

## Python Dependencies

### Core
- `django>=4.2` — Framework being profiled

### Test (`[project.optional-dependencies].tests`)
- `pytest`
- `pytest-django`

### Dev (`[project.optional-dependencies].dev`)
- `pytest`, `pytest-django`
- `watchfiles` — File watcher for auto Rust rebuild in dev mode
- `djangorestframework` — Used by test app API views
- `graphene-django` — Used by test app GraphQL schema
- `ruff` — Python linter

## Linting

- **ruff** configured in `pyproject.toml`:
  - `target-version = "py312"`
  - `line-length = 120`
  - `src = ["python", "tests"]`
  - Rules: `E` (pycodestyle errors), `F` (pyflakes), `I` (isort), `UP` (pyupgrade)
  - Excludes: `python/gangstarr/testapp/migrations`

## Testing

- **pytest** with `pytest-django`
- Settings: `DJANGO_SETTINGS_MODULE = "gangstarr.testapp.settings"`
- Test paths: `tests/`
- Python path: `.` (project root)
- Options: `--reuse-db`
- Rust tests: `cargo test` (unit tests in `fingerprint.rs`, `normalize.rs`, `resolver_index.rs`)

## Development Commands (Makefile)

| Command | Description |
|---------|-------------|
| `make install` | `uv sync --extra dev` — Install with dev dependencies |
| `make build` | `maturin develop` — Rebuild Rust extension |
| `make test` | `pytest` — Run Python test suite |
| `make dev` | `python dev.py` — Dev server with auto Rust rebuild on `.rs` changes |
| `make runserver` | Django dev server (no Rust watch) |
| `make migrate` | Django migrations via test app manage.py |
| `make loaddata` | Load Chinook fixture |
| `make lint` | `ruff check python tests` |
| `make lint-fix` | `ruff check --fix python tests` |
| `make tree` | Show project file tree |

## CI/CD

- **GitHub Actions** (`.github/workflows/CI.yml`)
- Builds wheels for: Linux (x86_64, x86, aarch64, armv7, s390x, ppc64le), musllinux, Windows (x64, x86, aarch64), macOS (x86_64, aarch64)
- Uses `PyO3/maturin-action@v1` for cross-platform builds
- Publishes to PyPI via `uv publish` on tag push
- Generates artifact attestation via `actions/attest-build-provenance@v3`

## Key Configuration

- `GANGSTAR_BASE_DIR` (Django setting, **required**) — Absolute path to application root; determines what is "application code" in stack traces
- `GANGSTAR_REPORTING_OPTIONS` (Django setting, optional) — Default `ReportingOptions` subclass instance
- `GANGSTARR_EXCLUDE_PATHS` (Django setting, optional) — List of URL prefixes to skip profiling (default: `/static/`, `/favicon.ico`, `/media/`, `/__debug__/`)
- `GANGSTARR_ALWAYS_ON` (Django setting, optional) — Profile even when `DEBUG=False`

## Maturin Configuration

```toml
[tool.maturin]
features = ["pyo3/extension-module"]
python-source = "python"
module-name = "gangstarr.gangstarr"
```

The Rust crate compiles to a `cdylib` and is exposed as `gangstarr.gangstarr` within the Python package at `python/gangstarr/`.
