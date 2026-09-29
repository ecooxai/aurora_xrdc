//! Portable entry point: no shell, systemd, package manager, or shared libraries at runtime.
#[path = "../launcher/mod.rs"]
mod launcher;
fn main() {
    if let Err(error) = launcher::run() {
        eprintln!("[aurora] {error:#}");
        std::process::exit(1);
    }
}
