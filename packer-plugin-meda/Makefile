PLUGIN_NAME = packer-plugin-meda
VERSION ?= 1.0.0
GORELEASER_VERSION = 1.21.2

# Go build flags
GO_BUILD_FLAGS = -ldflags="-s -w -X main.Version=$(VERSION)"

# Default target
.PHONY: all
all: build

# Build the plugin
.PHONY: build
build:
	go mod tidy
	go build $(GO_BUILD_FLAGS) -o $(PLUGIN_NAME)

# Install the plugin locally
.PHONY: install
install: build
	packer plugins install --path $(PLUGIN_NAME) github.com/cirunlabs/meda

# Test the plugin
.PHONY: test
test:
	go test -v ./...

# Format code
.PHONY: fmt
fmt:
	go fmt ./...

# Lint code
.PHONY: lint
lint:
	golangci-lint run

# Clean build artifacts
.PHONY: clean
clean:
	rm -f $(PLUGIN_NAME)
	rm -rf dist/

# Development setup
.PHONY: dev-setup
dev-setup:
	go mod tidy
	go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest

# Build for multiple platforms
.PHONY: build-all
build-all:
	GOOS=linux GOARCH=amd64 go build $(GO_BUILD_FLAGS) -o dist/$(PLUGIN_NAME)_linux_amd64
	GOOS=linux GOARCH=arm64 go build $(GO_BUILD_FLAGS) -o dist/$(PLUGIN_NAME)_linux_arm64
	GOOS=darwin GOARCH=amd64 go build $(GO_BUILD_FLAGS) -o dist/$(PLUGIN_NAME)_darwin_amd64
	GOOS=darwin GOARCH=arm64 go build $(GO_BUILD_FLAGS) -o dist/$(PLUGIN_NAME)_darwin_arm64
	GOOS=windows GOARCH=amd64 go build $(GO_BUILD_FLAGS) -o dist/$(PLUGIN_NAME)_windows_amd64.exe

# Create release with goreleaser
.PHONY: release
release:
	goreleaser release --rm-dist

# Create snapshot release
.PHONY: snapshot
snapshot:
	goreleaser release --snapshot --rm-dist

# Install goreleaser
.PHONY: install-goreleaser
install-goreleaser:
	go install github.com/goreleaser/goreleaser@v$(GORELEASER_VERSION)

# Validate plugin with sample template
.PHONY: validate
validate: build install
	cd ../images/ubuntu-minimal && packer validate template.pkr.hcl

# Help
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build        - Build the plugin binary"
	@echo "  install      - Install the plugin locally"
	@echo "  test         - Run tests"
	@echo "  fmt          - Format code"
	@echo "  lint         - Lint code"
	@echo "  clean        - Clean build artifacts"
	@echo "  dev-setup    - Set up development environment"
	@echo "  build-all    - Build for multiple platforms"
	@echo "  release      - Create release with goreleaser"
	@echo "  snapshot     - Create snapshot release"
	@echo "  validate     - Validate plugin with sample template"
	@echo "  help         - Show this help message"