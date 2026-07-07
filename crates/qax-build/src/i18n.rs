//! `tr!()` extraction and Qt Linguist `.ts` catalogue handling.
//!
//! The extractor understands the exact two `tr!` forms the macro accepts:
//! `tr!("text")` (default context) and `tr!("Context", "text")`. These helpers
//! are shared by the `cargo qax i18n` subcommand and available to build scripts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The default translation context, mirrored by `qax::i18n::DEFAULT_CONTEXT`.
pub const DEFAULT_CONTEXT: &str = "default";

/// One translatable string keyed by (context, source).
pub type Catalog = BTreeMap<String, BTreeMap<String, String>>; // context -> (source -> translation)

/// Recursively collects every `.rs` file under `dir` into `out`.
pub fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
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

/// Scans every `.rs` file under `src` and returns the found strings grouped by
/// context, preserving first-seen order within each context.
pub fn scan_sources(src: &Path) -> BTreeMap<String, Vec<String>> {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut files = Vec::new();
    collect_rs(src, &mut files);
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
    found
}

/// Scans `text` for `tr!(...)` invocations and returns their (context, source)
/// pairs. Recognizes `tr!("s")` and `tr!("ctx", "s")`, including raw strings.
pub fn extract_tr(text: &str) -> Vec<(String, String)> {
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
pub fn merge_catalog(found: &BTreeMap<String, Vec<String>>, existing: Option<&str>) -> Catalog {
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
pub fn parse_ts(text: &str) -> Catalog {
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

/// Renders a `.ts` XML document for `lang` from `cat`.
pub fn render_ts(lang: &str, cat: &Catalog) -> String {
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
