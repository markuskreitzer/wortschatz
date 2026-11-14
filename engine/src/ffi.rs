//! C ABI so native shells (Swift on macOS, anything with a C FFI on Linux) can drive the engine.
//!
//! ```c
//! void *wz_engine_new(const char *lexicon_json_path, const char *state_path_or_null);
//! char *wz_engine_next_card_json(void *engine);   // free with wz_string_free
//! size_t wz_engine_count(void *engine);
//! void  wz_string_free(char *s);
//! void  wz_engine_free(void *engine);
//! ```
//!
//! No panic may unwind into the host (a panic crossing `extern "C"` aborts the process, which
//! would take down macOS's legacyScreenSaver). Every entry point runs inside [`guard`], which
//! catches a panic and returns the given fallback (NULL / 0) instead; the release profile uses
//! `panic = "unwind"` so this works.
use crate::{Engine, Lexicon};
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

/// Runs `f`, turning a panic into `fallback`.
pub(crate) fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

unsafe fn opt_path(p: *const c_char) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok().filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// Returns NULL if the lexicon cannot be loaded.
///
/// # Safety
/// `lexicon_path` must be a valid NUL-terminated string; `state_path` may be NULL.
#[no_mangle]
pub unsafe extern "C" fn wz_engine_new(lexicon_path: *const c_char, state_path: *const c_char) -> *mut Engine {
    guard(std::ptr::null_mut(), || {
        let Some(lp) = opt_path(lexicon_path) else { return std::ptr::null_mut() };
        match Lexicon::load(&lp) {
            Ok(lex) => Box::into_raw(Box::new(Engine::new(lex, opt_path(state_path)))),
            Err(_) => std::ptr::null_mut(),
        }
    })
}

/// # Safety
/// `engine` must come from `wz_engine_new` and not be freed.
#[no_mangle]
pub unsafe extern "C" fn wz_engine_next_card_json(engine: *mut Engine) -> *mut c_char {
    guard(std::ptr::null_mut(), || {
        let Some(engine) = engine.as_mut() else { return std::ptr::null_mut() };
        let card = engine.next_card();
        match serde_json::to_string(&card).ok().and_then(|s| CString::new(s).ok()) {
            Some(c) => c.into_raw(),
            None => std::ptr::null_mut(),
        }
    })
}

/// # Safety
/// `engine` must come from `wz_engine_new` or be NULL.
#[no_mangle]
pub unsafe extern "C" fn wz_engine_count(engine: *const Engine) -> usize {
    guard(0, || engine.as_ref().map(|e| e.lexicon.entries.len()).unwrap_or(0))
}

/// # Safety
/// `s` must come from this library or be NULL.
#[no_mangle]
pub unsafe extern "C" fn wz_string_free(s: *mut c_char) {
    guard((), || {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    })
}

/// # Safety
/// `engine` must come from `wz_engine_new` or be NULL; it must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn wz_engine_free(engine: *mut Engine) {
    guard((), || {
        if !engine.is_null() {
            drop(Box::from_raw(engine));
        }
    })
}
