#!/usr/bin/env bash
set -euo pipefail

# CoreML Studio Demo Setup
# ========================
# This script prepares a clean demo environment.

MODELS_DIR="${COREML_MODELS_DIR:-$HOME/CoreML-Models}"
DB_DIR="$HOME/.coreml-playground"

echo "CoreML Studio Demo Setup"
echo "========================"
echo ""

# 1. Ensure models directory exists
echo "1. Checking models directory..."
mkdir -p "$MODELS_DIR"
echo "   Models directory: $MODELS_DIR"

# 2. Check for models
MODEL_COUNT=$(find "$MODELS_DIR" -name "*.mlmodel" -o -name "*.mlmodelc" 2>/dev/null | wc -l | tr -d ' ')
if [ "$MODEL_COUNT" -eq 0 ]; then
    echo "   No models found. The built-in Echo Demo model will be available."
    echo "   To add models, place .mlmodel files in $MODELS_DIR"
else
    echo "   Found $MODEL_COUNT model(s)"
fi

# 3. Check database
echo ""
echo "2. Checking database..."
if [ -f "$DB_DIR/sessions.db" ]; then
    echo "   Existing database found at $DB_DIR/sessions.db"
    read -p "   Reset database for demo? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -f "$DB_DIR/sessions.db"
        echo "   Database reset."
    fi
else
    echo "   No existing database (clean start)"
fi

# 4. Check prerequisites
echo ""
echo "3. Checking prerequisites..."

if command -v cargo &>/dev/null; then
    echo "   Rust: $(rustc --version)"
else
    echo "   ERROR: Rust not found. Install from https://rustup.rs"
    exit 1
fi

if rustup target list --installed | grep -q wasm32-unknown-unknown; then
    echo "   WASM target: installed"
else
    echo "   WASM target: not installed (run 'make setup-wasm')"
fi

if command -v cargo-leptos &>/dev/null; then
    echo "   cargo-leptos: installed"
else
    echo "   cargo-leptos: not installed (run 'cargo install cargo-leptos')"
fi

echo ""
echo "Setup complete! Run 'make dev' or 'cargo leptos watch' to start."
echo "Then open http://127.0.0.1:3100 in your browser."
