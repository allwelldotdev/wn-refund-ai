# Optional convenience wrapper. `docker compose up` alone is enough to run the app.

DATABASE_URL ?= postgres://refund:refund@localhost:5432/refund
export DATABASE_URL

# Native builds check SQL against the committed backend/.sqlx metadata, as the
# Docker build does (ADR-007), so they compile before the database is migrated.
SQLX_OFFLINE ?= true
export SQLX_OFFLINE

# Compose checks every service's variables, even for `up postgres`. Targets that
# only need the database pass a stand-in key so they work without one; the
# backend itself is only started by `up`, which needs the real key from .env.
DB_COMPOSE = OPENROUTER_API_KEY="$${OPENROUTER_API_KEY:-not-needed-for-this-target}" docker compose

.DEFAULT_GOAL := help
.PHONY: help up down dev psql seed db-reset sqlx-prepare test redteam model-eval check

help: ## List targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  %-14s %s\n",$$1,$$2}'

up: ## Build and start all services
	docker compose up --build

down: ## Stop services (keeps the database volume)
	$(DB_COMPOSE) down

dev: ## Postgres in Docker, backend natively (export OPENROUTER_API_KEY first)
	$(DB_COMPOSE) up -d --wait postgres
	cd backend && cargo run --bin refund-api

psql: ## Open psql on the compose database
	$(DB_COMPOSE) exec postgres psql -U refund -d refund

seed: ## Apply migrations and the idempotent seed to DATABASE_URL
	$(DB_COMPOSE) up -d --wait postgres
	cd backend && cargo run --bin refund-api -- seed

db-reset: ## Drop the database volume, then migrate and seed from scratch
	$(DB_COMPOSE) down -v
	$(MAKE) seed

sqlx-prepare: ## Regenerate backend/.sqlx offline query metadata (commit it)
	$(DB_COMPOSE) up -d --wait postgres
	cd backend && sqlx migrate run --source migrations && SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --all-targets

test: ## Backend tests (database tests use the compose postgres)
	$(DB_COMPOSE) up -d --wait postgres
	cd backend && cargo test --workspace

# The live eval tools read OPENROUTER_API_KEY from the shell (make does not read
# .env) and run on a scratch database they create and drop, never the demo data.
redteam: ## Red-team cases against the live models (export OPENROUTER_API_KEY first)
	$(DB_COMPOSE) up -d --wait postgres
	cd backend && cargo run --bin redteam -- --cases eval/cases.json

model-eval: ## Compare intake models on the red-team cases; options via ARGS="--repeat 3"
	$(DB_COMPOSE) up -d --wait postgres
	cd backend && cargo run --bin model-eval -- --cases eval/cases.json $(ARGS)

check: ## fmt, clippy -D warnings, tests; frontend tsc, eslint and unit tests
	cd backend && cargo fmt --all -- --check
	cd backend && cargo clippy --workspace --all-targets -- -D warnings
	$(MAKE) test
	cd frontend && { [ -d node_modules ] || npm ci --no-audit --no-fund; } && npx tsc --noEmit && npm run lint && npm test
