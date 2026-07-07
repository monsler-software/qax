//! `cargo qax` — build-time tooling for qax projects.
//!
//! Two subcommands:
//!
//!   * `cargo qax i18n`  — scan the crate's Rust sources for `tr!(...)` calls
//!     and generate/merge Qt Linguist `.ts` catalogues (one per language),
//!     preserving translations already filled in.
//!   * `cargo qax qrc`   — compile a `.qrc` resource file into a binary `.rcc`
//!     bundle (via Qt's `rcc`) you embed with `include_bytes!` and register at
//!     runtime through `qax::i18n::register_resource`.
//!
//! The heavy lifting (tr! extraction, `.ts` merge/render, tool discovery, and
//! invoking `rcc`/`lrelease`) lives in the shared `qax-build` crate, which build
//! scripts use too; this binary is a thin CLI over it.

use std::path::PathBuf;
use std::process::ExitCode;

use qax_build::{i18n, rcc};

fn main() -> ExitCode {
    // Invoked as `cargo qax <sub> ...`, so argv is `cargo-qax qax <sub> ...`.
    // Drop the leading `qax` cargo inserts when present.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("qax") {
        args.remove(0);
    }

    match args.first().map(String::as_str) {
        Some("i18n") => cmd_i18n(&args[1..]),
        Some("qrc") => cmd_qrc(&args[1..]),
        Some("-h") | Some("--help") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("cargo qax: unknown subcommand `{other}`\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        "cargo qax — tooling for qax\n\n\
         USAGE:\n\
         \x20 cargo qax i18n [--src DIR] [--out DIR] [--lang ru,en,...]\n\
         \x20     Scan sources for tr!() strings and write translations/<crate>_<lang>.ts\n\
         \x20     (existing translations are preserved).\n\n\
         \x20 cargo qax qrc <input.qrc> [-o OUTPUT.rcc]\n\
         \x20     Compile a Qt resource file into a binary .rcc bundle."
    );
}

// ===========================================================================
// i18n extraction
// ===========================================================================

fn cmd_i18n(args: &[String]) -> ExitCode {
    let mut src = PathBuf::from("src");
    let mut out = PathBuf::from("translations");
    let mut langs: Vec<String> = vec!["ru".to_string()];

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--src" => src = PathBuf::from(req(&mut it, "--src")),
            "--out" => out = PathBuf::from(req(&mut it, "--out")),
            "--lang" => {
                langs = req(&mut it, "--lang")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            other => {
                eprintln!("cargo qax i18n: unexpected argument `{other}`");
                return ExitCode::FAILURE;
            }
        }
    }

    // Collect every source string grouped by context.
    let found = i18n::scan_sources(&src);
    if found.is_empty() {
        eprintln!("cargo qax i18n: no tr!() strings found under {}", src.display());
        return ExitCode::FAILURE;
    }
    let total: usize = found.values().map(Vec::len).sum();
    println!(
        "Found {total} string(s) in {} context(s).",
        found.len()
    );

    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("cargo qax i18n: cannot create {}: {e}", out.display());
        return ExitCode::FAILURE;
    }

    let crate_name = crate_name().unwrap_or_else(|| "app".to_string());
    for lang in &langs {
        let path = out.join(format!("{crate_name}_{lang}.ts"));
        let existing = std::fs::read_to_string(&path).ok();
        let merged = i18n::merge_catalog(&found, existing.as_deref());
        let xml = i18n::render_ts(lang, &merged);
        match std::fs::write(&path, xml) {
            Ok(()) => println!("Wrote {}", path.display()),
            Err(e) => {
                eprintln!("cargo qax i18n: cannot write {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn req<'a>(it: &mut impl Iterator<Item = &'a String>, flag: &str) -> String {
    match it.next() {
        Some(v) => v.clone(),
        None => {
            eprintln!("cargo qax: {flag} needs a value");
            std::process::exit(1);
        }
    }
}

/// Reads the package name from Cargo.toml (best effort).
fn crate_name() -> Option<String> {
    let text = std::fs::read_to_string("Cargo.toml").ok()?;
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = t.strip_prefix("name")
        {
            let rest = rest.trim_start_matches([' ', '=']).trim();
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

// ===========================================================================
// qrc compilation
// ===========================================================================

fn cmd_qrc(args: &[String]) -> ExitCode {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(req(&mut it, "-o"))),
            other if !other.starts_with('-') => input = Some(PathBuf::from(other)),
            other => {
                eprintln!("cargo qax qrc: unexpected argument `{other}`");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(input) = input else {
        eprintln!("cargo qax qrc: missing <input.qrc>");
        return ExitCode::FAILURE;
    };
    let output = output.unwrap_or_else(|| input.with_extension("rcc"));

    let rcc = match rcc::locate() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cargo qax qrc: {e}");
            return ExitCode::FAILURE;
        }
    };

    match rcc::compile_qrc(&rcc, &input, &output) {
        Ok(()) => {
            println!("Compiled {} -> {}", input.display(), output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cargo qax qrc: {e}");
            ExitCode::FAILURE
        }
    }
}
