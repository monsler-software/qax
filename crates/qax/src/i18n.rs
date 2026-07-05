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

/// Loads a compiled `.qm` catalogue and installs it globally. `qm_path` may be a
/// filesystem path or a Qt resource path (`":/i18n/app_ru.qm"`) for a catalogue
/// embedded via [`register_resource`]. Returns `None` if it could not be read.
/// Call once per language at startup. To pick the catalogue for the system UI
/// language automatically, use [`load_translation_for_locale`] instead.
pub fn load_translation(qm_path: &str) -> Option<Translator> {
    let path = CString::new(qm_path).ok()?;
    let t = unsafe { sys::qt_translator_load(path.as_ptr()) };
    if t.is_null() {
        None
    } else {
        Some(Translator(t))
    }
}

/// Loads the translation catalogue matching the current UI language from a
/// directory of `.qm` files, and installs it. `basename` is the shared prefix of
/// the catalogues (e.g. `"app"` for `app_ru.qm`, `app_de.qm`, …) and `directory`
/// is where they live — a filesystem path, or a Qt resource directory like
/// `":/i18n"` for catalogues embedded via [`register_resource`]. Qt picks the
/// best match for `QLocale::system()`; returns `None` if no catalogue matched.
///
/// ```no_run
/// use qax::i18n;
///
/// // app_ru.qm / app_de.qm bundled under ":/i18n" in the app's resources:
/// let _t = i18n::load_translation_for_locale("app", ":/i18n");
/// ```
pub fn load_translation_for_locale(basename: &str, directory: &str) -> Option<Translator> {
    let base = CString::new(basename).ok()?;
    let dir = CString::new(directory).ok()?;
    let t = unsafe { sys::qt_translator_load_for_locale(base.as_ptr(), dir.as_ptr()) };
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

/// Consumes a malloc'd C string returned by the shim, copying it into an owned
/// `String` and freeing the original. `null` becomes an empty string.
fn take_string(raw: *mut std::os::raw::c_char) -> String {
    if raw.is_null() {
        return String::new();
    }
    unsafe {
        let s = CStr::from_ptr(raw).to_string_lossy().into_owned();
        sys::qt_string_free(raw);
        s
    }
}

/// How a floating-point number is rendered by [`Locale::format_float_with`],
/// mirroring the format characters `QLocale::toString(double, char, int)` takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatFormat {
    /// Fixed notation, e.g. `1234.50`.
    Fixed,
    /// Scientific notation, e.g. `1.2345e+03`.
    Scientific,
    /// The shorter of fixed and scientific (Qt's default).
    General,
}

impl FloatFormat {
    fn code(self) -> std::os::raw::c_char {
        (match self {
            FloatFormat::Fixed => b'f',
            FloatFormat::Scientific => b'e',
            FloatFormat::General => b'g',
        }) as std::os::raw::c_char
    }
}

/// A locale — a language/territory pair that drives number formatting, native
/// names, and text direction (a Rust-idiomatic wrapper over Qt's `QLocale`).
///
/// ```no_run
/// use qax::i18n::Locale;
///
/// let ru = Locale::from_name("ru_RU");
/// assert_eq!(ru.native_language_name(), "русский");
/// println!("{}", ru.format_float(1234.5)); // "1 234,5"
///
/// // Route Qt's default formatting through the system locale:
/// Locale::system().set_as_default();
/// ```
pub struct Locale(*mut sys::QtLocale);

impl Locale {
    /// The user's configured system locale (`QLocale::system()`).
    pub fn system() -> Self {
        Locale(unsafe { sys::qt_locale_system() })
    }
    /// The locale-independent "C" locale (English, `.`/`,` separators).
    pub fn c() -> Self {
        Locale(unsafe { sys::qt_locale_c() })
    }
    /// Builds a locale from a name like `"ru_RU"`, `"en"`, or `"pt_BR"`.
    /// Unrecognized names fall back to the C locale, per Qt.
    pub fn from_name(name: &str) -> Self {
        let c = CString::new(name).unwrap_or_default();
        Locale(unsafe { sys::qt_locale_from_name(c.as_ptr()) })
    }

    /// The locale name, e.g. `"ru_RU"`.
    pub fn name(&self) -> String {
        take_string(unsafe { sys::qt_locale_name(self.0) })
    }
    /// The BCP 47 name, e.g. `"ru-RU"` (as used in HTTP/HTML).
    pub fn bcp47_name(&self) -> String {
        take_string(unsafe { sys::qt_locale_bcp47_name(self.0) })
    }
    /// The language's English name, e.g. `"Russian"`.
    pub fn language_name(&self) -> String {
        take_string(unsafe { sys::qt_locale_language_name(self.0) })
    }
    /// The language's name in its own language, e.g. `"русский"`.
    pub fn native_language_name(&self) -> String {
        take_string(unsafe { sys::qt_locale_native_language_name(self.0) })
    }
    /// The territory's English name, e.g. `"Russia"`.
    pub fn territory_name(&self) -> String {
        take_string(unsafe { sys::qt_locale_territory_name(self.0) })
    }
    /// The territory's name in the locale's language, e.g. `"Россия"`.
    pub fn native_territory_name(&self) -> String {
        take_string(unsafe { sys::qt_locale_native_territory_name(self.0) })
    }
    /// The decimal separator, e.g. `"."` or `","`.
    pub fn decimal_point(&self) -> String {
        take_string(unsafe { sys::qt_locale_decimal_point(self.0) })
    }
    /// The digit-group separator, e.g. `","`, `" "`, or `"."`.
    pub fn group_separator(&self) -> String {
        take_string(unsafe { sys::qt_locale_group_separator(self.0) })
    }
    /// Whether the locale's script is written right-to-left (Arabic, Hebrew, …).
    pub fn is_rtl(&self) -> bool {
        unsafe { sys::qt_locale_is_rtl(self.0) != 0 }
    }

    /// Formats an integer with this locale's grouping, e.g. `1234567` → `"1 234 567"`.
    pub fn format_int(&self, value: i64) -> String {
        take_string(unsafe { sys::qt_locale_format_i64(self.0, value) })
    }
    /// Formats a float in the locale's general notation with default precision.
    pub fn format_float(&self, value: f64) -> String {
        self.format_float_with(value, FloatFormat::General, -1)
    }
    /// Formats a float with an explicit notation and precision. A negative
    /// `precision` selects Qt's default.
    pub fn format_float_with(&self, value: f64, format: FloatFormat, precision: i32) -> String {
        take_string(unsafe { sys::qt_locale_format_f64(self.0, value, format.code(), precision) })
    }

    /// Installs this locale as the process-wide default (`QLocale::setDefault`),
    /// so widgets and Qt's own formatting adopt it.
    pub fn set_as_default(&self) {
        unsafe { sys::qt_locale_set_default(self.0) };
    }
}

impl Clone for Locale {
    fn clone(&self) -> Self {
        Locale(unsafe { sys::qt_locale_clone(self.0) })
    }
}

impl Drop for Locale {
    fn drop(&mut self) {
        unsafe { sys::qt_locale_delete(self.0) };
    }
}

impl Default for Locale {
    /// The system locale.
    fn default() -> Self {
        Locale::system()
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
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
