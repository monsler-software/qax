//! Runtime internationalization and embedded resources.
//!
//! Mark translatable strings at their use site with [`tr!`]; each call routes
//! through [`tr`], which looks the text up in whatever `.qm` catalogues are
//! installed (falling back to the original text). The companion `cargo qax
//! i18n` tool scans your source for [`tr!`] calls and generates the Qt Linguist
//! `.ts` files translators fill in.
//!
//! ```no_run
//! use qax::{tr, i18n};
//!
//! // At startup, before building the UI:
//! let _ru = i18n::load_translation("translations/app_ru.qm");
//!
//! // Anywhere a user-facing string is produced:
//! let title = tr!("Now playing");
//! let menu = tr!("Menu", "Open file…"); // explicit context
//! ```

use std::ffi::{CStr, CString};

use qax_sys as sys;

/// An installed translation catalogue. Keep it alive for the program's lifetime;
/// dropping the value does not currently uninstall the catalogue.
pub struct Translator(#[allow(dead_code)] *mut sys::QtTranslator);

/// Loads a compiled `.qm` catalogue from disk and installs it globally. Returns
/// `None` if the file could not be read. Call once per language at startup.
pub fn load_translation(qm_path: &str) -> Option<Translator> {
    let path = CString::new(qm_path).ok()?;
    let t = unsafe { sys::qt_translator_load(path.as_ptr()) };
    if t.is_null() {
        None
    } else {
        Some(Translator(t))
    }
}

/// Translates `source` within `context` using the installed catalogues. Returns
/// `source` unchanged when there is no translation. Prefer the [`tr!`] macro,
/// which the extraction tool can find.
pub fn tr(context: &str, source: &str) -> String {
    let ctx = CString::new(context).unwrap_or_default();
    let src = CString::new(source).unwrap_or_default();
    unsafe {
        let raw = sys::qt_translate(ctx.as_ptr(), src.as_ptr());
        if raw.is_null() {
            return source.to_string();
        }
        let s = CStr::from_ptr(raw).to_string_lossy().into_owned();
        sys::qt_string_free(raw);
        s
    }
}

/// Registers an embedded compiled resource bundle (a `.rcc` built by `cargo qax
/// qrc`), making its files reachable under Qt's `:/` virtual filesystem. Feed it
/// bytes from `include_bytes!` so the data is `'static` and outlives the app.
///
/// ```ignore
/// static RES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/resources.rcc"));
/// qax::i18n::register_resource(RES);
/// ```
pub fn register_resource(data: &'static [u8]) -> bool {
    unsafe { sys::qt_resource_register(data.as_ptr()) != 0 }
}

/// The default translation context used by [`tr!`] when none is given. Mirrored
/// by the `cargo qax i18n` extractor.
pub const DEFAULT_CONTEXT: &str = "default";

/// Marks a user-facing string for translation and resolves it at runtime.
///
/// - `tr!("text")` — uses the [`DEFAULT_CONTEXT`].
/// - `tr!("Context", "text")` — groups the string under an explicit context.
///
/// The `cargo qax i18n` tool recognizes exactly these two forms.
#[macro_export]
macro_rules! tr {
    ($source:expr $(,)?) => {
        $crate::i18n::tr($crate::i18n::DEFAULT_CONTEXT, $source)
    };
    ($context:expr, $source:expr $(,)?) => {
        $crate::i18n::tr($context, $source)
    };
}
