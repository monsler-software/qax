//! Build-script helper for [qax](https://crates.io/crates/qax): compile Qt
//! translations and resources as part of `cargo build`, with no separate manual
//! step and no copy-pasted build script.
//!
//! Add `qax-build` as a build dependency and drive it from your `build.rs`:
//!
//! ```no_run
//! // build.rs
//! fn main() {
//!     qax_build::Build::new()
//!         .translations("translations", ["ru", "en"]) // *.ts -> OUT_DIR/*.qm
//!         .resource("assets/resources.qrc")            // *.qrc -> OUT_DIR/resources.rcc
//!         .run();
//! }
//! ```
//!
//! Then load the artifacts from `OUT_DIR` at runtime:
//!
//! ```ignore
//! static RES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/resources.rcc"));
//! qax::i18n::register_resource(RES);
//! let _ru = qax::i18n::load_translation(concat!(env!("OUT_DIR"), "/app_ru.qm"));
//! ```
//!
//! Compilation is **best-effort**: if `lrelease` or `rcc` is not installed the
//! build proceeds (emitting a `cargo:warning`) so a checkout without Qt tools
//! still builds — the app then just shows the original `tr!` strings and lacks
//! embedded resources.
//!
//! This crate only *compiles* existing `.ts`/`.qrc`. Extracting `tr!()` strings
//! into `.ts` catalogues is an explicit developer step handled by the
//! `cargo qax i18n` subcommand, so translator-owned `.ts` files are never
//! rewritten mid-build.

use std::path::{Path, PathBuf};

pub mod i18n;
pub mod qm;
pub mod rcc;
pub mod tools;

/// A declarative description of the translation and resource compilation to run
/// during a build. Construct with [`Build::new`], configure with
/// [`translations`](Build::translations) and [`resource`](Build::resource), then
/// invoke [`run`](Build::run).
#[derive(Default)]
pub struct Build {
    translations: Option<PathBuf>,
    resources: Vec<PathBuf>,
}

impl Build {
    /// Creates an empty build with nothing configured.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compiles every `.ts` catalogue in `dir` into `OUT_DIR/<stem>.qm`.
    ///
    /// The `_langs` argument is accepted for documentation and forward
    /// compatibility (e.g. `["ru", "en"]`); the current implementation compiles
    /// whatever `.ts` files are present in `dir` regardless. Only one
    /// translations directory is tracked; a later call replaces an earlier one.
    pub fn translations<I, S>(mut self, dir: impl AsRef<Path>, _langs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.translations = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Adds a `.qrc` file to compile into `OUT_DIR/<stem>.rcc`.
    pub fn resource(mut self, qrc: impl AsRef<Path>) -> Self {
        self.resources.push(qrc.as_ref().to_path_buf());
        self
    }

    /// Adds several `.qrc` files at once (see [`resource`](Build::resource)).
    pub fn resources<I, P>(mut self, qrcs: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        for q in qrcs {
            self.resources.push(q.as_ref().to_path_buf());
        }
        self
    }

    /// Runs the configured compilation. Meant to be called from `build.rs`:
    /// emits `cargo:rerun-if-changed` for its inputs and writes artifacts into
    /// `OUT_DIR`. Missing Qt tools produce a `cargo:warning` rather than a hard
    /// error, so the build still succeeds.
    pub fn run(self) {
        let out_dir = match std::env::var_os("OUT_DIR") {
            Some(d) => PathBuf::from(d),
            None => {
                println!("cargo:warning=qax-build: OUT_DIR not set; not running in a build script?");
                return;
            }
        };

        if let Some(dir) = &self.translations {
            build_translations(dir, &out_dir);
        }
        for qrc in &self.resources {
            build_resource(qrc, &out_dir);
        }
    }
}

fn build_translations(dir: &Path, out_dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let ts_files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ts"))
        .collect();
    if ts_files.is_empty() {
        return;
    }

    let lrelease = match qm::locate() {
        Ok(p) => p,
        Err(e) => {
            println!("cargo:warning=qax-build: {e}; skipping .ts -> .qm (translations won't be built)");
            return;
        }
    };
    for ts in ts_files {
        println!("cargo:rerun-if-changed={}", ts.display());
        if let Err(e) = qm::compile_ts(&lrelease, &ts, out_dir) {
            println!("cargo:warning=qax-build: {e}");
        }
    }
}

fn build_resource(qrc: &Path, out_dir: &Path) {
    println!("cargo:rerun-if-changed={}", qrc.display());
    if !qrc.is_file() {
        println!("cargo:warning=qax-build: resource {} not found", qrc.display());
        return;
    }
    let rcc = match rcc::locate() {
        Ok(p) => p,
        Err(e) => {
            println!("cargo:warning=qax-build: {e}; skipping .qrc -> .rcc ({} won't be embedded)", qrc.display());
            return;
        }
    };
    let stem = qrc.file_stem().unwrap_or_default();
    let out = out_dir.join(stem).with_extension("rcc");
    if let Err(e) = rcc::compile_qrc(&rcc, qrc, &out) {
        println!("cargo:warning=qax-build: {e}");
    }
}
