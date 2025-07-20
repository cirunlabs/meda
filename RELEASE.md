# Release Process

Meda uses an automated release process with GoReleaser for cross-platform builds and GitHub Releases.

## Prerequisites

Before creating a release, ensure:

1. All quality checks pass locally:
   ```bash
   ./scripts/check-quality.sh
   ```

2. All tests pass (including integration tests):
   ```bash
   ./scripts/check-quality.sh --with-integration
   ```

3. Update version in `Cargo.toml` if needed:
   ```toml
   [package]
   version = "0.2.0"  # Update version number
   ```

4. Update CHANGELOG.md or create release notes

## Local Testing

To test the release process locally without publishing:

```bash
# Install GoReleaser if you haven't already
go install github.com/goreleaser/goreleaser@latest

# Test cross-compilation and packaging locally
goreleaser release --snapshot --clean

# The artifacts will be available in the dist/ directory
ls -la dist/
```

## Creating a Release

### Option 1: GitHub UI (Recommended)

1. Go to [GitHub Releases](https://github.com/cirunlabs/meda/releases)
2. Click "Create a new release"
3. Choose a tag version (e.g., `v0.2.0`) - GitHub will create it
4. Add release title and description
5. Click "Publish release"

This triggers the automated release workflow.

### Option 2: Command Line

1. Ensure all changes are committed and pushed:
   ```bash
   git add .
   git commit -m "Prepare release v0.2.0"
   git push origin main
   ```

2. Create and push a new tag:
   ```bash
   git tag -a v0.2.0 -m "Release v0.2.0

   Features:
   - Auto-pull missing images when running VMs
   - Enhanced REST API documentation
   - Improved error handling

   Bug fixes:
   - Fixed integration test expectations
   - Improved code quality checks"

   git push origin v0.2.0
   ```

## Automated Release Process

When a tag is pushed, GitHub Actions automatically:

1. **Quality Checks**: Runs all linting, formatting, and tests
2. **Cross-Compilation**: Builds for multiple platforms:
   - Linux (x86_64, aarch64)
   - macOS (x86_64, aarch64)
   - Windows (x86_64)
3. **Packaging**: Creates release archives with binaries
4. **GitHub Release**: Creates release with:
   - Release notes from tag message
   - Pre-built binaries for all platforms
   - Checksums and signatures
5. **Artifacts**: Uploads build artifacts

## Release Artifacts

Each release includes:

- `meda-{version}-linux-amd64.tar.gz` - Linux x86_64 binary
- `meda-{version}-linux-arm64.tar.gz` - Linux ARM64 binary
- `meda-{version}-darwin-amd64.tar.gz` - macOS Intel binary
- `meda-{version}-darwin-arm64.tar.gz` - macOS Apple Silicon binary
- `meda-{version}-windows-amd64.zip` - Windows x86_64 binary
- `checksums.txt` - SHA256 checksums for all artifacts

## Manual Release Steps (if automation fails)

If the automated process fails:

1. **Build locally**:
   ```bash
   # Build release binary
   cargo build --release

   # Create archive
   tar -czf meda-v0.2.0-linux-amd64.tar.gz -C target/release meda
   ```

2. **Create GitHub release manually**:
   - Go to GitHub Releases
   - Create new release with tag
   - Upload the binary archive
   - Add release notes

## Troubleshooting

### Release Workflow Fails

1. Check [GitHub Actions](https://github.com/cirunlabs/meda/actions) for error logs
2. Common issues:
   - Quality checks failing → Fix code issues and re-tag
   - GoReleaser config errors → Check `.goreleaser.yaml`
   - Permission issues → Ensure `GITHUB_TOKEN` has proper permissions

### Test Release Locally

```bash
# Debug mode for detailed output
goreleaser release --snapshot --clean --debug

# Check specific build targets
goreleaser build --snapshot --single-target
```

### Version Issues

- Ensure version in `Cargo.toml` matches tag version
- Use semantic versioning (v1.2.3)
- Don't reuse tag names - create new version

## Post-Release Checklist

After successful release:

1. **Verify release artifacts** on GitHub
2. **Test download and installation**:
   ```bash
   # Test installation from release
   curl -L https://github.com/cirunlabs/meda/releases/download/v0.2.0/meda-v0.2.0-linux-amd64.tar.gz | tar -xz
   ./meda --version
   ```
3. **Update documentation** if needed
4. **Announce release** in relevant channels
5. **Start planning next release** with feature roadmap

## Release Schedule

- **Patch releases** (v0.1.1): Bug fixes, urgent issues
- **Minor releases** (v0.2.0): New features, enhancements
- **Major releases** (v1.0.0): Breaking changes, major milestones

Consider releasing when:
- Significant new features are ready
- Important bug fixes accumulate
- Monthly cadence for active development
