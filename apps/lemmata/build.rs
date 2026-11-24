fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    // The .exe's own icon, which Explorer, shortcuts and the taskbar show (needs the Windows SDK's
    // rc.exe, which the MSVC toolchain brings).
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        res.compile().expect("embedding the app icon");
    }
}
