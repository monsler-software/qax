//! Locating Qt command-line tools (`rcc`, `lrelease`) across the naming and
//! install-path variations different distributions ship.

use std::path::{Path, PathBuf};

/// Returns the first candidate that exists as an absolute path or resolves on
/// `PATH`. Candidates may be bare command names (looked up on `PATH`) or
/// absolute paths (checked directly).
pub fn which(candidates: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for cand in candidates {
        let p = Path::new(cand);
        if p.is_absolute() {
            if p.exists() {
                return Some(p.to_path_buf());
            }
            continue;
        }
        for dir in std::env::split_paths(&path) {
            let full = dir.join(cand);
            if full.exists() {
                return Some(full);
            }
        }
    }
    None
}

/// Locates Qt's resource compiler `rcc` (name and install-dir variants).
pub fn find_rcc() -> Option<PathBuf> {
    which(&[
        "rcc",
        "rcc-qt6",
        "rcc-qax",
        "/usr/lib/qt6/bin/rcc",
        "/usr/lib/qt6/rcc",
        "/usr/lib/qax/rcc",
    ])
}

/// Locates Qt's translation compiler `lrelease` (name and install-dir variants).
pub fn find_lrelease() -> Option<PathBuf> {
    which(&[
        "lrelease",
        "lrelease-qt6",
        "lrelease6",
        "/usr/lib/qt6/bin/lrelease",
        "/usr/lib/qt6/lrelease",
    ])
}
