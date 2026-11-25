use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    tauri_build::build();

    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=test_manifest.rc");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let profile = env::var("PROFILE").unwrap_or_default();

    if target_os == "windows" && target_env == "gnu" {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

        // Embed Common Controls v6 manifest for debug `cargo test` harnesses only;
        // release builds use tauri_build's official icon + manifest cleanly without .rsrc conflicts.
        if profile != "release" {
            let rc_path = manifest_dir.join("test_manifest.rc");
            let obj_path = out_dir.join("test_manifest.o");
            if rc_path.is_file() {
                if let Ok(st) = Command::new("windres")
                    .current_dir(&manifest_dir)
                    .args([
                        rc_path.to_str().unwrap(),
                        "-O",
                        "coff",
                        "-o",
                        obj_path.to_str().unwrap(),
                    ])
                    .status()
                {
                    if st.success() && obj_path.is_file() {
                        println!("cargo:rustc-link-arg={}", obj_path.display());
                    }
                }
            }
        }

        // Copy WebView2Loader.dll alongside the compiled target profile binaries for MinGW runtime
        // OUT_DIR is `<target>/<profile>/build/<crate>-<hash>/out`
        if let Some(build_dir) = out_dir.parent().and_then(|p| p.parent()) {
            if let Some(profile_dir) = build_dir.parent() {
                if let Ok(entries) = fs::read_dir(build_dir) {
                    for ent in entries.flatten() {
                        let name = ent.file_name().to_string_lossy().to_string();
                        if name.starts_with("webview2-com-sys") {
                            let dll = ent.path().join("out").join("x64").join("WebView2Loader.dll");
                            if dll.is_file() {
                                let _ = fs::copy(&dll, profile_dir.join("WebView2Loader.dll"));
                                let _ = fs::copy(&dll, profile_dir.join("deps").join("WebView2Loader.dll"));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}
