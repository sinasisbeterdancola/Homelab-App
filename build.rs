// Embeds the app icon into homelab.exe (shown in Explorer and the taskbar).
use std::{env, fs, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        mingw();
    } else {
        embed_resource::compile("assets/app.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}

/// MinGW's windres breaks on any path containing a space (it hands paths to its
/// gcc preprocessor through a shell). So compile the resource in the temp folder
/// with relative file names, and link the resulting object directly.
fn mingw() {
    let windres = env::var_os("RC_x86_64_pc_windows_gnu").unwrap_or_else(|| "windres".into());
    let tmp = env::temp_dir().join("homelab-res");
    fs::create_dir_all(&tmp).unwrap();
    fs::copy("assets/app.rc", tmp.join("app.rc")).unwrap();
    fs::copy("assets/icon.ico", tmp.join("icon.ico")).unwrap();

    // windres runs `gcc` from PATH: prefer the one next to the configured windres.
    let mut dirs: Vec<_> = Path::new(&windres).parent().map(Path::to_path_buf).into_iter().collect();
    dirs.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));

    let status = Command::new(&windres)
        .current_dir(&tmp)
        .env("PATH", env::join_paths(dirs).unwrap())
        .args(["app.rc", "-O", "coff", "-o", "icon.o"])
        .status()
        .expect("couldn't run windres");
    assert!(status.success(), "windres failed");
    println!("cargo:rustc-link-arg={}", tmp.join("icon.o").display());
}
