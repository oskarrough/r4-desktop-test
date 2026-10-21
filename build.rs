//! Gives the Windows exe its icon, for Explorer, the taskbar and Start Menu shortcuts.
fn main() {
    println!("cargo:rerun-if-changed=assets/radio4000.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/radio4000.ico")
            .set("ProductName", "Radio4000")
            // Task Manager and the volume mixer name the process by its description.
            .set("FileDescription", "Radio4000")
            .compile()
            .expect("embed the Windows icon");
    }
}
