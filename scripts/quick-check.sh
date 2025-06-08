#!/bin/bash
set -e

# Check command line arguments
if [[ "$1" == "--help" || "$1" == "-h" ]]; then
    echo "Quick Code Quality Check Script"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help, -h           Show this help message"
    echo ""
    echo "This script runs fast code quality checks including:"
    echo "- Code formatting (rustfmt)"
    echo "- Linting (clippy)"
    echo "- Documentation builds"
    echo "- Unit tests only (fast)"
    echo "- Whitespace and line ending checks"
    echo ""
    echo "For additional security and dependency checks, use:"
    echo "  ./scripts/check-quality.sh"
    exit 0
elif [[ -n "$1" ]]; then
    echo "Unknown option: $1"
    echo "Use --help for usage information"
    exit 1
fi

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

# Check line endings
echo "📄 Checking line endings..."
if command -v file &> /dev/null; then
    if file src/*.rs | grep -E -v '(ASCII text|UTF-8 Unicode text|Unicode text, UTF-8 text|C source, ASCII text)$'; then
        echo -e "${RED}❌ Found non-text files or binary content!${NC}"
        exit 1
    elif grep -l $'\r$' src/*.rs 2>/dev/null; then
        echo -e "${RED}❌ Found Windows line endings (CRLF)!${NC}"
        exit 1
    else
        echo -e "${GREEN}✅ All files have proper line endings${NC}"
    fi
else
    echo -e "${YELLOW}⚠️  file command not available, skipping line ending check${NC}"
fi

echo
echo -e "${GREEN}⚡ Quick quality checks passed!${NC}"
echo "Run ./scripts/check-quality.sh for additional security/dependency checks."
echo "Run ./scripts/check-quality.sh --with-integration for full integration tests."