fn main() {
    println!("cargo:rerun-if-changed=assets/secblitz.rc");
    println!("cargo:rerun-if-changed=assets/secblitz.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/secblitz.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embed Windows manifest");
    }
}
