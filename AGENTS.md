# Gangstarr - AI Agent Guidelines

## Project Overview

Gangstarr is a Django SQL query profiling library with a Rust/PyO3 extension module. It instruments Django's database layer to track which lines of application code trigger SQL queries, reporting on query count and duration. Named after the hip-hop duo Gang Starr — class names and concepts map to their discography.

## Architecture

- **Rust extension** (`src/lib.rs`): PyO3-based native module built with maturin, exposed as `gangstarr.gangstarr`
- **Python package** (`python/gangstarr/`): Django integration layer
  - `context_manager.py` — `full_clip` context manager, the primary user-facing API
  - `premier.py` — `Premier` class, the Django `execute_wrapper` hook that captures stack traces and attributes SQL to application code
  - `reporting.py` — `Guru` reporter hierarchy (`PrintingGuru`, `LoggingGuru`, `RaisingGuru`) with corresponding option dataclasses
  - `middleware.py` — `MomentOfTruthMiddleware` for automatic request-level profiling
- **Test app** (`python/gangstarr/testapp/`): Minimal Django app used by pytest
- **Settings** (`settings.py`): Django settings for test runs, requires `GANGSTAR_BASE_DIR`

## Naming Convention

All public API names reference Gang Starr songs/concepts:
- `full_clip` — context manager (song: "Full Clip")
- `Premier` — query interceptor (DJ Premier)
- `Guru` — reporter base class (rapper Guru)
- `MomentOfTruthMiddleware` — Django middleware (album: "Moment of Truth")
- `MassAppealException` — threshold exception (song: "Mass Appeal")

Follow this convention when adding new features.

## Build & Development

- Build system: maturin (Rust → Python wheel)
- Package manager: uv
- Install with test deps: `uv sync --extra tests` or `uv pip install -e ".[tests]"`
- Run tests: `source .venv/bin/activate && pytest`
- Rebuild native module after Rust changes: `maturin develop`

## Key Rules

- The Django setting `GANGSTAR_BASE_DIR` is required — it determines what counts as "application code" vs library code in stack traces
- Don't modify test files unless explicitly asked
- Keep Rust code minimal — heavy logic belongs in Python
- All reporting options are dataclasses inheriting from `ReportingOptions`
