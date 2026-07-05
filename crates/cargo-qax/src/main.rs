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
//! The extractor understands the exact two `tr!` forms the macro accepts:
//! `tr!("text")` (default context) and `tr!("Context", "text")`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DEFAULT_CONTEXT: &str = "default";

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

/// One translatable string keyed by (context, source).
type Catalog = BTreeMap<String, BTreeMap<String, String>>; // context -> (source -> translation)

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
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut files = Vec::new();
    collect_rs(&src, &mut files);
    if files.is_empty() {
        eprintln!("cargo qax i18n: no .rs files under {}", src.display());
        return ExitCode::FAILURE;
    }
    for f in &files {
        if let Ok(text) = std::fs::read_to_string(f) {
            for (ctx, source) in extract_tr(&text) {
                let bucket = found.entry(ctx).or_default();
                if !bucket.contains(&source) {
                    bucket.push(source);
                }
            }
        }
    }
    let total: usize = found.values().map(Vec::len).sum();
    println!(
        "Found {total} string(s) in {} context(s) across {} file(s).",
        found.len(),
        files.len()
    );

    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("cargo qax i18n: cannot create {}: {e}", out.display());
        return ExitCode::FAILURE;
    }

    let crate_name = crate_name().unwrap_or_else(|| "app".to_string());
    for lang in &langs {
        let path = out.join(format!("{crate_name}_{lang}.ts"));
        let existing = std::fs::read_to_string(&path).ok();
        let merged = merge_catalog(&found, existing.as_deref());
        let xml = render_ts(lang, &merged);
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

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Scans `text` for `tr!(...)` invocations and returns their (context, source)
/// pairs. Recognizes `tr!("s")` and `tr!("ctx", "s")`, including raw strings.
fn extract_tr(text: &str) -> Vec<(String, String)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find("tr!") {
        let start = i + rel;
        i = start + 3;
        // Reject identifiers that merely end in `tr` (e.g. `attr!`, `str!`).
        if start > 0 {
            let prev = bytes[start - 1];
            if prev == b'_' || prev.is_ascii_alphanumeric() {
                continue;
            }
        }
        let mut j = skip_ws(bytes, i);
        if bytes.get(j) != Some(&b'(') {
            continue;
        }
        j += 1;
        // First string literal.
        j = skip_ws(bytes, j);
        let Some((first, mut k)) = parse_str(bytes, j) else {
            continue;
        };
        k = skip_ws(bytes, k);
        match bytes.get(k) {
            Some(b')') => out.push((DEFAULT_CONTEXT.to_string(), first)),
            Some(b',') => {
                let m = skip_ws(bytes, k + 1);
                if let Some((second, _)) = parse_str(bytes, m) {
                    out.push((first, second));
                }
            }
            _ => {}
        }
    }
    out
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i] as char).is_whitespace() {
        i += 1;
    }
    i
}

/// Parses a Rust string literal starting at `i` (normal or raw). Returns the
/// decoded contents and the index just past the closing quote.
fn parse_str(b: &[u8], i: usize) -> Option<(String, usize)> {
    // Raw string: r"..." or r#"..."# (any number of hashes).
    if b.get(i) == Some(&b'r') {
        let mut h = i + 1;
        let mut hashes = 0;
        while b.get(h) == Some(&b'#') {
            hashes += 1;
            h += 1;
        }
        if b.get(h) == Some(&b'"') {
            let content_start = h + 1;
            let mut j = content_start;
            let closing: Vec<u8> = std::iter::once(b'"')
                .chain(std::iter::repeat_n(b'#', hashes))
                .collect();
            while j < b.len() {
                if b[j] == b'"' && b[j..].starts_with(&closing) {
                    let s = String::from_utf8_lossy(&b[content_start..j]).into_owned();
                    return Some((s, j + closing.len()));
                }
                j += 1;
            }
            return None;
        }
    }
    // Normal string with escapes.
    if b.get(i) != Some(&b'"') {
        return None;
    }
    // Accumulate raw bytes so multi-byte UTF-8 (e.g. "−", "…") survives,
    // decoding once at the end rather than byte-by-byte.
    let mut j = i + 1;
    let mut out: Vec<u8> = Vec::new();
    while j < b.len() {
        match b[j] {
            b'\\' => {
                j += 1;
                match b.get(j) {
                    Some(b'n') => out.push(b'\n'),
                    Some(b't') => out.push(b'\t'),
                    Some(b'r') => out.push(b'\r'),
                    Some(b'\\') => out.push(b'\\'),
                    Some(b'"') => out.push(b'"'),
                    Some(b'\'') => out.push(b'\''),
                    Some(b'0') => out.push(b'\0'),
                    Some(&c) => out.push(c),
                    None => return None,
                }
                j += 1;
            }
            b'"' => return Some((String::from_utf8_lossy(&out).into_owned(), j + 1)),
            c => {
                out.push(c);
                j += 1;
            }
        }
    }
    None
}

/// Combines freshly-extracted strings with translations from an existing `.ts`,
/// keeping any translation already provided.
fn merge_catalog(found: &BTreeMap<String, Vec<String>>, existing: Option<&str>) -> Catalog {
    let prior = existing.map(parse_ts).unwrap_or_default();
    let mut cat: Catalog = Catalog::new();
    for (ctx, sources) in found {
        let bucket = cat.entry(ctx.clone()).or_default();
        for src in sources {
            let translated = prior
                .get(ctx)
                .and_then(|m| m.get(src))
                .cloned()
                .unwrap_or_default();
            bucket.insert(src.clone(), translated);
        }
    }
    cat
}

/// Best-effort extraction of (context -> source -> translation) from a `.ts`.
fn parse_ts(text: &str) -> Catalog {
    let mut cat = Catalog::new();
    let mut ctx = DEFAULT_CONTEXT.to_string();
    let mut cur_source: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(name) = between(line, "<name>", "</name>") {
            ctx = name;
        } else if let Some(src) = between(line, "<source>", "</source>") {
            cur_source = Some(unescape_xml(&src));
        } else if line.starts_with("<translation")
            && let Some(src) = cur_source.take()
        {
            let tr = between(line, ">", "</translation>")
                .map(|s| unescape_xml(&s))
                .unwrap_or_default();
            cat.entry(ctx.clone()).or_default().insert(src, tr);
        }
    }
    cat
}

fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(close)? + a;
    Some(s[a..b].to_string())
}

fn render_ts(lang: &str, cat: &Catalog) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str("<!DOCTYPE TS>\n");
    s.push_str(&format!("<TS version=\"2.1\" language=\"{}\">\n", xml_attr(lang)));
    for (ctx, messages) in cat {
        s.push_str("<context>\n");
        s.push_str(&format!("    <name>{}</name>\n", escape_xml(ctx)));
        for (source, translation) in messages {
            s.push_str("    <message>\n");
            s.push_str(&format!("        <source>{}</source>\n", escape_xml(source)));
            if translation.is_empty() {
                s.push_str("        <translation type=\"unfinished\"></translation>\n");
            } else {
                s.push_str(&format!(
                    "        <translation>{}</translation>\n",
                    escape_xml(translation)
                ));
            }
            s.push_str("    </message>\n");
        }
        s.push_str("</context>\n");
    }
    s.push_str("</TS>\n");
    s
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn xml_attr(s: &str) -> String {
    escape_xml(s).replace('"', "&quot;")
}
fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
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

    // Prefer `rcc`, fall back to the versioned `rcc-qax` some distros ship.
    let rcc = which(&["rcc", "rcc-qax", "/usr/lib/qax/rcc"]);
    let Some(rcc) = rcc else {
        eprintln!("cargo qax qrc: could not find Qt's `rcc` on PATH");
        return ExitCode::FAILURE;
    };

    let status = Command::new(&rcc)
        .arg("--binary")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("Compiled {} -> {}", input.display(), output.display());
            ExitCode::SUCCESS
        }
        Ok(s) => {
            eprintln!("cargo qax qrc: rcc exited with {s}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("cargo qax qrc: failed to run {}: {e}", rcc.display());
            ExitCode::FAILURE
        }
    }
}

/// Returns the first candidate that exists as an absolute path or resolves on
/// PATH.
fn which(candidates: &[&str]) -> Option<PathBuf> {
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
