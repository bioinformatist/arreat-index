fn main() {
    println!("cargo:rerun-if-env-changed=CASCLIB_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ZLIB_STATIC_LIB_DIR");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    if let Ok(directory) = std::env::var("CASCLIB_LIB_DIR") {
        println!("cargo:rustc-link-search=native={directory}");
    }
    let zlib_directory = std::env::var("ZLIB_STATIC_LIB_DIR")
        .expect("ZLIB_STATIC_LIB_DIR must be set to the directory containing zlib static library (e.g. ${pkgs.zlib.static}/lib)");
    println!("cargo:rustc-link-search=native={zlib_directory}");
    println!("cargo:rustc-link-lib=static=casc");
    println!("cargo:rustc-link-lib=static=z");
    println!("cargo:rustc-link-lib=stdc++");
}
