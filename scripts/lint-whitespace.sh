#!/bin/bash

# Script to check for and optionally fix trailing whitespace issues
# Usage:
#   ./lint-whitespace.sh         # Check only
#   ./lint-whitespace.sh --fix   # Check and fix

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

FIX_MODE=false
if [[ "$1" == "--fix" ]]; then
    FIX_MODE=true
fi

echo -e "${BLUE}🔍 Checking for trailing whitespace...${NC}"

# File patterns to check
PATTERNS=(
    "*.rs"
    "*.go"
    "*.md"
    "*.yml"
    "*.yaml"
    "*.json"
    "*.toml"
    "*.sh"
    "*.hcl"
    "*.pkr.hcl"
    "Makefile"
)

found_issues=false
files_with_issues=()

# Check each pattern
for pattern in "${PATTERNS[@]}"; do
    while IFS= read -r -d '' file; do
        if [[ -f "$file" ]] && grep -q '[[:space:]]$' "$file"; then
            found_issues=true
            files_with_issues+=("$file")

            if [[ "$FIX_MODE" == "true" ]]; then
                echo -e "${YELLOW}Fixing:${NC} $file"
                sed -i 's/[[:space:]]*$//' "$file"
            else
                echo -e "${RED}Found trailing whitespace:${NC} $file"
                # Show the lines with trailing whitespace
                grep -n '[[:space:]]$' "$file" | head -3 | while IFS= read -r line; do
                    echo -e "  ${YELLOW}${line}${NC}"
                done
            fi
        fi
    done < <(find . -name "$pattern" -not -path "./.git/*" -not -path "./target/*" -print0)
done

echo ""

if [[ "$found_issues" == "true" ]]; then
    if [[ "$FIX_MODE" == "true" ]]; then
        echo -e "${GREEN}✅ Fixed trailing whitespace in ${#files_with_issues[@]} files${NC}"
    else
        echo -e "${RED}❌ Found trailing whitespace in ${#files_with_issues[@]} files${NC}"
        echo -e "${BLUE}💡 Run with --fix to automatically remove trailing whitespace:${NC}"
        echo -e "   ./scripts/lint-whitespace.sh --fix"
        exit 1
    fi
else
    echo -e "${GREEN}✅ No trailing whitespace found!${NC}"
fi