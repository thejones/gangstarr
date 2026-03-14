.PHONY: tree test install build migrate loaddata dev runserver

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
	maturin develop

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
