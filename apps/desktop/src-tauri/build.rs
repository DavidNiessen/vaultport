// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;
use std::process::Command;

fn main() {
    link_clang_runtime();
    let embed_manifest = windows_msvc();
    let mut attributes = tauri_build::Attributes::new();
    if embed_manifest {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {e:#}");
    }
    if embed_manifest {
        embed_windows_manifest();
    }
}

fn windows_msvc() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}

/// Embeds the Common Controls v6 manifest into every executable of this package on MSVC.
///
/// tauri-build puts the manifest in the app's resource file, which only the app binary links.
/// Test executables then lack it, load Common Controls v5 and fail to start with
/// `STATUS_ENTRYPOINT_NOT_FOUND` (`TaskDialogIndirect` only exists in v6). Linker arguments
/// apply to the app and its test binaries alike.
fn embed_windows_manifest() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}

/// Links clang's compiler runtime on Apple targets when llama.cpp is built in.
///
/// llama.cpp's Metal backend uses `@available(...)`, which compiles to calls to
/// `___isPlatformVersionAtLeast` from `libclang_rt.<platform>.a`. rustc links with
/// `-nodefaultlibs`, so that runtime is never added and the link fails.
fn link_clang_runtime() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LOCAL_AI");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(os.as_str(), "macos" | "ios")
        || std::env::var_os("CARGO_FEATURE_LOCAL_AI").is_none()
    {
        return;
    }
    let runtime = match (
        os.as_str(),
        std::env::var("CARGO_CFG_TARGET_ABI").as_deref(),
    ) {
        ("macos", _) => "clang_rt.osx",
        (_, Ok("sim")) => "clang_rt.iossim",
        _ => "clang_rt.ios",
    };
    let resource_dir = [
        ("clang", &["--print-resource-dir"][..]),
        ("xcrun", &["clang", "--print-resource-dir"][..]),
    ]
    .iter()
    .find_map(|(cmd, args)| {
        let out = Command::new(cmd).args(*args).output().ok()?;
        let dir = String::from_utf8(out.stdout).ok()?;
        let dir = PathBuf::from(dir.trim());
        (out.status.success() && dir.is_dir()).then_some(dir)
    });
    let Some(dir) = resource_dir else {
        println!("cargo:warning=clang not found; the {runtime} runtime is not linked");
        return;
    };
    println!(
        "cargo:rustc-link-search=native={}",
        dir.join("lib").join("darwin").display()
    );
    println!("cargo:rustc-link-lib=static={runtime}");
}
