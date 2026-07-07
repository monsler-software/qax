//! Compiling Qt Linguist `.ts` catalogues into runtime `.qm` binaries via
//! `lrelease`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tools;

/// Compiles a single `.ts` file into `<out_dir>/<stem>.qm` using `lrelease`.
/// Returns the path of the written `.qm` on success.
pub fn compile_ts(lrelease: &Path, ts: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let stem = ts
        .file_stem()
        .ok_or_else(|| format!("{}: no file stem", ts.display()))?;
    let qm = out_dir.join(stem).with_extension("qm");
    let status = Command::new(lrelease)
        .arg(ts)
        .arg("-qm")
        .arg(&qm)
        .status()
        .map_err(|e| format!("failed to run lrelease: {e}"))?;
    if status.success() {
        Ok(qm)
    } else {
        Err(format!("lrelease {} exited with {status}", ts.display()))
    }
}

/// Locates `lrelease`, or returns an error describing that it is missing.
pub fn locate() -> Result<PathBuf, String> {
    tools::find_lrelease().ok_or_else(|| "lrelease not found on PATH".to_string())
}
