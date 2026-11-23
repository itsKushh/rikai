fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=../../assets/rikai.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = ["MAJOR", "MINOR", "PATCH"].map(|part| {
            format!(
                "RIKAI_{part}={}",
                std::env::var(format!("CARGO_PKG_VERSION_{part}")).unwrap()
            )
        });
        embed_resource::compile_for("app.rc", &["rikai"], version)
            .manifest_required()
            .expect("Ressource d’icône Windows");
    }
}
