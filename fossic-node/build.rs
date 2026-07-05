extern crate napi_build;

fn main() {
    napi_build::setup();

    // napi CLI v3 sets NAPI_TYPE_DEF_TMP_FOLDER (a dir); napi-derive v2
    // proc-macro reads TYPE_DEF_TMP_PATH (a file). Bridge them here.
    // Contained workaround for CLI v3 / napi-derive v2 version skew.
    if let Ok(folder) = std::env::var("NAPI_TYPE_DEF_TMP_FOLDER") {
        let pkg = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
        println!("cargo:rustc-env=TYPE_DEF_TMP_PATH={}/{}", folder, pkg);
    }
}
