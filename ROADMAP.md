# Tooler Roadmap

## Status Legend

- 🔴 **Critical**: Security vulnerabilities requiring immediate attention
- 🟠 **High**: Significant functionality or architectural issues  
- 🟡 **Medium**: Improvements and enhancements
- 🟢 **Low**: Nice-to-have features
- ✅ **Complete**: Finished items

---

## Critical Priority (Security)

### 🔴 Path Traversal in Tar Archive Extraction
- **File**: `src/download.rs` (lines 138-174)
- **Issue**: ZIP extraction has path traversal protection, but tar.gz/tar.xz do not
- **Risk**: Malicious archives can write files outside intended directory (e.g., `../../../.bashrc`)
- **Fix**: Implement same path validation for tar archives as exists for ZIP

### 🔴 Install Script Lacks Verification  
- **File**: `install.sh`
- **Issue**: Downloads and executes binaries without checksum verification
- **Risk**: Man-in-the-middle attacks or compromised GitHub releases
- **Fix**: Add SHA256 checksum verification using GitHub's published checksums

### 🔴 No Download Integrity Verification
- **File**: `src/main.rs` (tool execution)
- **Issue**: Executes downloaded binaries without cryptographic verification
- **Risk**: Tampered releases from compromised maintainer accounts
- **Fix**: Implement trust-on-first-use (TOFU) with stored checksums, verify when available

---

## High Priority (Architecture & Maintainability)

### 🟠 Refactor Large Source Files
- **Target**: `src/install.rs` (1110 lines), `src/main.rs` (650+ lines)
- **Issue**: Excessive complexity creates security fix risk and hinders testing
- **Structure**:
  ```
  src/
    main.rs              # Entry point only
    cli.rs               # Command definitions (already exists)
    config.rs            # Configuration management (already exists)
    download.rs          # Download logic (already exists)
    install/
      mod.rs             # Public interface
      core.rs            # Main installation orchestration
      github.rs          # GitHub API interaction
      recovery.rs        # Self-healing from filesystem
      updates.rs         # Update checking logic
      version.rs         # Version matching and comparison
      list.rs            # Tool listing/display
    tool_id.rs           # Tool identifier parsing (already exists)
    types.rs             # Type definitions (already exists)
    platform.rs          # Platform detection (already exists)
    shim.rs              # Shim script management (extract from main.rs)
    security.rs          # Security utilities (path validation, checksums)
  ```
- **Tests**: All tests in `test_*.rs` files, either adjacent to source or in `tests/` directory

### 🟠 Fix Stringly-Typed Version Handling
- **Issue**: String sentinel values "default" and "latest" used inconsistently (led to recent bug)
- **Recent Bug**: "default" version was passed to GitHub API as literal tag name
- **Fix**: Refactor `version: Option<String>` to typed enum:
  ```rust
  pub enum Version {
      Latest,           // Use GitHub's latest
      Specific(String), // Explicit version
  }
  ```
- **Benefit**: Compile-time prevention of invalid states

### 🟠 Extract Duplicate Error Handling
- **File**: `src/main.rs` (lines 105-111, 165-176, 191-216, 488-511)
- **Issue**: Same 404 error handling logic duplicated 4+ times
- **Fix**: Create helper function `handle_install_error()`

### 🟠 Define Constants for Magic Values
- **Issue**: String literals scattered throughout ("github", "url", "__", "latest", "default", 0o755)
- **Fix**: Create `src/constants.rs` module:
  ```rust
  pub const FORGE_GITHUB: &str = "github";
  pub const FORGE_URL: &str = "url";
  pub const DIR_SEPARATOR: &str = "__";
  pub const PERM_EXECUTABLE: u32 = 0o755;
  // etc.
  ```

---

## Medium Priority (Enhancements)

### 🟡 File Permission Race Condition
- **File**: `src/download.rs` (lines 338-342)
- **Issue**: Permissions set after download, leaving window where file is world-readable
- **Fix**: Set restrictive permissions (0o600) on creation, finalize after download

### 🟡 Information Disclosure in Errors
- **File**: Multiple locations in `src/main.rs`
- **Issue**: Error messages reveal internal paths
- **Fix**: Sanitize paths in user-facing errors, use relative paths or tool names only

### 🟡 Unbounded Redirect Following
- **File**: `src/download.rs` (line 17)
- **Issue**: No limit on HTTP redirects during download
- **Fix**: Configure reqwest with max 10 redirects

### 🟡 TLS Configuration Options
- **Issue**: No way to configure certificate validation for corporate proxies
- **Fix**: Add `TOOLER_CA_BUNDLE` and `TOOLER_NO_VERIFY_TLS` (with warnings) environment variables

### 🟡 Download Size Limits
- **Issue**: No maximum file size limit - malicious releases could exhaust disk space
- **Fix**: Add 500MB default limit, configurable via `TOOLER_MAX_DOWNLOAD_SIZE`

### 🟡 Tool Name Sanitization
- **File**: `src/main.rs` (shim creation)
- **Issue**: Tool names used in shell scripts without validation
- **Fix**: Validate tool names contain only alphanumeric, hyphens, and underscores

### 🟡 Refined Version Recovery
- **Issue**: `try_recover_tool` needs improvement for deeply nested version structures (e.g., `infisical-cli/v0.41.90`)
- **Related**: Properly handle complex GitHub tag formats

### 🟡 Deduce Install Type
- **Issue**: When recovering tools from filesystem, install type detection is heuristic-based
- **Fix**: Store install type metadata more reliably, improve detection logic

---

## Low Priority (Cleanup)

### 🟢 Dead Code Removal
- **Functions**: `find_highest_version()` (install.rs:527), `api_version()` (tool_id.rs:127)
- **Status**: Marked `#[allow(dead_code)]` but never used
- **Action**: Remove or properly integrate

### 🟢 Test File Cleanup
- **File**: `tests/test_version.rs`
- **Issue**: Contains only helper code, named confusingly
- **Fix**: Rename to `version_helper.rs` or merge into `common.rs`

### 🟢 Implement URL Version Discovery
- **File**: `src/install.rs` (line 411)
- **Issue**: `discover_url_versions()` is empty stub
- **Action**: Implement or remove if not needed

### 🟢 Don't Call GitHub for Unknown Tools
- **Issue**: `tooler run foo` (unqualified) makes GitHub API call that gives 404
- **Fix**: Check if tool exists locally or is qualified (has `/`) before calling GitHub

### 🟢 GitHub Tag Lookups
- **Feature**: "Smart versioning" to handle tags with/without 'v' prefixes automatically
- **Benefit**: More flexible version specification

---

## Completed ✅

### Recently Fixed (2026-02-02)
- ✅ **"default" Version Bug**: Fixed 404 error when running tools without explicit version
  - Added `build_gh_release_url()` function with proper version handling
  - Added unit tests for all version cases
  - Converted e2e tests to feature flags (`#[cfg(feature = "e2e")]`)

### Previously Completed
- ✅ Tool Aliases and automatic deduction
- ✅ Multi-binary archive support (kubeadm from kubectl)
- ✅ Auto-shimming default enabled
- ✅ Tooler-controlled shim directory (`~/.local/share/tooler/shims`)
- ✅ `tooler config show` with format options
- ✅ Auto-shim on `tooler pull`
- ✅ Enhanced `tooler list` with colors, age, architecture, stale status
- ✅ Fixed version display 'v' prefix bug
- ✅ Log level via `LOG_LEVEL` and `TOOLER_LOG_LEVEL`
- ✅ Actual auto-updates on run
- ✅ Self-healing filesystem discovery (recover lost configs)
- ✅ Fixed terraform/tflint substring matching bug
- ✅ Fixed tool age reporting (using `installed_at` not `last_accessed`)
- ✅ Robust update check logic with `last_checked` field
- ✅ Fixed usage tracking for short-name queries

---

## Future Ideas (Backlog)

- 🔮 **Enhanced Forge Support**: GitLab, Gitea, or other Git hosts
- 🔮 **Export/Import Config**: Easier migration of toolsets between machines
- 🔮 **Lock Files**: Pin entire toolset versions for reproducible environments
- 🔮 **Plugin System**: Allow custom install logic for complex tools
- 🔮 **Binary Signing**: Cryptographic signatures for all downloaded tools

---

## Test Coverage Gaps

### Security-Relevant Missing Tests
1. Path traversal attempts in archives
2. Checksum verification failure handling
3. Tool name sanitization (malicious names)
4. Permission error handling (read-only filesystems)
5. Concurrent installation race conditions
6. Malformed config recovery edge cases
7. Binary architecture mismatch rejection

### Recommended Test Structure
```
tests/
  unit/                    # Unit tests for specific modules
    test_version.rs
    test_platform.rs
    test_config.rs
  integration/             # Integration tests
    test_cli.rs
    test_install.rs
    test_recovery.rs
  e2e/                     # End-to-end tests (network required)
    test_github_tools.rs
    test_url_tools.rs
  security/                # Security-focused tests
    test_path_traversal.rs
    test_checksum_verify.rs
    test_input_validation.rs
  common.rs                # Shared test utilities
```

---

## Architecture Principles

1. **Security First**: All file operations validated, checksums verified, paths sanitized
2. **Fail Secure**: Default to safe behavior (no execution without verification)
3. **Separation of Concerns**: Each module has single responsibility
4. **Type Safety**: Use enums instead of string sentinel values
5. **Testability**: Small, focused functions that can be unit tested
6. **Defense in Depth**: Multiple validation layers

---

## Notes

- Planning files (task_plan.md, findings.md, progress.md, SECURITY_REVIEW.md) are local working documents and should not be committed
- Use priority levels (Critical/High/Medium/Low) instead of time-based language
- E2E tests use `#[cfg(feature = "e2e")]` and require `--features e2e` flag
- All tests must use proper isolation (TOOLER_CONFIG, TOOLER_DATA_DIR, etc.)
