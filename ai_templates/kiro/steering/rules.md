# Steering Rules

## Code Style
- Use dataclasses for configuration/options, not plain dicts
- Type hints on all function signatures
- Keep modules small and single-purpose
- Follow existing Gang Starr naming theme for new public APIs

## Python
- Target Python 3.8+ (per pyproject.toml)
- Use `from __future__ import annotations` for forward references
- Django is the only framework dependency — don't add others without discussion
- Reporting classes follow the pattern: options dataclass + Guru subclass

## Rust
- PyO3 extension module lives in `src/lib.rs`
- Keep the Rust surface area minimal — it's for performance-critical paths only
- Use `maturin develop` to rebuild during development

## Testing
- Tests use pytest + pytest-django with SQLite
- Test Django app lives in `python/gangstarr/testapp/`
- `settings.py` at project root configures the test Django environment
- Run: `source .venv/bin/activate && pytest`

## Dependencies
- Use uv for package management
- Install test deps: `uv sync --extra tests`
- Lock file: `uv.lock` — commit it
