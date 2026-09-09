fn main() {
    println!("cargo:rerun-if-env-changed=BEAM_RELEASE_VERSION");
    let version = std::env::var("BEAM_RELEASE_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    let parts: Vec<_> = version.split('.').collect();
    assert!(parts.len() == 3 && parts.iter().all(|part| !part.is_empty()
        && part.bytes().all(|byte| byte.is_ascii_digit())
        && (part.len() == 1 || !part.starts_with('0'))), "BEAM_RELEASE_VERSION must be X.Y.Z");
    println!("cargo:rustc-env=BEAM_VERSION={version}");
    println!("cargo:rerun-if-env-changed=BEAM_RELEASE_PUBLIC_KEY");
    if let Ok(key) = std::env::var("BEAM_RELEASE_PUBLIC_KEY") {
        println!("cargo:rustc-env=BEAM_RELEASE_PUBLIC_KEY={key}");
    }
}
