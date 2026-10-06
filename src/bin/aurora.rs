//! Platform launcher entry point.
#[cfg(target_os = "linux")]
#[path = "../launcher/mod.rs"]
mod launcher;

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = launcher::run() {
        eprintln!("[aurora] {error:#}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
fn main() {
    eprintln!("apple_xrdc: run the main server binary with: cargo run -- --passwd <password>");
}
