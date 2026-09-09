fn main() {
    println!("cargo:rerun-if-env-changed=BEAM_RELEASE_PUBLIC_KEY");
    if let Ok(key) = std::env::var("BEAM_RELEASE_PUBLIC_KEY") {
        println!("cargo:rustc-env=BEAM_RELEASE_PUBLIC_KEY={key}");
    }
}
