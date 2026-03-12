# Tooler Roadmap

## Status Legend

- 🔴 **Critical**: Security vulnerabilities requiring immediate attention
- 🟠 **High**: Significant functionality or architectural issues
- 🟡 **Medium**: Improvements and enhancements
- 🟢 **Low**: Nice-to-have features
- ✅ **Complete**: Finished items

---

## Critical Priority (Security)

### 🔴 Install Script Lacks Verification
- **File**: `install.sh`
- **Issue**: Downloads and executes binaries without checksum verification
- **Risk**: Man-in-the-middle attacks or compromised GitHub releases
- **Fix**: Add SHA256 checksum verification using GitHub's published checksums

### 🔴 No Download Integrity Verification
- **File**: `src/install/mod.rs` (install_or_update_tool)
- **Issue**: Executes downloaded binaries without cryptographic verification
- **Risk**: Tampered releases from compromised maintainer accounts
- **Fix**: Implement trust-on-first-use (TOFU) with stored checksums, verify when available

---

## High Priority (Architecture & Maintainability)

### 🟠 Continue Refactoring Large Source Files
- **Current state**: `src/install/mod.rs` (1147 lines), `src/main.rs` (910 lines)
- **Progress**: `install.rs` was split into `install/mod.rs` + `install/github.rs`, but both files remain large
- **Remaining work**:
  ```
  src/
    main.rs              # Still 910 lines; extract shim/symlink/self-update logic
    install/
      mod.rs             # Still 1147 lines; split further:
        core.rs          # Installation orchestration (install_or_update_tool)
        recovery.rs      # Self-healing (try_recover_tool, recover_all_installed_tools)
        updates.rs       # Update checking (check_for_updates)
        version.rs       # Version matching (version_matches, find_highest_version)
        list.rs          # Tool listing/display (list_installed_tools)
    shim.rs              # Extract from main.rs: create_shim_script, create_tool_symlink, strip_platform_suffix
    security.rs          # Security utilities (path validation, checksums)
  ```

### 🟠 Fix Stringly-Typed Version Handling
- **File**: `src/tool_id.rs` (line 85: `Some("default".to_string())`)
- **Issue**: String sentinel values "default" and "latest" used inconsistently (led to bug fixed in v0.6.2)
- **Fix**: Refactor `version: Option<String>` to typed enum:
  ```rust
  pub enum Version {
      Latest,           // Use GitHub's latest
      Specific(String), // Explicit version
  }
  ```
- **Benefit**: Compile-time prevention of invalid states

### 🟠 Extract Duplicate Error Handling
- **File**: `src/main.rs` (Pull handler lines 246-282, execute_run lines 617-641)
- **Issue**: `GitHubReleaseError` match arms duplicated in Pull and execute_run
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
- **File**: `src/download.rs` (line 34: `fs::File::create`)
- **Issue**: Downloaded file is world-readable until permissions are set after extraction
- **Fix**: Set restrictive permissions (0o600) on creation, finalize after download

### 🟡 Information Disclosure in Errors
- **File**: Multiple locations in `src/main.rs`
- **Issue**: Error messages reveal internal paths (executable_path shown in logs)
- **Fix**: Sanitize paths in user-facing errors, use relative paths or tool names only

### 🟡 Unbounded Redirect Following
- **File**: `src/download.rs` (line 17: `reqwest::get(url)`)
- **Issue**: No limit on HTTP redirects during download (uses reqwest default)
- **Fix**: Build a `reqwest::Client` with explicit max 10 redirects

### 🟡 TLS Configuration Options
- **Issue**: No way to configure certificate validation for corporate proxies
- **Fix**: Add `TOOLER_CA_BUNDLE` and `TOOLER_NO_VERIFY_TLS` (with warnings) environment variables

### 🟡 Download Size Limits
- **Issue**: No maximum file size limit; malicious releases could exhaust disk space
- **Fix**: Add 500MB default limit, configurable via `TOOLER_MAX_DOWNLOAD_SIZE`

### 🟡 Tool Name Sanitization
- **File**: `src/main.rs` (create_tool_symlink, create_shim_script)
- **Issue**: Tool names used in shell scripts and symlink paths without validation
- **Fix**: Validate tool names contain only alphanumeric, hyphens, and underscores

### 🟡 Refined Version Recovery
- **Issue**: `try_recover_tool` needs improvement for deeply nested version structures (e.g., `infisical-cli/v0.41.90`)
- **Related**: Properly handle complex GitHub tag formats

### 🟡 Deduce Install Type
- **File**: `src/install/mod.rs` (lines 877-888)
- **Issue**: When recovering tools from filesystem, install type detection relies on file count heuristic (<=3 files = binary)
- **Fix**: Store install type metadata more reliably, improve detection logic

---

## Low Priority (Cleanup)

### 🟢 Dead Code Removal
- **Functions**: `find_highest_version()` (`src/install/mod.rs:541`), `api_version()` (`src/tool_id.rs:127`)
- **Status**: Marked `#[allow(dead_code)]` but never called outside tests
- **Action**: Remove or properly integrate

### 🟢 Test File Cleanup
- **File**: `tests/test_version.rs`
- **Issue**: Contains only a standalone `main()` function with a copy of `version_matches()` logic; not a real test module
- **Fix**: Remove or convert to a proper unit test

### 🟢 Implement URL Version Discovery
- **File**: `src/install/github.rs` (line 96)
- **Issue**: `discover_url_versions()` is an empty stub returning `Ok(vec![])`
- **Action**: Implement directory scraping or remove if not needed

### 🟢 Don't Call GitHub for Unknown Tools
- **Issue**: `tooler run foo` (unqualified name without `/`) makes GitHub API call that returns 404
- **Fix**: Check if tool exists locally or is qualified (has `/`) before calling GitHub

### 🟢 GitHub Tag Lookups
- **Feature**: "Smart versioning" to handle tags with/without 'v' prefixes automatically
- **Benefit**: More flexible version specification

---

## Completed

### v0.6.3 (2026-03)
- ✅ **Path Traversal Protection for Tar Archives**: Added `starts_with(extract_dir)` validation to both `extract_tar_gz` and `extract_tar_xz` in `src/download.rs`, matching the existing ZIP protection
- ✅ **Self-Update**: `tooler update tooler` replaces the running binary via `handle_self_update()` in `src/main.rs`
- ✅ **Multi-Binary Symlinks**: `pull` and `run` create symlinks for all executables found in the tool directory, not just the primary binary
- ✅ **Platform Suffix Stripping**: `strip_platform_suffix()` extracts base tool names (e.g., `cmk.linux.x86-64` becomes `cmk`) for cleaner symlinks
- ✅ **Deep Search Improvements**: `find_tool_entry` and `has_matching_binary` match binaries with platform suffixes (e.g., searching for "cmk" finds `cmk.linux.x86-64`)
- ✅ **Install Script Improvements**: `install.sh` tries `gh` CLI first for authenticated API access, provides rate limit error guidance, and bootstraps self-update via `tooler pull morgaesis/tooler`
- ✅ **Initial install.rs Split**: Extracted `src/install/github.rs` from monolithic `src/install.rs` into `src/install/` module structure

### v0.6.2 (2026-02-02)
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
1. Path traversal attempts in archives (protection exists but no test)
2. Checksum verification failure handling (not yet implemented)
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
