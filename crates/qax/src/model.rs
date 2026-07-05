//! [`Model`] — a bag of named fields shared with QML.
//!
//! Backed by `QQmlPropertyMap`, so from QML a model looks like an object with
//! plain properties: read `backend.count`, write `backend.count = 5`, and bind
//! against it. On the Rust side you [`set`](Model::set)/[`get`](Model::get)
//! fields and subscribe to changes with [`on_change`](Model::on_change) — the
//! callback fires for writes coming from *either* side, giving a single
//! reactive channel in idiomatic Rust closures.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

use qax_sys as sys;

use crate::value::{IntoValue, Value};

/// Heap-stable state behind a [`Model`]. Boxed so its address stays fixed after
/// we hand a pointer to it to the C shim as the change-callback's user data.
/// Callback fired when a field changes, with the key and a read-only field view.
type ChangeHandler = Box<dyn FnMut(&str, &Fields)>;

struct Inner {
    map: *mut sys::QtPropertyMap,
    on_change: Option<ChangeHandler>,
}

/// A read-only view of a model's fields, handed to change callbacks so they can
/// inspect the fresh values without borrowing the whole [`Model`].
pub struct Fields {
    map: *mut sys::QtPropertyMap,
}

impl Fields {
    /// Reads a field, or `None` if it is unset.
    pub fn get(&self, key: &str) -> Option<Value> {
        read_value(self.map, key)
    }
}

pub struct Model {
    inner: Box<Inner>,
}

impl Model {
    pub fn new() -> Self {
        let map = unsafe { sys::qt_property_map_new() };
        assert!(!map.is_null(), "failed to create QQmlPropertyMap");
        Model {
            inner: Box::new(Inner {
                map,
                on_change: None,
            }),
        }
    }

    /// Sets (or inserts) a field. Notifies QML bindings and any change callback.
    pub fn set(&mut self, key: &str, value: impl IntoValue) {
        let k = CString::new(key).expect("field name contains NUL");
        let map = self.inner.map;
        match value.into_value() {
            Value::Int(v) => unsafe { sys::qt_property_map_set_i64(map, k.as_ptr(), v) },
            Value::Float(v) => unsafe { sys::qt_property_map_set_f64(map, k.as_ptr(), v) },
            Value::Bool(v) => unsafe { sys::qt_property_map_set_bool(map, k.as_ptr(), v as i32) },
            Value::Str(v) => {
                let s = CString::new(v).expect("string value contains NUL");
                unsafe { sys::qt_property_map_set_str(map, k.as_ptr(), s.as_ptr()) };
            }
        }
    }

    /// Reads a field, or `None` if it is unset.
    pub fn get(&self, key: &str) -> Option<Value> {
        read_value(self.inner.map, key)
    }

    /// Registers the change observer. Fires with the changed key whenever a
    /// field changes — including writes originating in QML. Replaces any
    /// previously registered observer.
    pub fn on_change(&mut self, callback: impl FnMut(&str, &Fields) + 'static) {
        self.inner.on_change = Some(Box::new(callback));
        let user = self.inner.as_mut() as *mut Inner as *mut c_void;
        unsafe { sys::qt_property_map_on_changed(self.inner.map, trampoline, user) };
    }

    /// The underlying `QObject` pointer, for exposing this model to a QML engine.
    pub(crate) fn as_object(&self) -> *mut sys::QtObject {
        unsafe { sys::qt_property_map_as_object(self.inner.map) }
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe { sys::qt_property_map_delete(self.inner.map) };
    }
}

/// C callback trampoline. `user` is a `*mut Inner` whose address is stable for
/// the model's lifetime.
///
/// A `set` from inside the callback re-enters this function synchronously, so we
/// must never hold a live `&mut Inner` across the user closure. We *move* the
/// closure out of `Inner` (via raw-pointer access, no long-lived reference)
/// before invoking it and restore it afterwards. As a bonus this makes the
/// observer non-reentrant: a nested change sees `on_change == None` and returns,
/// preventing infinite recursion.
extern "C" fn trampoline(user: *mut c_void, key: *const c_char) {
    let inner = user as *mut Inner;
    // Brief field borrows that end before we call into user code.
    let map = unsafe { (*inner).map };
    let mut cb = match unsafe { (*inner).on_change.take() } {
        Some(cb) => cb,
        None => return,
    };

    if let Ok(key) = unsafe { CStr::from_ptr(key) }.to_str() {
        cb(key, &Fields { map });
    }

    unsafe { (*inner).on_change = Some(cb) };
}

fn read_value(map: *mut sys::QtPropertyMap, key: &str) -> Option<Value> {
    let k = CString::new(key).ok()?;
    unsafe {
        match sys::qt_property_map_kind(map, k.as_ptr()) {
            sys::QT_VK_I64 => Some(Value::Int(sys::qt_property_map_get_i64(map, k.as_ptr()))),
            sys::QT_VK_F64 => Some(Value::Float(sys::qt_property_map_get_f64(map, k.as_ptr()))),
            sys::QT_VK_BOOL => {
                Some(Value::Bool(sys::qt_property_map_get_bool(map, k.as_ptr()) != 0))
            }
            sys::QT_VK_STRING => {
                let raw = sys::qt_property_map_get_str(map, k.as_ptr());
                if raw.is_null() {
                    return None;
                }
                let s = CStr::from_ptr(raw).to_string_lossy().into_owned();
                sys::qt_string_free(raw);
                Some(Value::Str(s))
            }
            _ => None,
        }
    }
}
