# Release Process

## Local Testing

To test the release process locally without publishing to GitHub:

```bash
# Install GoReleaser if you haven't already
go install github.com/goreleaser/goreleaser@latest

# Run GoReleaser in "snapshot" mode (doesn't publish, creates local artifacts)
goreleaser release --snapshot --clean

# The artifacts will be available in the dist/ directory
ls -la dist/
```

## Creating a Release

To create an official release:

1. Update your code and commit changes
2. Create and push a new tag:
```bash
git tag -a v0.1.0 -m "First release"
git push origin v0.1.0
```

3. The GitHub Actions workflow will automatically:
   - Build the project
   - Create a GitHub release
   - Upload the artifacts

## Troubleshooting

If you encounter issues with the release process:

1. Check the GitHub Actions logs for errors
2. Verify that your `.goreleaser.yaml` configuration is correct
3. Test locally with `--debug` flag:
```bash
goreleaser release --snapshot --clean --debug
```

## Release Notes

When creating a tag with `git tag -a`, you can include release notes in the tag message.
Alternatively, you can edit the release notes on GitHub after the release is created.
