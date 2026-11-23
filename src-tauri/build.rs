fn main() {
    tauri_build::build();

    // tauri-build embeds the application manifest in the windowed exe only, and
    // it does that on the release profile. The debug / test binaries built from
    // the same crate carry no manifest, so the loader cannot bind the Common
    // Controls v6 entrypoints the dialog code imports and aborts with
    // 0xc0000139 before the first test runs. Emit the manifest link args only
    // for debug; adding them to a release build duplicates the resource tauri
    // already embedded and CVTRES rejects the merge.
    //
    // Gate on the TARGET OS, not `cfg!(windows)`: that host flag is true even
    // when cross-compiling to Android from a Windows machine, and `/MANIFEST:*`
    // is an MSVC-only link arg that NDK clang rejects outright.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" && std::env::var("PROFILE").as_deref() == Ok("debug") {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let manifest = out.join("filedrop-test.manifest");
        std::fs::write(
            &manifest,
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#,
        )
        .expect("write test manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PROFILE");
    // tauri-build embeds `icons/icon.ico` into the Windows resource but does not
    // watch it, so regenerating the logo with `tauri icon` would otherwise be
    // silently reused from a stale cached resource.lib.
    println!("cargo:rerun-if-changed=icons");
}

