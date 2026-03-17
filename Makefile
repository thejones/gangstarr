.PHONY: tree test install build migrate loaddata dev runserver \
	lint lint-fix bump-patch bump-minor bump-major changelog precommit-install

## Show project file tree
tree:
	tree -I '.venv|target|__pycache__|*.pyc|.pytest_cache|*.egg-info|*.so|*.dylib|*.dSYM|.git'

## Run tests
test:
	source .venv/bin/activate && pytest

## Install with dev deps
install:
	uv sync --extra dev

## Rebuild Rust extension
build:
	maturin build

## Run Django migrations
migrate:
	source .venv/bin/activate && python python/gangstarr/testapp/manage.py migrate

## Load Chinook fixture
loaddata:
	source .venv/bin/activate && python python/gangstarr/testapp/manage.py loaddata chinook

## Run dev server (auto-rebuilds Rust on .rs changes)
dev:
	source .venv/bin/activate && python dev.py

## Run Django dev server (no Rust watch)
runserver:
	source .venv/bin/activate && python python/gangstarr/testapp/manage.py runserver

## Lint Python code
lint:
	source .venv/bin/activate && ruff check python tests

## Lint and fix Python code
lint-fix:
	source .venv/bin/activate && ruff check --fix python tests

## Bump patch version (0.1.0 → 0.1.1)
bump-patch:
	cargo set-version --bump patch

## Bump minor version (0.1.0 → 0.2.0)
bump-minor:
	cargo set-version --bump minor

## Bump major version (0.1.0 → 1.0.0)
bump-major:
	cargo set-version --bump major

## Generate CHANGELOG.md from git history
changelog:
	git-cliff -o CHANGELOG.md

## Install pre-commit and pre-push hooks
precommit-install:
	source .venv/bin/activate && pre-commit install && pre-commit install --hook-type pre-push
