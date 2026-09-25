# SDK Versioning and Distribution

## Overview

The Atomic Patent SDK is auto-generated from the OpenAPI specification served by the API server. This document defines how the SDK is versioned independently from the server crate, ensuring consumers can track SDK updates separately from server releases.

## SDK Versioning Scheme

### SemVer (Semantic Versioning)

SDKs follow **Semantic Versioning 2.0.0**:

```
SDK_VERSION = MAJOR.MINOR.PATCH

Examples: 1.0.0, 1.1.0, 2.0.0-alpha.1
```

### Version Compatibility Matrix

| SDK Version | API Version | Breaking Changes | Migration Effort | Support Status |
|---|---|---|---|---|
| 1.x.x | 1.0.0, 1.1.0 | None (backward compatible) | None | Active |
| 2.0.0 (future) | 2.0.0 | Yes (new endpoints, changed types) | Code changes required | Future |

### Version Independence

The SDK version is **independent** from both:

1. **Server Crate Version** (`api-server/Cargo.toml`)
   - Server may update from 0.1.0 → 0.2.0 without SDK version change
   - No consumer impact unless API behavior changes

2. **API Version** (`api-server/src/versioning.rs`)
   - API version controls URL paths (`/v1/`, `/v2/`) and deprecation policy
   - SDK version controls the stability and compatibility of generated code
   - A single SDK version may support multiple API versions (e.g., SDK 1.2.0 supports both `/v1/` and `/v1.1/` endpoints)

## SDK Generation and Publishing

### Automatic SDK Generation on Schema Change

**CI Step**: `.github/workflows/sdk-regenerate.yml` (to be added)

```yaml
name: Regenerate SDK on Schema Change

on:
  push:
    paths:
      - 'api-server/src/**'
      - '.github/workflows/sdk-regenerate.yml'
    branches:
      - main
      - develop

jobs:
  regenerate-sdk:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Generate SDK from OpenAPI spec
        run: |
          # Extract OpenAPI spec from running server or JSON file
          cargo build -p api-server --release
          # Run server in background and generate SDK
          cargo run -p api-server &
          sleep 5
          # Use openapi-generator-cli to regenerate SDKs
          ./scripts/generate-sdks.sh

      - name: Diff generated SDK against HEAD
        run: |
          git diff --stat
          # Fail if non-version-bumps detected
          ./scripts/validate-sdk-diff.sh

      - name: Comment on PR with SDK changes
        if: github.event_name == 'pull_request'
        run: |
          echo "## SDK Changes" >> $GITHUB_STEP_SUMMARY
          git diff HEAD -- sdk/ >> $GITHUB_STEP_SUMMARY || echo "No SDK changes"
```

### SDK Package Separation

The SDK is published independently for each target language:

#### TypeScript/JavaScript SDK

- **Package Name**: `@atomicip/client` (or similar)
- **Registry**: npm
- **Current Version**: 1.0.0 (from `api-server/src/sdk.rs::SdkConfig.package_version`)
- **Publishing**: Automated via CI when SDK changes and version is bumped

Example package.json:
```json
{
  "name": "@atomicip/client",
  "version": "1.0.0",
  "description": "Atomic Patent TypeScript Client - Auto-generated from OpenAPI",
  "main": "dist/index.js",
  "types": "dist/index.d.ts"
}
```

#### Python SDK

- **Package Name**: `atomicip-client`
- **Registry**: PyPI
- **Version**: 1.0.0
- **Publishing**: Automated via CI

Example setup.py:
```python
setup(
    name='atomicip-client',
    version='1.0.0',
    description='Atomic Patent Python Client - Auto-generated from OpenAPI',
    author='Atomic Patent Contributors',
    license='MIT',
)
```

#### Go SDK

- **Import Path**: `github.com/atomicip/atomicip-go`
- **Version**: 1.0.0 (via git tags)
- **Publishing**: Automated via CI, tagged as `sdk/go/v1.0.0`

#### Rust SDK

- **Crate Name**: `atomicip-client` (separate from `api-server`)
- **Registry**: crates.io
- **Version**: 1.0.0 (from Cargo.toml)
- **Publishing**: Automated via CI

Example Cargo.toml:
```toml
[package]
name = "atomicip-client"
version = "1.0.0"
description = "Atomic Patent Rust Client - Auto-generated from OpenAPI"
```

## Version Bumping Rules

### MAJOR (X.0.0)

Bump MAJOR version when:
- **API endpoints are removed** (breaking the client contract)
- **Response types change significantly** (e.g., field removed, type changed)
- **Authentication scheme changes** (e.g., OAuth → JWT)
- **Error response structure changes**

Example: When API v2.0.0 is released with `/v2/` endpoints and `/v1/` is deprecated

### MINOR (1.X.0)

Bump MINOR version when:
- **New endpoints are added** (backward compatible)
- **New optional fields are added to request/response types**
- **New SDK helper methods are added** (e.g., batch operation helpers)
- **Documentation updates** without code changes

Example: When a new `/v1/ip/batch-verify` endpoint is added

### PATCH (1.0.X)

Bump PATCH version when:
- **Bug fixes** in generated code or SDK helpers
- **Non-breaking dependency updates**
- **Documentation/comment improvements in SDK**
- **Performance improvements**

Example: Fixing a null-pointer bug in the TypeScript SDK

## SDK Change Detection

### Automated Diff Validation

File: `.github/scripts/validate-sdk-diff.sh`

The CI pipeline validates SDK changes:

```bash
#!/bin/bash
set -e

# Get diff of generated SDK files
DIFF=$(git diff HEAD -- sdk/)

# Check if version was bumped when code changes detected
if [ -n "$DIFF" ] && ! git diff HEAD -- sdk/package.json | grep -q '"version"'; then
    echo "❌ SDK code changed but version not bumped!"
    echo "Changes detected:"
    echo "$DIFF"
    exit 1
fi

echo "✅ SDK diff validation passed"
```

### Schema Change Detection

File: `.github/scripts/detect-schema-changes.sh`

Compares OpenAPI specs to determine version bump requirement:

```bash
#!/bin/bash

# Generate current spec from running server
CURRENT_SPEC=$(./target/release/api-server --export-openapi)

# Compare against last known good spec
if ! diff -q <(echo "$CURRENT_SPEC" | jq -S .) schema.baseline.json &>/dev/null; then
    echo "SCHEMA_CHANGED=true" >> $GITHUB_ENV
    
    # Detect breaking changes
    BREAKING=$(./scripts/openapi-breaking-changes.py schema.baseline.json)
    if [ -n "$BREAKING" ]; then
        echo "BREAKING_CHANGE=true" >> $GITHUB_ENV
    fi
fi
```

## Publishing Workflow

### Manual Publish (for patch versions)

```bash
# 1. Bump version in all SDK configs
./scripts/bump-sdk-version.sh patch

# 2. Regenerate SDKs
./scripts/generate-sdks.sh

# 3. Publish to registries
cargo publish -p atomicip-client          # Rust crate
npm publish @atomicip/client              # npm
python -m twine upload dist/*             # PyPI
git tag -a "sdk/go/v1.0.1" -m "Go SDK 1.0.1"  # GitHub release
```

### Automated Publish (on merge to main)

1. **PR Detection**: PR modifies `.github/workflows/`, `api-server/src/`, or `.github/scripts/`
2. **Schema Validation**: Validate that OpenAPI spec is well-formed
3. **SDK Regeneration**: Generate all SDKs in CI
4. **Version Bump**: Automatically bump PATCH on successful generation
5. **Publish**: On merge to main, publish all SDK packages

## SDK Compatibility Examples

### Example 1: Backward Compatible Addition

```
API v1.1.0: Adds GET /v1/ip/{ip_id}/history endpoint
SDK Update: 1.0.0 → 1.1.0 (MINOR bump)
  - New method: getIpHistory(ipId: number)
  - All existing methods unchanged
  - No consumer code changes required
```

### Example 2: Breaking Change

```
API v2.0.0: Changes /v1/ip/commit response from {ip_id} to {id, version}
SDK Update: 1.x.x → 2.0.0 (MAJOR bump)
  - Consumer must update property access: response.ip_id → response.id
  - Build fails without code update (type safety)
```

### Example 3: Bug Fix

```
SDK 1.0.1: Fixes memory leak in batch operation
SDK Update: 1.0.0 → 1.0.1 (PATCH bump)
  - No new features
  - No breaking changes
  - Drop-in replacement
```

## Versioning Constants

### Current Version Reference

**Server Crate**: `api-server/Cargo.toml` → `0.1.0`
**API Version**: `api-server/src/versioning.rs` → `CURRENT_VERSION = "1.0.0"`
**SDK Version**: `api-server/src/sdk.rs::SdkConfig` → `package_version = "1.0.0"`

### Updating SDK Version

All language SDKs maintain the **same version number** for consistency:

```
# TypeScript/JavaScript
sdk/ts/package.json: "version": "1.0.1"

# Python
sdk/python/setup.py: version='1.0.1'

# Go
git tag: sdk/go/v1.0.1

# Rust
sdk/rust/Cargo.toml: version = "1.0.1"
```

## Testing SDK Compatibility

### SDK Integration Tests

Path: `api-server/tests/sdk-generated-types.rs`

Verifies all generated SDKs correctly serialize/deserialize API responses:

```rust
#[tokio::test]
async fn test_sdk_typescript_response_parsing() {
    let response = client.get_ip(1).await.unwrap();
    
    // Verify SDK type matches API response
    assert_eq!(response.status, 200);
    assert!(response.data.is_some());
}

#[tokio::test]
async fn test_sdk_error_response_shape() {
    let response = client.get_ip(999999).await;
    
    // Verify error follows canonical shape
    assert!(matches!(response, Err(_)));
}
```

## Consumer Guidance

### Installing the SDK

**TypeScript/JavaScript**:
```bash
npm install @atomicip/client@^1.0.0
```

**Python**:
```bash
pip install atomicip-client~=1.0.0
```

**Go**:
```bash
go get github.com/atomicip/atomicip-go@v1.0.0
```

**Rust**:
```toml
[dependencies]
atomicip-client = "1.0"
```

### Version Pinning Strategy

- **Development**: Pin to exact version (e.g., `1.0.5`) for reproducibility
- **Library**: Allow minor updates (e.g., `^1.0.0` in npm, `~=1.0` in pip)
- **Production**: Pin to patch version or exact (e.g., `1.0.5`)

## References

- **Semantic Versioning**: https://semver.org/
- **OpenAPI Specification**: https://openapis.org/
- **API Versioning Policy**: See `api-server/src/versioning.rs`
- **SDK Generation**: See `api-server/src/sdk.rs`
