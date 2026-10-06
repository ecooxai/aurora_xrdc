use anyhow::{Result, anyhow};

pub struct UInputPointerInjector;
impl UInputPointerInjector {
    pub fn connect() -> Result<Self> { Err(anyhow!("uinput is Linux-only; macOS uses CoreGraphics")) }
    pub fn emit_motion(&mut self, _dx: i32, _dy: i32) -> Result<()> { Err(anyhow!("uinput unavailable on macOS")) }
}

pub struct UInputWheelInjector;
impl UInputWheelInjector {
    pub fn connect() -> Result<Self> { Err(anyhow!("uinput is Linux-only; macOS uses CoreGraphics scroll events")) }
    pub fn emit_scroll(&mut self, _horizontal: i32, _vertical: i32) -> Result<()> { Err(anyhow!("uinput unavailable on macOS")) }
}
