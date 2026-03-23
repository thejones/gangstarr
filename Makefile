.PHONY: tree test install build migrate loaddata dev runserver \
	lint lint-fix bump-patch bump-minor bump-major changelog precommit-install \
	db-up db-down db-reset db-populate db-shell pg-exec

MANAGE = python python/gangstarr/testapp/manage.py
PG_ENV = PGDATABASE=gangstarr PGUSER=gangstarr PGPASSWORD=gangstarr PGHOST=localhost PGPORT=5433

## Show project file tree
tree:
	tree -I '.venv|target|__pycache__|*.pyc|.pytest_cache|*.egg-info|*.so|*.dylib|*.dSYM|.git'

## Run tests
test:
	source .venv/bin/activate && $(PG_ENV) pytest

## Install with dev deps
install:
	uv sync --extra dev

## Rebuild Rust extension
build:
	maturin build

## Run Django migrations
migrate:
	source .venv/bin/activate && $(PG_ENV) $(MANAGE) migrate

## Load Chinook fixture
loaddata:
	source .venv/bin/activate && $(PG_ENV) $(MANAGE) loaddata chinook

## Start Docker (if needed), bring up DB, migrate, load fixtures, then run dev server
dev:
	@docker info > /dev/null 2>&1 || (open -a Docker && echo "Starting Docker Desktop…" && until docker info > /dev/null 2>&1; do sleep 2; done)
	$(MAKE) db-up
	$(MAKE) migrate
	$(MAKE) loaddata
	source .venv/bin/activate && $(PG_ENV) python dev.py

## Run Django dev server (no Rust watch)
runserver:
	source .venv/bin/activate && $(PG_ENV) $(MANAGE) runserver

## ── Postgres (Supabase Docker) ──────────────────────────────────────

## Start Supabase Postgres container
db-up:
	docker compose up -d --wait
	@echo "Waiting for Postgres…"
	@until docker compose exec db pg_isready -U gangstarr -q 2>/dev/null; do sleep 1; done
	@docker compose exec -e PGPASSWORD=gangstarr db psql -h localhost -U gangstarr -d postgres -tc \
		"SELECT 1 FROM pg_database WHERE datname = 'gangstarr'" 2>/dev/null | grep -q 1 \
		|| docker compose exec -e PGPASSWORD=gangstarr db createdb -h localhost -U gangstarr gangstarr
	@docker compose exec -e PGPASSWORD=gangstarr db psql -h localhost -U gangstarr -d gangstarr \
		-c "CREATE ROLE supabase_admin" 2>/dev/null || true
	@docker compose exec -e PGPASSWORD=gangstarr db psql -h localhost -U gangstarr -d gangstarr \
		-c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements; CREATE EXTENSION IF NOT EXISTS postgis;"
	@echo "Postgres is ready on localhost:5433"

## Stop Supabase Postgres container
db-down:
	docker compose down

## Destroy volume and recreate
db-reset:
	docker compose down -v
	$(MAKE) db-up migrate loaddata

## Populate statistics — load data + hammer endpoints
db-populate:
	source .venv/bin/activate && $(PG_ENV) $(MANAGE) populate_statistics

## Open psql shell (exec into db container)
db-shell:
	docker compose exec db psql -U gangstarr -d gangstarr

## Open psql shell in a dedicated client container
pg-exec:
	docker compose run --rm psql

## ── Code quality ────────────────────────────────────────────────────

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
