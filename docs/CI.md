# Continuous Integration (CI) Setup

This document describes the CI/CD pipeline setup for the Meda project using GitHub Actions.

## Workflows

### 1. Build and Test (`rust.yml`)

Runs on every push and pull request to the `main` branch.

**Jobs:**
- **Check**: Fast compilation check using `cargo check`
- **Build**: Builds both debug and release versions
- **Test**: Runs unit and integration tests with proper VM dependencies

**Features:**
- Rust dependency caching for faster builds
- System dependency installation (qemu-utils, genisoimage, etc.)
- KVM access configuration for VM testing
- Separate unit and integration test execution

### 2. Lint and Format (`lint.yml`)

Comprehensive code quality checks that run on every push and pull request.

**Jobs:**

#### Code Quality Checks (`lint`)
- **Clippy linting**: All warnings treated as errors (`-D warnings`)
- **Documentation**: Verifies all docs build correctly
- **Security audit**: Checks for known vulnerabilities using `cargo audit`
- **Dependency analysis**: Identifies unused dependencies with `cargo machete`
- **TODO/FIXME tracking**: Reports untracked TODO comments

#### Formatting Check (`format-check`)
- **Rust formatting**: Enforces consistent code style with `rustfmt`
- **Trailing whitespace**: Detects and fails on trailing whitespace
- **Line endings**: Ensures proper Unix line endings (LF) and valid text encoding
  - Accepts ASCII text, UTF-8 text, and files detected as "C source, ASCII text"
  - Rejects Windows line endings (CRLF) and binary content

#### Spell Check (`spell-check`)
- **Comment spelling**: Checks spelling in Rust code comments
- **Documentation spelling**: Checks spelling in Markdown files
- **Non-blocking**: Reports issues but doesn't fail the build

#### Security Checks (`security`)
- **Security audit**: Comprehensive dependency vulnerability scanning
- **Secret detection**: Scans for potential hardcoded secrets
- **Non-blocking warnings**: Reports concerns without failing builds

#### Code Coverage (`code-coverage`)
- **Coverage generation**: Uses `cargo tarpaulin` for coverage reports
- **Codecov integration**: Uploads coverage reports (requires `CODECOV_TOKEN`)
- **Optional**: Continues even if coverage generation fails

#### Quality Summary (`summary`)
- **Summary report**: Provides consolidated status of all quality checks
- **GitHub Step Summary**: Displays results in PR interface
- **Failure conditions**: Fails only on critical issues (lint/format)

## Configuration

### Required Secrets (Optional)
- `CODECOV_TOKEN`: For code coverage reporting to Codecov

### Cache Strategy
The workflows use GitHub Actions cache to speed up builds:
- Separate cache keys for different job types (check, build, test, lint)
- Includes Cargo registry, git dependencies, and target directory
- Fallback cache keys for partial matches

## Code Quality Standards

### Mandatory Checks (Will Fail PR)
- All Clippy warnings must be resolved
- Code must be properly formatted with `rustfmt`
- No trailing whitespace allowed
- Files must have consistent UTF-8 encoding

### Advisory Checks (Will Warn)
- Spelling issues in comments and documentation
- Security vulnerabilities in dependencies
- Potential hardcoded secrets
- Code coverage metrics

## Local Development

To run the same checks locally before pushing:

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run Clippy with same settings as CI
cargo clippy --all-targets --all-features -- -D warnings

# Run security audit
cargo install cargo-audit
cargo audit

# Check for unused dependencies
cargo install cargo-machete
cargo machete

# Generate code coverage
cargo install cargo-tarpaulin
cargo tarpaulin --out html --output-dir ./coverage
```

## Best Practices

1. **Pre-commit**: Run `cargo fmt` and `cargo clippy` before committing
2. **Documentation**: Ensure all public functions have proper documentation
3. **Testing**: Write tests for new functionality
4. **Security**: Regularly update dependencies and review audit reports
5. **Comments**: Write clear, well-spelled comments for complex logic

## Troubleshooting

### Common CI Failures

1. **Clippy warnings**: Fix all warnings or use `#[allow(...)]` for intentional cases
2. **Format issues**: Run `cargo fmt` locally
3. **Line ending issues**: 
   - Convert Windows line endings: `dos2unix src/*.rs`
   - Check encoding: `file src/*.rs` (should show ASCII or UTF-8 text)
   - Files detected as "C source, ASCII text" are acceptable for Rust files
4. **Test failures**: Ensure tests pass locally with system dependencies installed
5. **Cache issues**: Clear cache by updating `Cargo.lock` or changing cache keys

### Performance

- First run after dependency changes may be slow due to cache misses
- Subsequent runs should be faster due to aggressive caching
- Parallel job execution minimizes total CI time

## Monitoring

The CI setup provides comprehensive monitoring through:
- GitHub Actions status checks on PRs
- Step summaries with quality metrics
- Optional integration with external services (Codecov)
- Detailed logs for debugging failures