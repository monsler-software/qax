//! Exercises the canvas mouse plumbing end to end through the FFI: an `RsCanvas`
//! forwards press/move/release to the Rust callback with the right kind, local
//! coordinates, and decoded button. Runs headless (offscreen). Synthetic events
//! deliver synchronously, so the callback fires inside `qt_canvas_send_mouse`.

use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicI32, Ordering};

use qax::Application;
use qax_sys as sys;

static PRESSES: AtomicI32 = AtomicI32::new(0);
static MOVES: AtomicI32 = AtomicI32::new(0);
static RELEASES: AtomicI32 = AtomicI32::new(0);
static LAST_X: AtomicI32 = AtomicI32::new(-1);
static LAST_Y: AtomicI32 = AtomicI32::new(-1);
static LAST_BUTTON: AtomicI32 = AtomicI32::new(-1);

extern "C" fn noop_paint(_u: *mut c_void, _p: *mut sys::QtPainter, _w: c_int, _h: c_int) {}

extern "C" fn record(_u: *mut c_void, kind: c_int, x: c_int, y: c_int, button: c_int) {
    match kind {
        0 => PRESSES.fetch_add(1, Ordering::SeqCst),
        1 => MOVES.fetch_add(1, Ordering::SeqCst),
        2 => RELEASES.fetch_add(1, Ordering::SeqCst),
        _ => 0,
    };
    LAST_X.store(x, Ordering::SeqCst);
    LAST_Y.store(y, Ordering::SeqCst);
    LAST_BUTTON.store(button, Ordering::SeqCst);
}

#[test]
fn canvas_forwards_mouse_events() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let _app = Application::new();

    let canvas = unsafe { sys::qt_canvas_new(noop_paint, std::ptr::null_mut()) };
    // Tracking on so move events fire without a button held.
    unsafe { sys::qt_canvas_on_mouse(canvas, record, std::ptr::null_mut(), 1) };

    // Press left at (10, 20).
    unsafe { sys::qt_canvas_send_mouse(canvas, 0, 10, 20, 1) };
    assert_eq!(PRESSES.load(Ordering::SeqCst), 1);
    assert_eq!(LAST_X.load(Ordering::SeqCst), 10);
    assert_eq!(LAST_Y.load(Ordering::SeqCst), 20);
    assert_eq!(LAST_BUTTON.load(Ordering::SeqCst), 1, "left button code");

    // Move to (30, 40).
    unsafe { sys::qt_canvas_send_mouse(canvas, 1, 30, 40, 0) };
    assert_eq!(MOVES.load(Ordering::SeqCst), 1);
    assert_eq!(LAST_X.load(Ordering::SeqCst), 30);
    assert_eq!(LAST_Y.load(Ordering::SeqCst), 40);

    // Release right at (30, 40).
    unsafe { sys::qt_canvas_send_mouse(canvas, 2, 30, 40, 2) };
    assert_eq!(RELEASES.load(Ordering::SeqCst), 1);
    assert_eq!(LAST_BUTTON.load(Ordering::SeqCst), 2, "right button code");

    unsafe { sys::qt_widget_delete(canvas) };
}
