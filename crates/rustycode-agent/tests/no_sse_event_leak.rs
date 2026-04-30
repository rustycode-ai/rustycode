//! Compile-fail guard: `SSEEvent` must stay internal to rustycode-llm.
//!
//! This test intentionally does NOT compile if `SSEEvent` is re-exported.
//! If it compiles, that means `SSEEvent` leaked into the public API again.

/// ```compile_fail
/// use rustycode_llm::provider::SSEEvent;
/// ```
const _SSE_EVENT_NOT_EXPORTED: () = ();

/// ```compile_fail
/// use rustycode_llm::provider::ContentBlockType;
/// ```
const _CONTENT_BLOCK_TYPE_NOT_EXPORTED: () = ();

/// ```compile_fail
/// use rustycode_llm::provider::ContentDelta;
/// ```
const _CONTENT_DELTA_NOT_EXPORTED: () = ();

#[test]
fn sse_event_types_not_exported() {
    // If this compiles, the compile_fail doctests above ensure the types are inaccessible.
}
