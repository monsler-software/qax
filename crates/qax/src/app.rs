use qax_sys as sys;

/// The Qt application and its event loop (wraps `QGuiApplication`).
///
/// Exactly one may exist per process; construction is not thread-safe and must
/// happen on the main thread before any GUI object is created.
pub struct Application {
    ptr: *mut sys::QtApp,
}

impl Application {
    /// Creates the application object. Panics if Qt fails to initialize.
    pub fn new() -> Self {
        let ptr = unsafe { sys::qt_app_new() };
        assert!(!ptr.is_null(), "failed to create QGuiApplication");
        Application { ptr }
    }

    /// Runs the event loop until the last window closes or [`Application::quit`]
    /// is called. Returns the process exit code.
    pub fn exec(&self) -> i32 {
        unsafe { sys::qt_app_exec(self.ptr) }
    }

    /// Requests the event loop to terminate.
    pub fn quit(&self) {
        unsafe { sys::qt_app_quit(self.ptr) };
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        unsafe { sys::qt_app_delete(self.ptr) };
    }
}
