#!/bin/bash

# Quick one-liner to remove trailing whitespace from common file types
# Usage: ./quick-whitespace-clean.sh

echo "🧹 Quick trailing whitespace cleanup..."

# Find and fix trailing whitespace in one go
find . \
    -type f \
    -not -path "./.git/*" \
    -not -path "./target/*" \
    \( -name "*.rs" -o -name "*.go" -o -name "*.md" -o -name "*.yml" -o -name "*.yaml" -o -name "*.json" -o -name "*.toml" -o -name "*.sh" -o -name "*.hcl" -o -name "Makefile" \) \
    -exec sed -i 's/[[:space:]]*$//' {} +

echo "✅ Done! All trailing whitespaces removed."