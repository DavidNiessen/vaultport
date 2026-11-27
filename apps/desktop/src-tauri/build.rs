// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;
use std::process::Command;

fn main() {
    link_clang_runtime();
    tauri_build::build();
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
