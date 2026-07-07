//! Compiling `.qrc` resource files into binary `.rcc` bundles via Qt's `rcc`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tools;

/// Compiles `qrc` into `out` (a binary `.rcc` bundle) using Qt's `rcc`.
pub fn compile_qrc(rcc: &Path, qrc: &Path, out: &Path) -> Result<(), String> {
    let status = Command::new(rcc)
        .arg("--binary")
        .arg(qrc)
        .arg("-o")
        .arg(out)
        .status()
        .map_err(|e| format!("failed to run {}: {e}", rcc.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("rcc {} exited with {status}", qrc.display()))
    }
}

/// Locates `rcc`, or returns an error describing that it is missing.
pub fn locate() -> Result<PathBuf, String> {
    tools::find_rcc().ok_or_else(|| "rcc not found on PATH".to_string())
}
