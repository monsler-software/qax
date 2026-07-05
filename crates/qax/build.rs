// Compiles Qt Linguist `.ts` catalogues in `translations/` into runtime `.qm`
// binaries with Qt's `lrelease`, so an app's translations are built as part of
// `cargo build` — no separate manual step.
//
// This is best-effort: if `lrelease` isn't installed, or there are no `.ts`
// files, the build proceeds without translations (the app then just shows the
// original `tr!` strings). Downstream crates can copy this build script.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let dir = Path::new("translations");
    println!("cargo:rerun-if-changed=translations");
    if !dir.is_dir() {
        return;
    }

    let Some(lrelease) = find_lrelease() else {
        println!(
            "cargo:warning=lrelease not found; skipping .ts -> .qm (translations won't be built)"
        );
        return;
    };

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let ts = entry.path();
        if ts.extension().is_none_or(|e| e != "ts") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", ts.display());
        let qm = ts.with_extension("qm");
        let status = Command::new(&lrelease).arg(&ts).arg("-qm").arg(&qm).status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => println!("cargo:warning=lrelease {} exited with {s}", ts.display()),
            Err(e) => println!("cargo:warning=failed to run lrelease: {e}"),
        }
    }
}

/// Locates `lrelease` (Qt6 name variants and common install dirs).
fn find_lrelease() -> Option<PathBuf> {
    let candidates = [
        "lrelease",
        "lrelease-qt6",
        "lrelease6",
        "/usr/lib/qt6/bin/lrelease",
        "/usr/lib/qt6/lrelease",
    ];
    let path = std::env::var_os("PATH").unwrap_or_default();
    for cand in candidates {
        let p = Path::new(cand);
        if p.is_absolute() && p.exists() {
            return Some(p.to_path_buf());
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
