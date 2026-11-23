fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=../../assets/meetingnotes.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("app.rc", &["meetingnotes"], embed_resource::NONE)
            .manifest_required()
            .expect("Ressource d’icône Windows");
    }
}
