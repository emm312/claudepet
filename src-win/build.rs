fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("../Resources/AppIcon/AppIcon.ico")
            .compile()
            .expect("failed to embed AppIcon.ico into the exe");
    }
}
