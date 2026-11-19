//! OS-level quit requests (#144): on macOS, Cmd+Q, the app menu's Quit,
//! the Dock's Quit and a logout all send `terminate:` to NSApplication,
//! which winit 0.30 lets end the process at once - its application
//! delegate has no `applicationShouldTerminate:`, so the unsaved-changes
//! prompt (#107) was skipped. `install` adds that method to winit's
//! delegate class: it answers "cancel" and raises a flag, and the shell
//! turns the flag into the same guarded Quit the window X sends.
//!
//! Provenance: our own design, specced from Apple's published
//! NSApplicationDelegate documentation (applicationShouldTerminate: /
//! NSTerminateCancel) and the Objective-C runtime reference
//! (class_addMethod); no HSMWorks material involved.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::state::State;

/// Set by the OS hook, taken by the shell once per loop turn.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Record an OS quit request (the hook's only side effect). Only the
/// macOS hook (and the tests) call it, so other targets don't build it -
/// it would be dead code under `clippy -D warnings` there.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn request() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
}

/// Turn a pending OS quit request into the guarded Quit command - the
/// same entry the window X and Ctrl+Q use, so a dirty document asks
/// Save / Don't Save / Cancel first. True when one was queued (the caller
/// then asks for a frame, which drains the queue).
pub(crate) fn route_pending(state: &mut State) -> bool {
    if !QUIT_REQUESTED.swap(false, Ordering::SeqCst) {
        return false;
    }
    crate::actions::quit(state);
    true
}

/// Hook NSApplication's terminate path (see the module note). Call once
/// the winit event loop exists - it installs the delegate. Returns whether
/// the hook is live; a refusal is logged and leaves winit's behaviour.
#[cfg(target_os = "macos")]
pub(crate) fn install() -> bool {
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::msg_send;

    type ShouldTerminate = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize;
    // NSApplicationTerminateReply's NSTerminateCancel.
    const NS_TERMINATE_CANCEL: usize = 0;

    // Must not unwind across the FFI boundary: it only stores a flag.
    extern "C" fn should_terminate(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> usize {
        request();
        NS_TERMINATE_CANCEL
    }

    // AnyClass::get, not class!: the macro panics on a missing class.
    let Some(ns_app) = AnyClass::get("NSApplication") else {
        diag_warn!("os quit hook: AppKit not loaded - Cmd+Q will not ask about unsaved changes");
        return false;
    };
    // SAFETY: `sharedApplication` and `delegate` are plain AppKit getters
    // returning (possibly nil) object pointers; winit's EventLoop::new has
    // already created the application on this (the main) thread.
    let app: *mut AnyObject = unsafe { msg_send![ns_app, sharedApplication] };
    if app.is_null() {
        diag_warn!("os quit hook: no NSApplication - Cmd+Q will not ask about unsaved changes");
        return false;
    }
    let delegate: *mut AnyObject = unsafe { msg_send![app, delegate] };
    // SAFETY: a non-null delegate is a live object retained by the app.
    let Some(delegate) = (unsafe { delegate.as_ref() }) else {
        diag_warn!("os quit hook: no application delegate - Cmd+Q will not ask about unsaved changes");
        return false;
    };
    let cls: &AnyClass = delegate.class();
    let types = c"Q@:@"; // NSUInteger (id self, SEL _cmd, id sender)
    // SAFETY: the selector name is a valid C string; `should_terminate`
    // has exactly the C signature the type string declares (usize is
    // NSUInteger on every macOS target), and class_addMethod only adds
    // (never replaces) - it returns NO when the class already has one.
    let added = unsafe {
        let name = objc2::ffi::sel_registerName(c"applicationShouldTerminate:".as_ptr());
        let imp: ShouldTerminate = should_terminate;
        // The runtime stores every method as the untyped `Imp` and calls
        // it back with the signature `types` names.
        let imp = std::mem::transmute::<ShouldTerminate, objc2::runtime::Imp>(imp);
        objc2::ffi::class_addMethod(std::ptr::from_ref(cls).cast_mut().cast(), name, Some(imp), types.as_ptr())
    };
    if added == objc2::ffi::NO {
        diag_warn!(
            "os quit hook: {} already answers applicationShouldTerminate: - left as is",
            cls.name()
        );
        return false;
    }
    true
}

/// Other platforms report their quit paths as a window close request,
/// which the shell already guards.
#[cfg(not(target_os = "macos"))]
pub(crate) fn install() -> bool {
    false
}
