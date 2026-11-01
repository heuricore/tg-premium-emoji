//! Compiles the vendored rlottie with the `cc` crate: no cmake, no bindgen, no network.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_cpp(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "wasm") {
                continue;
            }
            collect_cpp(&path, out);
        } else if path.extension().is_some_and(|e| e == "cpp") {
            out.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../vendor/rlottie");
    println!("cargo:rerun-if-changed={}", root.display());

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(
        out.join("config.h"),
        "/* no image module, no threads, no model cache */\n",
    )
    .expect("write config.h");

    let mut files = Vec::new();
    collect_cpp(&root.join("src"), &mut files);
    files.sort();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .warnings(false)
        .files(&files)
        .include(&out)
        .include(root.join("inc"));
    for sub in [
        "src/vector",
        "src/vector/freetype",
        "src/vector/pixman",
        "src/vector/stb",
        "src/lottie",
        "src/binding/c",
    ] {
        build.include(root.join(sub));
    }
    build
        .define("LOT_BUILD", None)
        .define("LOTTIE_DISABLE_ARM_NEON", None);
    if build.get_compiler().is_like_msvc() {
        build.flag("/EHs-c-").flag("/GR-");
    } else {
        build
            .flag("-fno-exceptions")
            .flag("-fno-rtti")
            .flag("-fvisibility=hidden");
    }
    build.compile("rlottie");
}
