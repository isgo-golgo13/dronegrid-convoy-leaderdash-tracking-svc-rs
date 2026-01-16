# ==============================================================================
# DRONE CONVOY TRACKER - Production Build System
# ==============================================================================
# Classification: UNCLASSIFIED // FOR OFFICIAL USE ONLY
#
# Makefile for building, testing, and deploying the complete drone convoy
# tracking system including ScyllaDB backend, GraphQL API, and Leptos frontend.
#
# Usage:
#   make help          - Show available targets
#   make all           - Build everything (backend + frontend)
#   make dev           - Start development environment
#   make prod          - Build for production
#   make docker        - Build Docker images
# ==============================================================================

SHELL := /bin/bash
.ONESHELL:
.SHELLFLAGS := -eu -o pipefail -c
.DELETE_ON_ERROR:
MAKEFLAGS += --warn-undefined-variables
MAKEFLAGS += --no-builtin-rules

# ------------------------------------------------------------------------------
# Configuration
# ------------------------------------------------------------------------------

PROJECT_NAME := drone-convoy-tracker
VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
GIT_SHA := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_TIME := $(shell date -u +"%Y-%m-%dT%H:%M:%SZ")

ROOT_DIR := $(shell pwd)
TARGET_DIR := $(ROOT_DIR)/target
DIST_DIR := $(ROOT_DIR)/dist
FRONTEND_DIR := $(ROOT_DIR)/crates/drone-frontend
SCHEMA_DIR := $(ROOT_DIR)/schema

CARGO := cargo
RUSTFLAGS_RELEASE := -C target-cpu=native -C opt-level=3 -C lto=fat -C codegen-units=1
CARGO_BUILD_JOBS := $(shell nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

TRUNK := trunk
WASM_TARGET := wasm32-unknown-unknown

DOCKER := docker
DOCKER_REGISTRY := ghcr.io/enginevector
DOCKER_TAG := $(VERSION)-$(GIT_SHA)

SCYLLA_HOST := localhost
SCYLLA_PORT := 9042
SCYLLA_KEYSPACE := drone_ops
REDIS_URL := redis://localhost:6379

RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[0;33m
BLUE := \033[0;34m
PURPLE := \033[0;35m
CYAN := \033[0;36m
NC := \033[0m

.DEFAULT_GOAL := help

# ------------------------------------------------------------------------------
# Help
# ------------------------------------------------------------------------------

.PHONY: help
help:
	@echo ""
	@echo "$(CYAN)╔══════════════════════════════════════════════════════════════════╗$(NC)"
	@echo "$(CYAN)║         DRONE CONVOY TRACKER - Build System                      ║$(NC)"
	@echo "$(CYAN)╚══════════════════════════════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)Usage:$(NC) make [target]"
	@echo ""
	@echo "$(GREEN)Build Targets:$(NC)"
	@echo "  $(BLUE)all$(NC)              Build everything (backend + frontend)"
	@echo "  $(BLUE)quick$(NC)            Quick debug build (backend only, no frontend)"
	@echo "  $(BLUE)build-backend$(NC)    Build all backend crates (release)"
	@echo "  $(BLUE)build-frontend$(NC)   Build Leptos frontend (WASM)"
	@echo "  $(BLUE)build-debug$(NC)      Build all crates (debug mode)"
	@echo "  $(BLUE)build-api$(NC)        Build only GraphQL API server"
	@echo "  $(BLUE)build-simulator$(NC)  Build only drone simulator"
	@echo ""
	@echo "$(GREEN)Run Targets:$(NC)"
	@echo "  $(BLUE)run-api$(NC)          Run GraphQL API (debug)"
	@echo "  $(BLUE)run-api-release$(NC)  Run GraphQL API (release)"
	@echo "  $(BLUE)run-simulator$(NC)    Run drone simulator (debug)"
	@echo "  $(BLUE)run-simulator-release$(NC) Run drone simulator (release)"
	@echo ""
	@echo "$(GREEN)Development:$(NC)"
	@echo "  $(BLUE)dev$(NC)              Start full development environment"
	@echo "  $(BLUE)dev-backend$(NC)      Start API in watch mode"
	@echo "  $(BLUE)dev-frontend$(NC)     Start frontend dev server"
	@echo "  $(BLUE)dev-db$(NC)           Start development databases"
	@echo ""
	@echo "$(GREEN)WASM/Frontend:$(NC)"
	@echo "  $(BLUE)setup-wasm$(NC)       Install WASM toolchain and trunk"
	@echo "  $(BLUE)wasm-check$(NC)       Verify WASM environment is ready"
	@echo ""
	@echo "$(GREEN)Testing:$(NC)"
	@echo "  $(BLUE)test$(NC)             Run all tests"
	@echo "  $(BLUE)test-unit$(NC)        Run unit tests"
	@echo "  $(BLUE)lint$(NC)             Run linters (fmt + clippy)"
	@echo ""
	@echo "$(GREEN)Production:$(NC)"
	@echo "  $(BLUE)prod$(NC)             Full production build"
	@echo "  $(BLUE)docker$(NC)           Build Docker images"
	@echo "  $(BLUE)package$(NC)          Create distribution package"
	@echo ""
	@echo "$(GREEN)Utilities:$(NC)"
	@echo "  $(BLUE)setup$(NC)            Install development dependencies"
	@echo "  $(BLUE)clean$(NC)            Clean build artifacts"
	@echo "  $(BLUE)docs$(NC)             Generate documentation"
	@echo ""
	@echo "$(PURPLE)Version: $(VERSION) | SHA: $(GIT_SHA)$(NC)"
	@echo ""

# ------------------------------------------------------------------------------
# Setup
# ------------------------------------------------------------------------------

.PHONY: setup
setup:
	@echo "$(CYAN)▶ Installing development dependencies...$(NC)"
	@rustup show active-toolchain || rustup default stable
	@rustup target add $(WASM_TARGET)
	@cargo install trunk --locked 2>/dev/null || true
	@cargo install wasm-bindgen-cli --locked 2>/dev/null || true
	@cargo install cargo-watch --locked 2>/dev/null || true
	@cargo install cargo-audit --locked 2>/dev/null || true
	@echo "$(GREEN)✓ Setup complete!$(NC)"

.PHONY: setup-wasm
setup-wasm:
	@echo "$(CYAN)▶ Setting up WASM environment...$(NC)"
	@rustup target add $(WASM_TARGET)
	@cargo install trunk --locked 2>/dev/null || echo "trunk already installed"
	@cargo install wasm-bindgen-cli --locked 2>/dev/null || echo "wasm-bindgen-cli already installed"
	@echo "$(GREEN)✓ WASM environment ready$(NC)"
	@echo ""
	@echo "  WASM target: $(WASM_TARGET)"
	@echo "  Trunk:       $$(trunk --version 2>/dev/null || echo 'not found')"
	@echo ""

.PHONY: wasm-check
wasm-check:
	@echo "$(CYAN)▶ Checking WASM environment...$(NC)"
	@rustup target list --installed | grep -q $(WASM_TARGET) && \
		echo "$(GREEN)✓ WASM target installed$(NC)" || \
		{ echo "$(RED)✗ WASM target missing - run 'make setup-wasm'$(NC)"; exit 1; }
	@command -v trunk >/dev/null 2>&1 && \
		echo "$(GREEN)✓ trunk: $$(trunk --version)$(NC)" || \
		{ echo "$(RED)✗ trunk not found - run 'make setup-wasm'$(NC)"; exit 1; }
	@command -v wasm-bindgen >/dev/null 2>&1 && \
		echo "$(GREEN)✓ wasm-bindgen: $$(wasm-bindgen --version)$(NC)" || \
		echo "$(YELLOW)⚠ wasm-bindgen not found (optional)$(NC)"

.PHONY: check-deps
check-deps:
	@command -v cargo >/dev/null 2>&1 || { echo "$(RED)✗ cargo not found$(NC)"; exit 1; }
	@command -v trunk >/dev/null 2>&1 || { echo "$(RED)✗ trunk not found - run 'make setup'$(NC)"; exit 1; }
	@rustup target list --installed | grep -q $(WASM_TARGET) || { echo "$(RED)✗ WASM target not installed$(NC)"; exit 1; }
	@echo "$(GREEN)✓ Dependencies OK$(NC)"

# ------------------------------------------------------------------------------
# Build
# ------------------------------------------------------------------------------

.PHONY: all
all: build-backend build-frontend
	@echo "$(GREEN)✓ Full build complete!$(NC)"

.PHONY: build
build: all

.PHONY: build-backend
build-backend:
	@echo "$(CYAN)▶ Building backend crates...$(NC)"
	@RUSTFLAGS="$(RUSTFLAGS_RELEASE)" $(CARGO) build \
		--release \
		--jobs $(CARGO_BUILD_JOBS) \
		--workspace \
		--exclude drone-frontend
	@echo "$(GREEN)✓ Backend build complete$(NC)"

.PHONY: build-frontend
build-frontend: wasm-check
	@echo "$(CYAN)▶ Building frontend (WASM)...$(NC)"
	@cd $(FRONTEND_DIR) && $(TRUNK) build --release
	@echo "$(GREEN)✓ Frontend build complete$(NC)"
	@echo "  Output: $(FRONTEND_DIR)/dist/"

.PHONY: build-debug
build-debug:
	@echo "$(CYAN)▶ Building (debug mode)...$(NC)"
	@$(CARGO) build --workspace
	@echo "$(GREEN)✓ Debug build complete$(NC)"

.PHONY: build-api
build-api:
	@echo "$(CYAN)▶ Building GraphQL API...$(NC)"
	@RUSTFLAGS="$(RUSTFLAGS_RELEASE)" $(CARGO) build --release --package drone-graphql-api
	@echo "$(GREEN)✓ API: $(TARGET_DIR)/release/drone-graphql-api$(NC)"

.PHONY: build-simulator
build-simulator:
	@echo "$(CYAN)▶ Building Drone Simulator...$(NC)"
	@RUSTFLAGS="$(RUSTFLAGS_RELEASE)" $(CARGO) build --release --package drone-simulator
	@echo "$(GREEN)✓ Simulator: $(TARGET_DIR)/release/drone-simulator$(NC)"

# Quick debug build (excludes frontend - much faster)
.PHONY: quick
quick:
	@echo "$(CYAN)▶ Quick build (backend only, debug)...$(NC)"
	@$(CARGO) build --workspace --exclude drone-frontend
	@echo "$(GREEN)✓ Quick build complete$(NC)"
	@echo "  API:       $(TARGET_DIR)/debug/drone-api"
	@echo "  Simulator: $(TARGET_DIR)/debug/drone-simulator"

# ------------------------------------------------------------------------------
# Run
# ------------------------------------------------------------------------------

.PHONY: run-api
run-api:
	@echo "$(CYAN)▶ Starting GraphQL API...$(NC)"
	@$(CARGO) run --package drone-graphql-api

.PHONY: run-api-release
run-api-release: build-api
	@echo "$(CYAN)▶ Starting GraphQL API (release)...$(NC)"
	@$(TARGET_DIR)/release/drone-api

.PHONY: run-simulator
run-simulator:
	@echo "$(CYAN)▶ Starting Drone Simulator...$(NC)"
	@$(CARGO) run --package drone-simulator

.PHONY: run-simulator-release
run-simulator-release: build-simulator
	@echo "$(CYAN)▶ Starting Drone Simulator (release)...$(NC)"
	@$(TARGET_DIR)/release/drone-simulator

# ------------------------------------------------------------------------------
# Development
# ------------------------------------------------------------------------------

.PHONY: dev
dev:
	@echo "$(CYAN)▶ Starting development environment...$(NC)"
	@echo "$(YELLOW)  Starting API + Frontend in parallel...$(NC)"
	@$(MAKE) -j2 dev-backend dev-frontend

.PHONY: dev-backend
dev-backend:
	@echo "$(CYAN)▶ Starting API server (watch mode)...$(NC)"
	@cargo watch -x 'run --package drone-graphql-api'

.PHONY: dev-frontend
dev-frontend: wasm-check
	@echo "$(CYAN)▶ Starting frontend dev server...$(NC)"
	@cd $(FRONTEND_DIR) && $(TRUNK) serve --open

.PHONY: dev-db
dev-db:
	@echo "$(CYAN)▶ Starting development databases...$(NC)"
	@docker compose -f docker/docker-compose.dev.yml up -d scylla redis
	@echo "$(YELLOW)  Waiting for ScyllaDB...$(NC)"
	@sleep 15
	@$(MAKE) db-init
	@echo "$(GREEN)✓ Databases ready$(NC)"

.PHONY: dev-stop
dev-stop:
	@docker compose -f docker/docker-compose.dev.yml down

# ------------------------------------------------------------------------------
# Database
# ------------------------------------------------------------------------------

.PHONY: db-init
db-init:
	@echo "$(CYAN)▶ Initializing ScyllaDB schema...$(NC)"
	@docker exec -i scylla cqlsh < $(SCHEMA_DIR)/cql/001_core_schema.cql 2>/dev/null || \
		cqlsh $(SCYLLA_HOST) $(SCYLLA_PORT) -f $(SCHEMA_DIR)/cql/001_core_schema.cql
	@echo "$(GREEN)✓ Schema initialized$(NC)"

.PHONY: db-shell
db-shell:
	@docker exec -it scylla cqlsh

.PHONY: redis-cli
redis-cli:
	@docker exec -it redis redis-cli

# ------------------------------------------------------------------------------
# Testing
# ------------------------------------------------------------------------------

.PHONY: test
test:
	@echo "$(CYAN)▶ Running tests...$(NC)"
	@$(CARGO) test --workspace --all-features
	@echo "$(GREEN)✓ All tests passed$(NC)"

.PHONY: test-unit
test-unit:
	@$(CARGO) test --workspace --lib

.PHONY: test-integration
test-integration:
	@$(CARGO) test --workspace --test '*'

# ------------------------------------------------------------------------------
# Linting
# ------------------------------------------------------------------------------

.PHONY: lint
lint: fmt-check clippy

.PHONY: fmt
fmt:
	@$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check:
	@$(CARGO) fmt --all -- --check

.PHONY: clippy
clippy:
	@echo "$(CYAN)▶ Running Clippy...$(NC)"
	@$(CARGO) clippy --workspace --all-features -- -D warnings
	@echo "$(GREEN)✓ Clippy passed$(NC)"

.PHONY: audit
audit:
	@$(CARGO) audit

# ------------------------------------------------------------------------------
# Documentation
# ------------------------------------------------------------------------------

.PHONY: docs
docs:
	@$(CARGO) doc --workspace --no-deps --document-private-items
	@echo "$(GREEN)✓ Docs: $(TARGET_DIR)/doc/drone_domain/index.html$(NC)"

.PHONY: docs-open
docs-open: docs
	@open $(TARGET_DIR)/doc/drone_domain/index.html 2>/dev/null || \
		xdg-open $(TARGET_DIR)/doc/drone_domain/index.html

# ------------------------------------------------------------------------------
# Docker
# ------------------------------------------------------------------------------

.PHONY: docker
docker: docker-api docker-frontend

.PHONY: docker-api
docker-api:
	@echo "$(CYAN)▶ Building API Docker image...$(NC)"
	@$(DOCKER) build -f docker/Dockerfile.api \
		-t $(DOCKER_REGISTRY)/$(PROJECT_NAME)-api:$(DOCKER_TAG) \
		-t $(DOCKER_REGISTRY)/$(PROJECT_NAME)-api:latest \
		--build-arg VERSION=$(VERSION) \
		--build-arg GIT_SHA=$(GIT_SHA) .
	@echo "$(GREEN)✓ API image built$(NC)"

.PHONY: docker-frontend
docker-frontend:
	@echo "$(CYAN)▶ Building frontend Docker image...$(NC)"
	@$(DOCKER) build -f docker/Dockerfile.frontend \
		-t $(DOCKER_REGISTRY)/$(PROJECT_NAME)-frontend:$(DOCKER_TAG) \
		-t $(DOCKER_REGISTRY)/$(PROJECT_NAME)-frontend:latest .
	@echo "$(GREEN)✓ Frontend image built$(NC)"

.PHONY: docker-up
docker-up:
	@docker compose -f docker/docker-compose.yml up -d
	@echo "$(GREEN)✓ Stack started$(NC)"
	@echo "  Frontend:  http://localhost:3000"
	@echo "  API:       http://localhost:8080/graphql"

.PHONY: docker-down
docker-down:
	@docker compose -f docker/docker-compose.yml down

# ------------------------------------------------------------------------------
# Production
# ------------------------------------------------------------------------------

.PHONY: prod
prod: clean lint test build-backend build-frontend
	@echo ""
	@echo "$(GREEN)╔══════════════════════════════════════════════════════════════════╗$(NC)"
	@echo "$(GREEN)║              PRODUCTION BUILD COMPLETE                           ║$(NC)"
	@echo "$(GREEN)╚══════════════════════════════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "  API Binary:  $(TARGET_DIR)/release/drone-graphql-api"
	@echo "  Frontend:    $(FRONTEND_DIR)/dist/"
	@echo "  Version:     $(VERSION) ($(GIT_SHA))"
	@echo ""

.PHONY: package
package: prod
	@echo "$(CYAN)▶ Creating distribution package...$(NC)"
	@mkdir -p $(DIST_DIR)
	@cp $(TARGET_DIR)/release/drone-graphql-api $(DIST_DIR)/
	@cp -r $(FRONTEND_DIR)/dist $(DIST_DIR)/frontend
	@cp -r $(SCHEMA_DIR) $(DIST_DIR)/
	@cp README.md $(DIST_DIR)/
	@cd $(DIST_DIR) && tar -czf $(PROJECT_NAME)-$(VERSION).tar.gz *
	@echo "$(GREEN)✓ Package: $(DIST_DIR)/$(PROJECT_NAME)-$(VERSION).tar.gz$(NC)"

.PHONY: zip
zip:
	@mkdir -p $(DIST_DIR)
	@zip -r $(DIST_DIR)/$(PROJECT_NAME)-$(VERSION).zip . \
		-x "target/*" -x ".git/*" -x "dist/*" -x "*.zip"
	@echo "$(GREEN)✓ Archive: $(DIST_DIR)/$(PROJECT_NAME)-$(VERSION).zip$(NC)"

# ------------------------------------------------------------------------------
# Cleanup
# ------------------------------------------------------------------------------

.PHONY: clean
clean:
	@echo "$(CYAN)▶ Cleaning...$(NC)"
	@$(CARGO) clean
	@rm -rf $(FRONTEND_DIR)/dist
	@rm -rf $(DIST_DIR)
	@echo "$(GREEN)✓ Clean$(NC)"

# ------------------------------------------------------------------------------
# Utilities
# ------------------------------------------------------------------------------

.PHONY: loc
loc:
	@tokei . --exclude target --exclude dist 2>/dev/null || \
		find . -name "*.rs" -not -path "./target/*" | xargs wc -l | tail -1

.PHONY: deps
deps:
	@$(CARGO) tree --workspace

.PHONY: version
version:
	@echo "Project:    $(PROJECT_NAME)"
	@echo "Version:    $(VERSION)"
	@echo "Git SHA:    $(GIT_SHA)"
	@echo "Build Time: $(BUILD_TIME)"
	@echo "Rust:       $$(rustc --version)"

# ------------------------------------------------------------------------------
# CI
# ------------------------------------------------------------------------------

.PHONY: ci
ci: check-deps lint test build
	@echo "$(GREEN)✓ CI pipeline complete$(NC)"

.PHONY: ci-full
ci-full: ci audit docker
	@echo "$(GREEN)✓ Full CI pipeline complete$(NC)"
