# emx-llm TODO List

## Completed Fixes ✅

### ~~#1. `src/bin/emx-llm.rs:42` - Potential panic in TxtarEntry parsing~~ ✅
**Fixed**: Removed `unwrap()`, directly assign `files.last_mut()` after push.

### ~~#2. `src/bin/emx-llm.rs:132` - Redundant unwrap after is_some() check~~ ✅
**Fixed**: Changed to `if let Some(model_ref) = &model` pattern.

### ~~#3. API Key Logging Risk~~ ✅
**Fixed**: Implemented custom `Debug` trait for `ProviderConfig` and `ModelConfig` that redacts API keys.

### ~~#5. No Rate Limiting Protection~~ ✅
**Fixed**: Added retry logic with exponential backoff for HTTP 429 responses (max 3 retries).

### ~~#6. Config File Loaded Multiple Times~~ ✅
**Fixed**: Refactored `load_for_model` to load TOML once via `load_toml_config()`.

### ~~#7. Unused Parameter~~ ✅
**Fixed**: Removed unused `_path_prefix` parameter from `resolve_model_config`.

### ~~#10. Missing Error Context~~ ✅
**Fixed**: Added error context to JSON parsing failures with response body.

### ~~#12. Timeout Configuration~~ ✅
**Fixed**: Added `timeout_secs` field to `ProviderConfig`, configurable via config file.

---

## Deferred / Not Applicable

### #4. No Certificate Pinning/Validation Configuration
**Status**: Deferred (per user request)

### #8. Deprecated Methods in message.rs
**Status**: Keep for backward compatibility, remove in next major version.

### #9. Inconsistent Error Types
**Status**: Deferred - requires larger refactoring.

### #11. No Retry Logic (beyond 429)
**Status**: Partial - only 429 rate limiting retry implemented.

---

## Remaining Items

### #13. No Connection Pool Configuration
**Issue**: Cannot configure connection pool size, keep-alive, etc.
**Fix**: Expose HTTP client configuration options.

### #14. No Request/Response Interceptors
**Issue**: Cannot hook into request/response cycle for logging, metrics, etc.
**Fix**: Consider middleware/interceptor pattern.

### #15. Extract Config Search to Separate Module
**Issue**: `find_sections_by_key` and `search_toml_sections` add complexity to config.rs.
**Fix**: Move to `src/config/search.rs` or similar.

### #16. Provider-Specific Request Building
**Issue**: Request building logic is mixed with client implementation.
**Fix**: Extract to separate `RequestBuilder` trait or struct.

### #17. SSE Parsing Duplication
**Issue**: OpenAI and Anthropic SSE parsing share similar patterns but are duplicated.
**Fix**: Create common SSE parsing utilities.

### #18. Missing Integration Tests for Error Cases
**Issue**: No tests for network failures, malformed responses, etc.
**Fix**: Add tests using mock server for error scenarios.

### #19. No Fuzz Testing for SSE Parsing
**Issue**: SSE parsing handles external input but has no fuzz testing.
**Fix**: Add fuzz tests for `SseBuffer::next_line()`.

### #20. Missing Safety Documentation
**Issue**: No documentation about panic guarantees.
**Fix**: Add `# Panics` section to public API documentation.

### #21. Missing Security Policy
**Issue**: No `SECURITY.md` file.
**Fix**: Add security policy document.
