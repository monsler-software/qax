use std::ffi::CString;

use qax_sys as sys;

fn cstr(s: &str) -> CString {
    CString::new(s).expect("application string contains NUL")
}

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

    /// Runs the event loop for `ms` milliseconds, then returns. Lets timers and
    /// posted callbacks fire without blocking indefinitely — useful for tests,
    /// demos, or driving a short animation from a non-GUI context.
    pub fn run_for(&self, ms: i32) -> i32 {
        unsafe { sys::qt_app_run_for(self.ptr, ms) }
    }

    /// Requests the event loop to terminate.
    pub fn quit(&self) {
        unsafe { sys::qt_app_quit(self.ptr) };
    }

    /// Sets the application id used by the desktop environment to group windows
    /// and find the app's `.desktop` file and icon. On Wayland this is the
    /// surface app-id (from Qt's *desktop file name*); pass the base name of your
    /// `.desktop` file, e.g. `"org.example.Player"`. Also sets the internal
    /// application name to the same value. Call before creating any window.
    pub fn set_application_id(&self, id: &str) -> &Self {
        let c = cstr(id);
        unsafe {
            sys::qt_app_set_desktop_file_name(c.as_ptr());
            sys::qt_app_set_application_name(c.as_ptr());
        }
        self
    }

    /// Sets the internal application name (`QCoreApplication::applicationName`),
    /// used for settings paths and some window-manager hints.
    pub fn set_application_name(&self, name: &str) -> &Self {
        unsafe { sys::qt_app_set_application_name(cstr(name).as_ptr()) };
        self
    }

    /// Sets the human-readable name shown in window title bars and task switchers
    /// (`QGuiApplication::applicationDisplayName`).
    pub fn set_display_name(&self, name: &str) -> &Self {
        unsafe { sys::qt_app_set_application_display_name(cstr(name).as_ptr()) };
        self
    }

    /// Sets the application version string (`QCoreApplication::applicationVersion`).
    pub fn set_version(&self, version: &str) -> &Self {
        unsafe { sys::qt_app_set_application_version(cstr(version).as_ptr()) };
        self
    }

    /// Sets the organization name and (optional) domain used for settings storage
    /// (`QCoreApplication::setOrganizationName` / `setOrganizationDomain`).
    pub fn set_organization(&self, name: &str, domain: &str) -> &Self {
        unsafe {
            sys::qt_app_set_organization_name(cstr(name).as_ptr());
            sys::qt_app_set_organization_domain(cstr(domain).as_ptr());
        }
        self
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
