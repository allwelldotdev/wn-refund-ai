# Optional convenience wrapper. `docker compose up` alone is enough to run the app.

DATABASE_URL ?= postgres://refund:refund@localhost:5432/refund
export DATABASE_URL

.DEFAULT_GOAL := help
.PHONY: help up down dev psql seed db-reset sqlx-prepare test redteam check

help: ## List targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  %-14s %s\n",$$1,$$2}'

up: ## Build and start all services
	docker compose up --build

down: ## Stop services (keeps the database volume)
	docker compose down

dev: ## Postgres in Docker, backend natively
	docker compose up -d --wait postgres
	cd backend && cargo run --bin refund-api

psql: ## Open psql on the compose database
	docker compose exec postgres psql -U refund -d refund

seed: ## Apply migrations and the idempotent seed to DATABASE_URL
	docker compose up -d --wait postgres
	cd backend && cargo run --bin refund-api -- seed

db-reset: ## Drop the database volume, then migrate and seed from scratch
	docker compose down -v
	$(MAKE) seed

sqlx-prepare: ## Regenerate backend/.sqlx offline query metadata (commit it)
	docker compose up -d --wait postgres
	cd backend && sqlx migrate run --source migrations && cargo sqlx prepare --workspace -- --all-targets

test: ## Backend tests (database tests use the compose postgres)
	docker compose up -d --wait postgres
	cd backend && cargo test --workspace

redteam: ## Red-team suite against the live model (available from milestone 7)
	@echo "Red-team suite is added in milestone 7." && exit 1

check: ## fmt, clippy -D warnings, tests; tsc + eslint once the frontend exists
	cd backend && cargo fmt --all -- --check
	cd backend && cargo clippy --workspace --all-targets -- -D warnings
	$(MAKE) test
	@if [ -d frontend ]; then cd frontend && npx tsc --noEmit && npm run lint; fi
