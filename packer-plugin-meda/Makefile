# Makefile for Packer + Meda image building and pushing

# Default variables
IMAGE_TAG ?= latest
REGISTRY ?= ghcr.io
ORGANIZATION ?= $(shell echo $$GITHUB_REPOSITORY_OWNER)
ifeq ($(ORGANIZATION),)
ORGANIZATION = cirunlabs
endif
TEMPLATE ?= ../images/ubuntu-base/template.pkr.hcl
export PATH := $(PATH):$(HOME)/go/bin
PUSH_ENABLED ?= true
DRY_RUN ?= false

# Plugin variables
PLUGIN_VERSION ?= 1.0.0
PLUGIN_BINARY = packer-plugin-meda
PLUGIN_INSTALL_PATH = ~/.config/packer/plugins/github.com/cirunlabs/meda/
MEDA_DIR = ..

.PHONY: help build-plugin install-plugin init validate build push dry-run clean all check-auth config

## Default target
all: build-plugin install-plugin validate build

## Show help
help:
	@echo "Packer + Meda Image Builder"
	@echo ""
	@echo "Usage:"
	@echo "  make all                    # Build plugin, install, and build image with push"
	@echo "  make build                  # Build and push image to GHCR"
	@echo "  make dry-run                # Build image with dry-run push (test only)"
	@echo "  make push                   # Build and push (same as build)"
	@echo "  make validate               # Validate Packer template"
	@echo "  make check-auth             # Check authentication requirements"
	@echo "  make config                 # Show current configuration"
	@echo ""
	@echo "Variables:"
	@echo "  IMAGE_TAG=$(IMAGE_TAG)"
	@echo "  REGISTRY=$(REGISTRY)"
	@echo "  ORGANIZATION=$(ORGANIZATION)"
	@echo "  TEMPLATE=$(TEMPLATE)"
	@echo "  PUSH_ENABLED=$(PUSH_ENABLED)"
	@echo ""
	@echo "Authentication:"
	@echo "  For GHCR push: export GITHUB_TOKEN=your_token"
	@echo "  Get token at: https://github.com/settings/tokens"
	@echo ""
	@echo "Examples:"
	@echo "  export GITHUB_TOKEN=ghp_xxxxxxxxxxxx"
	@echo "  make build IMAGE_TAG=v1.0.0 ORGANIZATION=myorg"
	@echo "  make dry-run"
	@echo "  make build PUSH_ENABLED=false"

## Build the Packer plugin
build-plugin:
	@echo "Building Packer plugin..."
	go generate . && go mod tidy && go build -o $(PLUGIN_BINARY) .

## Install the Packer plugin
install-plugin: build-plugin
	@echo "Installing Packer plugin..."
	packer plugins install --path ./$(PLUGIN_BINARY) github.com/cirunlabs/meda

## Initialize Packer plugins
init: install-plugin
	@echo "Initializing Packer plugins..."
	packer init $(TEMPLATE)

## Validate Packer template
validate: install-plugin
	@echo "Validating Packer template..."
	packer validate \
		-var="image_tag=$(IMAGE_TAG)" \
		-var="registry=$(REGISTRY)" \
		-var="organization=$(ORGANIZATION)" \
		-var="push_enabled=$(PUSH_ENABLED)" \
		-var="dry_run=$(DRY_RUN)" \
		$(TEMPLATE)

## Build and push image to registry
build: validate check-auth
	@echo "Building and pushing image..."
	@echo "Target: $(REGISTRY)/$(ORGANIZATION)/ubuntu-base:$(IMAGE_TAG)"
	packer build \
		-var="image_tag=$(IMAGE_TAG)" \
		-var="registry=$(REGISTRY)" \
		-var="organization=$(ORGANIZATION)" \
		-var="push_enabled=$(PUSH_ENABLED)" \
		-var="dry_run=$(DRY_RUN)" \
		$(TEMPLATE)

## Build and push (alias for build)
push: build

## Dry run - build without actually pushing
dry-run:
	@echo "Running dry-run build (no actual push)..."
	$(MAKE) build DRY_RUN=true

## Build without pushing to registry (local only)
local:
	@echo "Building local image only (no push)..."
	$(MAKE) build PUSH_ENABLED=false

## Quick build (skip validation)
quick:
	@echo "Quick build (skipping validation)..."
	packer build \
		-var="image_tag=$(IMAGE_TAG)" \
		-var="registry=$(REGISTRY)" \
		-var="organization=$(ORGANIZATION)" \
		-var="push_enabled=$(PUSH_ENABLED)" \
		-var="dry_run=$(DRY_RUN)" \
		$(TEMPLATE)

## Clean up build artifacts
clean:
	@echo "Cleaning up build artifacts..."
	rm -f ../manifest.json
	rm -f $(PLUGIN_BINARY)
	# Clean up any leftover VMs
	-cd $(MEDA_DIR) && cargo run -- list --json 2>/dev/null | jq -r '.[].name' | grep -E '^packer-' | xargs -r -I {} cargo run -- delete {} --force
	# Clean up any test images
	-cd $(MEDA_DIR) && cargo run -- images list --json 2>/dev/null | jq -r '.[].name' | grep -E 'test|packer' | xargs -r -I {} cargo run -- rmi {}

## Build specific templates
ubuntu-minimal:
	$(MAKE) build TEMPLATE=../images/ubuntu-minimal/template.pkr.hcl

ubuntu-docker:
	$(MAKE) build TEMPLATE=../images/ubuntu-docker/template.pkr.hcl

ubuntu-base:
	$(MAKE) build TEMPLATE=../images/ubuntu-base/template.pkr.hcl

## Development helpers
dev-setup: build-plugin install-plugin
	@echo "Development environment ready!"

dev-test:
	$(MAKE) dry-run IMAGE_TAG=dev-test

## Show current configuration
config:
	@echo "Current configuration:"
	@echo "  IMAGE_TAG: $(IMAGE_TAG)"
	@echo "  REGISTRY: $(REGISTRY)"
	@echo "  ORGANIZATION: $(ORGANIZATION)"
	@echo "  TEMPLATE: $(TEMPLATE)"
	@echo "  PUSH_ENABLED: $(PUSH_ENABLED)"
	@echo "  DRY_RUN: $(DRY_RUN)"
	@echo "  Target image: $(REGISTRY)/$(ORGANIZATION)/ubuntu-base:$(IMAGE_TAG)"
	@echo ""
	@echo "Authentication:"
	@if [ "$(PUSH_ENABLED)" = "true" ] && [ "$(REGISTRY)" = "ghcr.io" ]; then \
		if [ -z "$$GITHUB_TOKEN" ]; then \
			echo "  ❌ GITHUB_TOKEN: Not set (required for GHCR push)"; \
			echo "  💡 Set with: export GITHUB_TOKEN=your_token"; \
		else \
			echo "  ✅ GITHUB_TOKEN: Set"; \
		fi \
	else \
		echo "  ℹ️  GITHUB_TOKEN: Not required (push disabled or non-GHCR registry)"; \
	fi

## Check authentication requirements
check-auth:
	@echo "Checking authentication requirements..."
	@if [ "$(PUSH_ENABLED)" = "true" ] && [ "$(REGISTRY)" = "ghcr.io" ]; then \
		if [ -z "$$GITHUB_TOKEN" ]; then \
			echo "❌ ERROR: GITHUB_TOKEN environment variable is required for pushing to GHCR"; \
			echo "💡 Please set it with: export GITHUB_TOKEN=your_token"; \
			echo "🔗 Get token at: https://github.com/settings/tokens"; \
			exit 1; \
		else \
			echo "✅ GITHUB_TOKEN is set for GHCR authentication"; \
		fi \
	else \
		echo "ℹ️  No authentication required (push disabled or non-GHCR registry)"; \
	fi