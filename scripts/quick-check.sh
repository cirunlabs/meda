#!/bin/bash
set -e

echo "⚡ Running quick code quality checks..."
echo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

check_status() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ $1 passed${NC}"
    else
        echo -e "${RED}❌ $1 failed${NC}"
        exit 1
    fi
}

# Check formatting
echo "📝 Checking code formatting..."
cargo fmt --check
check_status "Formatting"

# Check compilation
echo "🔧 Checking compilation..."
cargo check --quiet
check_status "Compilation"

# Run Clippy with strict settings
echo "🔍 Running Clippy linting..."
cargo clippy --all-targets --all-features --quiet -- -D warnings
check_status "Clippy linting"

# Check documentation builds
echo "📚 Checking documentation..."
cargo doc --no-deps --document-private-items --quiet
check_status "Documentation"

# Run tests but skip integration tests (faster)
echo "🧪 Running tests (excluding integration)..."
cargo test --quiet --exclude integration_tests 2>/dev/null || cargo test --quiet --bin meda
check_status "Tests"

# Check for trailing whitespace
echo "🔎 Checking for trailing whitespace..."
if grep -r '[[:space:]]$' src/ 2>/dev/null; then
    echo -e "${RED}❌ Found trailing whitespace!${NC}"
    exit 1
else
    echo -e "${GREEN}✅ No trailing whitespace found${NC}"
fi

echo
echo -e "${GREEN}⚡ Quick quality checks passed!${NC}"
echo "Run ./scripts/check-quality.sh for full checks including integration tests."