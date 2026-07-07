// Compiles this crate's Qt Linguist `.ts` catalogues in `translations/` into
// runtime `.qm` binaries (written to OUT_DIR) as part of `cargo build`, via the
// shared `qax-build` helper. Best-effort: a checkout without `lrelease` still
// builds, just without translations.
//
// Downstream crates do the same from their own build.rs — add `qax-build` as a
// build dependency and call `qax_build::Build::new().translations(..).run()`.
fn main() {
    qax_build::Build::new()
        .translations("translations", ["ru"])
        .run();
}
