// Embeds the app icon in the Windows .exe (shown in Explorer and the taskbar).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon/icon.ico");
    let rc = out.join("icon.rc");
    std::fs::write(
        &rc,
        format!(
            "1 ICON \"{}\"\n",
            ico.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let obj = out.join("icon.o");
    let windres = if cfg!(windows) {
        "windres"
    } else {
        "x86_64-w64-mingw32-windres"
    };
    let ok = std::process::Command::new(windres)
        .arg(&rc)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&obj)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    } else {
        println!("cargo:warning=windres not found; the .exe will use the default icon");
    }
}
