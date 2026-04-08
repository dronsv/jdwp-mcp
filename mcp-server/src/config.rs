// Tunable limits for object inspection and tracing safety.
//
// These control how deeply the debugger inspects objects to prevent
// killing the JDWP connection on large/complex object graphs.

/// Max fields to show with values in auto-resolve (stack output).
/// Objects with more fields show "ClassName@hex(N fields)" instead.
pub const AUTO_RESOLVE_MAX_FIELDS: usize = 8;

/// Threshold below which auto-resolve skips value fetch entirely.
pub const AUTO_RESOLVE_LARGE_OBJECT_THRESHOLD: usize = 20;

/// inspect: objects with <= this many fields get full string resolution.
pub const INSPECT_SMALL_OBJECT_THRESHOLD: usize = 10;

/// inspect: objects with more than this many fields show metadata only (no values).
pub const INSPECT_LARGE_OBJECT_THRESHOLD: usize = 30;

/// inspect: max array elements to show.
pub const INSPECT_MAX_ARRAY_ELEMENTS: i32 = 20;

/// inspect: arrays with <= this many elements get string resolution per element.
pub const INSPECT_SMALL_ARRAY_THRESHOLD: i32 = 10;

/// Max trace calls before auto-stop.
pub const TRACE_MAX_CALLS: usize = 500;

/// Max timeout for wait_for_event / wait_for_class (ms).
pub const MAX_WAIT_TIMEOUT_MS: u64 = 120_000;
