#!/bin/bash

# Script to remove trailing whitespaces from all text files in the project

set -e

echo "🧹 Removing trailing whitespaces from all files..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counter for modified files
modified_count=0

# Find all text files, excluding:
# - .git directory
# - target directory (Rust build)
# - node_modules (if any)
# - binary files
# - this script itself

while IFS= read -r -d '' file; do
    # Check if file has trailing whitespace
    if grep -q '[[:space:]]$' "$file"; then
        echo -e "${YELLOW}Cleaning:${NC} $file"

        # Remove trailing whitespace
        # Use sed -i with backup, then remove backup
        sed -i.bak 's/[[:space:]]*$//' "$file"
        rm -f "${file}.bak"

        ((modified_count++))
    fi
done < <(find . \
    -type f \
    -not -path "./.git/*" \
    -not -path "./target/*" \
    -not -path "./node_modules/*" \
    -not -path "*/\.DS_Store" \
    -not -name "*.png" \
    -not -name "*.jpg" \
    -not -name "*.jpeg" \
    -not -name "*.gif" \
    -not -name "*.ico" \
    -not -name "*.pdf" \
    -not -name "*.zip" \
    -not -name "*.tar" \
    -not -name "*.gz" \
    -not -name "*.bz2" \
    -not -name "*.7z" \
    -not -name "*.bin" \
    -not -name "*.exe" \
    -not -name "*.dll" \
    -not -name "*.so" \
    -not -name "*.dylib" \
    -not -name "*.lock" \
    -not -name "*.sum" \
    \( -name "*.rs" -o \
    -name "*.go" -o \
    -name "*.md" -o \
    -name "*.yml" -o \
    -name "*.yaml" -o \
    -name "*.json" -o \
    -name "*.toml" -o \
    -name "*.sh" -o \
    -name "*.bash" -o \
    -name "*.zsh" -o \
    -name "*.fish" -o \
    -name "*.py" -o \
    -name "*.js" -o \
    -name "*.ts" -o \
    -name "*.jsx" -o \
    -name "*.tsx" -o \
    -name "*.html" -o \
    -name "*.css" -o \
    -name "*.scss" -o \
    -name "*.sass" -o \
    -name "*.xml" -o \
    -name "*.txt" -o \
    -name "*.hcl" -o \
    -name "*.pkr.hcl" -o \
    -name "*.tf" -o \
    -name "Makefile" -o \
    -name "Dockerfile" -o \
    -name ".gitignore" -o \
    -name ".env*" -o \
    -name "LICENSE" -o \
    -name "README" \) \
    -print0)

echo -e "\n${GREEN}✅ Done!${NC}"
echo -e "Modified ${GREEN}$modified_count${NC} files"

# Optional: Show git diff summary
if command -v git &> /dev/null && [ -d .git ]; then
    echo -e "\n📊 Git status:"
    git diff --stat
fi