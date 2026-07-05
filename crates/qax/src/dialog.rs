//! Modal dialogs and pop-up menus — small imperative helpers you call directly
//! (typically from a [`Component::update`](crate::ui::Component::update) handler),
//! not part of the reactive tree. Each blocks until the user responds.
//!
//! ```no_run
//! use qax::dialog;
//! if dialog::confirm("Quit", "Discard unsaved changes?") {
//!     // …
//! }
//! if let Some(name) = dialog::input("New playlist", "Name:", "") {
//!     println!("create {name}");
//! }
//! ```

use std::ffi::{CStr, CString};

use qax_sys as sys;

fn cstr(s: &str) -> CString {
    CString::new(s).expect("dialog string contains NUL")
}

/// Shows an informational message box with an OK button.
pub fn message(title: &str, text: &str) {
    unsafe { sys::qt_dialog_message(cstr(title).as_ptr(), cstr(text).as_ptr()) };
}

/// Shows a Yes/No question and returns `true` if the user chose Yes.
pub fn confirm(title: &str, text: &str) -> bool {
    unsafe { sys::qt_dialog_confirm(cstr(title).as_ptr(), cstr(text).as_ptr()) != 0 }
}

/// Prompts for a single line of text, pre-filled with `initial`. Returns the
/// entered string, or `None` if the user cancelled.
pub fn input(title: &str, label: &str, initial: &str) -> Option<String> {
    let ptr = unsafe {
        sys::qt_dialog_input(
            cstr(title).as_ptr(),
            cstr(label).as_ptr(),
            cstr(initial).as_ptr(),
        )
    };
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
    unsafe { sys::qt_string_free(ptr) };
    Some(s)
}

/// Consumes a malloc'd C string returned by the shim, freeing it. Returns `None`
/// for a null pointer (the user cancelled).
fn take_path(ptr: *mut std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
    unsafe { sys::qt_string_free(ptr) };
    Some(s)
}

/// Optional string to a C pointer: an empty/absent value passes NULL so Qt uses
/// its default. Returns a guard [`CString`] that must outlive the pointer.
fn opt_cstr(s: &str) -> Option<CString> {
    (!s.is_empty()).then(|| cstr(s))
}

fn opt_ptr(c: &Option<CString>) -> *const std::os::raw::c_char {
    c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
}

/// Shows an "open file" chooser and returns the selected path, or `None` if
/// cancelled. `dir` is the starting directory (empty for the default) and
/// `filter` is a Qt name filter such as `"Images (*.png *.jpg);;All files (*)"`
/// (empty for no filter).
pub fn open_file(title: &str, dir: &str, filter: &str) -> Option<String> {
    let (d, f) = (opt_cstr(dir), opt_cstr(filter));
    let ptr = unsafe {
        sys::qt_dialog_open_file(cstr(title).as_ptr(), opt_ptr(&d), opt_ptr(&f))
    };
    take_path(ptr)
}

/// Shows a "save file" chooser and returns the chosen path, or `None` if
/// cancelled. See [`open_file`] for the `dir` and `filter` arguments.
pub fn save_file(title: &str, dir: &str, filter: &str) -> Option<String> {
    let (d, f) = (opt_cstr(dir), opt_cstr(filter));
    let ptr = unsafe {
        sys::qt_dialog_save_file(cstr(title).as_ptr(), opt_ptr(&d), opt_ptr(&f))
    };
    take_path(ptr)
}

/// Shows a directory chooser and returns the selected folder, or `None` if
/// cancelled. `dir` is the starting directory (empty for the default).
pub fn open_dir(title: &str, dir: &str) -> Option<String> {
    let d = opt_cstr(dir);
    let ptr = unsafe { sys::qt_dialog_open_dir(cstr(title).as_ptr(), opt_ptr(&d)) };
    take_path(ptr)
}

/// Pops up a context menu of `items` at the given global screen coordinates and
/// returns the chosen item's index, or `None` if dismissed. Pair with a canvas
/// `on_mouse_down` handler that captured a right-click.
pub fn context_menu(items: &[&str], x: i32, y: i32) -> Option<usize> {
    let owned: Vec<CString> = items.iter().map(|s| cstr(s)).collect();
    let ptrs: Vec<*const std::os::raw::c_char> = owned.iter().map(|c| c.as_ptr()).collect();
    let choice = unsafe { sys::qt_popup_menu(ptrs.as_ptr(), ptrs.len() as i32, x, y) };
    (choice >= 0).then_some(choice as usize)
}
