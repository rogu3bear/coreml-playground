# Development
.PHONY: dev dev-real check test fmt lint clean

dev:          ## Start development server with live reload (mock mode)
	COREML_MOCK=1 bash ./scripts/with-wasm-bindgen-cli.sh cargo leptos watch

dev-real:     ## Start development server with real CoreML backend
	bash ./scripts/with-wasm-bindgen-cli.sh cargo leptos watch

check:        ## Check both SSR and WASM targets
	cargo check --features ssr
	cargo check --features hydrate --target wasm32-unknown-unknown

test:         ## Run all tests
	COREML_MOCK=1 cargo test --features ssr

fmt:          ## Format all Rust code
	cargo fmt

lint:         ## Run clippy on both targets
	cargo clippy --features ssr -- -D warnings
	cargo clippy --features hydrate --target wasm32-unknown-unknown -- -D warnings

clean:        ## Clean build artifacts
	cargo clean
	rm -rf data/sessions.db
	rm -rf target/site

# Build
.PHONY: build build-release

build:        ## Build debug SSR binary
	bash ./scripts/with-wasm-bindgen-cli.sh cargo leptos build

build-release: ## Build optimized release binary
	bash ./scripts/with-wasm-bindgen-cli.sh cargo leptos build --release

# Setup
.PHONY: setup setup-wasm

setup:        ## Install all development dependencies
	rustup target add wasm32-unknown-unknown
	cargo install cargo-leptos
	@mkdir -p ~/CoreML-Models
	@mkdir -p data
	@echo "Setup complete! Run 'make dev' to start developing."

setup-wasm:   ## Install just the WASM target
	rustup target add wasm32-unknown-unknown

# Demo
.PHONY: demo demo-models

demo:         ## Run the demo setup and start the app
	./scripts/demo-setup.sh

demo-models:  ## Create the CoreML models directory
	mkdir -p ~/CoreML-Models
	@echo "Place your .mlmodel files in ~/CoreML-Models/"

# Help
.PHONY: help
help:         ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

.DEFAULT_GOAL := help
