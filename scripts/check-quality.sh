#!/bin/bash
set -e

# Check command line arguments
WITH_INTEGRATION=false
if [[ "$1" == "--with-integration" ]]; then
    WITH_INTEGRATION=true
    echo "🔍 Running code quality checks (including integration tests)..."
elif [[ "$1" == "--help" || "$1" == "-h" ]]; then
    echo "Code Quality Check Script"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --with-integration    Include slow integration tests"
    echo "  --help, -h           Show this help message"
    echo ""
    echo "This script runs comprehensive code quality checks including:"
    echo "- Code formatting (rustfmt)"
    echo "- Linting (clippy)"
    echo "- Documentation builds"
    echo "- Unit tests (integration tests with --with-integration)"
    echo "- Security audit (if cargo-audit is installed)"
    echo "- Dependency analysis (if cargo-machete is installed)"
    exit 0
elif [[ -n "$1" ]]; then
    echo "Unknown option: $1"
    echo "Use --help for usage information"
    exit 1
else
    echo "🔍 Running code quality checks (excluding integration tests)..."
    echo "   Use --with-integration to include integration tests"
fi
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
if [ "$WITH_INTEGRATION" = true ]; then
    echo "🧪 Running all tests (including integration)..."
    cargo test --quiet
    check_status "All tests"
else
    echo "🧪 Running unit tests..."
    cargo test --quiet --exclude integration_tests 2>/dev/null || cargo test --quiet --bin meda
    check_status "Unit tests"
fi

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
if [ "$WITH_INTEGRATION" = true ]; then
    echo -e "${GREEN}🎉 All quality checks passed (including integration tests)!${NC}"
else
    echo -e "${GREEN}🎉 All quality checks passed!${NC}"
    echo "Integration tests were skipped for speed. Use --with-integration to include them."
fi
echo "Your code is ready for commit and will pass CI checks."