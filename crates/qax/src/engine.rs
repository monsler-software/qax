use std::ffi::CString;

use qax_sys as sys;

use crate::model::Model;

/// The QML runtime (wraps `QQmlApplicationEngine`).
///
/// Load a `.qml` entry point with [`load_file`](QmlEngine::load_file) /
/// [`load_url`](QmlEngine::load_url) / [`load_data`](QmlEngine::load_data), and
/// expose Rust [`Model`]s to it as context objects with
/// [`set_context`](QmlEngine::set_context).
pub struct QmlEngine {
    ptr: *mut sys::QtEngine,
}

impl QmlEngine {
    pub fn new() -> Self {
        let ptr = unsafe { sys::qt_qml_engine_new() };
        assert!(!ptr.is_null(), "failed to create QQmlApplicationEngine");
        QmlEngine { ptr }
    }

    /// Loads a QML file from a local filesystem path.
    pub fn load_file(&mut self, path: &str) {
        let c = CString::new(path).expect("path contains NUL");
        unsafe { sys::qt_qml_engine_load_file(self.ptr, c.as_ptr()) };
    }

    /// Loads QML from a URL (e.g. `qrc:/main.qml` or `file:///...`).
    pub fn load_url(&mut self, url: &str) {
        let c = CString::new(url).expect("url contains NUL");
        unsafe { sys::qt_qml_engine_load_url(self.ptr, c.as_ptr()) };
    }

    /// Loads QML directly from source text. `url` is the logical name used for
    /// error messages and relative imports.
    pub fn load_data(&mut self, source: &str, url: &str) {
        let u = CString::new(url).expect("url contains NUL");
        unsafe {
            sys::qt_qml_engine_load_data(
                self.ptr,
                source.as_ptr() as *const _,
                source.len(),
                u.as_ptr(),
            )
        };
    }

    /// Exposes a [`Model`] to QML under `name`, reachable as a global object in
    /// every loaded document. The model must outlive the engine.
    pub fn set_context(&mut self, name: &str, model: &Model) {
        let c = CString::new(name).expect("name contains NUL");
        unsafe { sys::qt_qml_engine_set_context_object(self.ptr, c.as_ptr(), model.as_object()) };
    }

    /// Number of top-level objects successfully instantiated. Zero after a load
    /// means the QML failed to compile or run.
    pub fn root_count(&self) -> usize {
        unsafe { sys::qt_qml_engine_root_count(self.ptr) as usize }
    }
}

impl Default for QmlEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for QmlEngine {
    fn drop(&mut self) {
        unsafe { sys::qt_qml_engine_delete(self.ptr) };
    }
}
