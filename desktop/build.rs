fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("packaging/windows/filebeam.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap_or_else(|error| panic!("could not compile Windows icon resource: {error}"));
    }

    println!("cargo:rerun-if-changed=packaging/windows/filebeam.rc");
    println!("cargo:rerun-if-changed=packaging/icons/filebeam.ico");
    println!("cargo:rerun-if-env-changed=FILEBEAM_RELEASE_VERSION");
    println!("cargo:rerun-if-env-changed=FILEBEAM_RELEASE_TAG");
    println!("cargo:rerun-if-env-changed=FILEBEAM_RELEASE_SHA");
    println!("cargo:rerun-if-env-changed=FILEBEAM_RELEASE_PUBLIC_KEY");

    let version = std::env::var("FILEBEAM_RELEASE_VERSION").unwrap_or_else(|_| {
        std::env::var("CARGO_PKG_VERSION").expect("Cargo supplies package version")
    });
    assert!(
        valid_version(&version),
        "FILEBEAM_RELEASE_VERSION must be X.Y.Z"
    );
    let tag = std::env::var("FILEBEAM_RELEASE_TAG").unwrap_or_else(|_| format!("v{version}"));
    assert_eq!(
        tag,
        format!("v{version}"),
        "FILEBEAM_RELEASE_TAG must be vX.Y.Z"
    );
    let sha = std::env::var("FILEBEAM_RELEASE_SHA").unwrap_or_else(|_| "0000000".into());
    assert!(
        !sha.is_empty()
            && sha
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-'),
        "FILEBEAM_RELEASE_SHA is invalid"
    );
    println!("cargo:rustc-env=FILEBEAM_VERSION={version}");
    println!("cargo:rustc-env=FILEBEAM_RELEASE_TAG={tag}");
    println!("cargo:rustc-env=FILEBEAM_RELEASE_SHA={sha}");
    if let Ok(key) = std::env::var("FILEBEAM_RELEASE_PUBLIC_KEY") {
        println!("cargo:rustc-env=FILEBEAM_RELEASE_PUBLIC_KEY={key}");
    }
}

fn valid_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}
