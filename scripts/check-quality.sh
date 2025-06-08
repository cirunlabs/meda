#!/bin/bash
set -e

echo "🔍 Running code quality checks..."
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
cargo check
check_status "Compilation"

# Run Clippy with strict settings
echo "🔍 Running Clippy linting..."
cargo clippy --all-targets --all-features -- -D warnings
check_status "Clippy linting"

# Check documentation builds
echo "📚 Checking documentation..."
cargo doc --no-deps --document-private-items --quiet
check_status "Documentation"

# Run tests
echo "🧪 Running tests..."
cargo test --quiet
check_status "Tests"

# Check for trailing whitespace (if grep is available)
echo "🔎 Checking for trailing whitespace..."
if command -v grep &> /dev/null; then
    if grep -r '[[:space:]]$' src/ 2>/dev/null; then
        echo -e "${RED}❌ Found trailing whitespace!${NC}"
        exit 1
    else
        echo -e "${GREEN}✅ No trailing whitespace found${NC}"
    fi
else
    echo -e "${YELLOW}⚠️  grep not available, skipping trailing whitespace check${NC}"
fi

# Optional: Run security audit if cargo-audit is installed
echo "🛡️  Checking security audit..."
if command -v cargo-audit &> /dev/null; then
    cargo audit
    check_status "Security audit"
else
    echo -e "${YELLOW}⚠️  cargo-audit not installed, skipping security check${NC}"
    echo "   Install with: cargo install cargo-audit"
fi

# Optional: Check for unused dependencies if cargo-machete is installed
echo "🧹 Checking for unused dependencies..."
if command -v cargo-machete &> /dev/null; then
    cargo machete
    check_status "Unused dependencies check"
else
    echo -e "${YELLOW}⚠️  cargo-machete not installed, skipping unused dependency check${NC}"
    echo "   Install with: cargo install cargo-machete"
fi

echo
echo -e "${GREEN}🎉 All quality checks passed!${NC}"
echo "Your code is ready for commit and will pass CI checks."