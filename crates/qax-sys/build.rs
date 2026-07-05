// Compiles the C++ shim and links it against Qt6.
//
// There are no cargo features to toggle: every wrapper is always compiled. The
// "smart component" behaviour is fully automatic — the shim is built with
// per-function/per-data sections, and the final binary is linked with
// --gc-sections (+ --as-needed), so the linker discards every wrapper function
// (and, transitively, every Qt shared library) that your Rust code never
// references. Use a widget and it stays; don't and it disappears. See
// .cargo/config.toml for the linker flags that reach the final binary.
use std::env;

const QT_MODULES: &[&str] = &["Qt6Core", "Qt6Gui", "Qt6Qml", "Qt6Quick", "Qt6Widgets"];

fn main() {
    println!("cargo:rerun-if-changed=cpp/shim.cpp");
    println!("cargo:rerun-if-changed=cpp/shim.h");

    let mut build = cc::Build::new();
    build.cpp(true).file("cpp/shim.cpp").std("c++17");
    build.flag_if_supported("-fPIC");
    // Emit each function/global in its own section so the linker's --gc-sections
    // can drop the ones nothing references.
    build.flag_if_supported("-ffunction-sections");
    build.flag_if_supported("-fdata-sections");

    for module in QT_MODULES {
        let lib = pkg_config::Config::new()
            .cargo_metadata(true) // emit link directives for the final binary
            .probe(module)
            .unwrap_or_else(|e| panic!("Qt6 module {module} not found: {e}"));
        for path in &lib.include_paths {
            build.include(path);
        }
        for (name, value) in &lib.defines {
            build.define(name, value.as_deref());
        }
    }

    build.compile("qaxshim");

    // The C++ runtime link propagates to the final binary (unlike link-arg).
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
}
