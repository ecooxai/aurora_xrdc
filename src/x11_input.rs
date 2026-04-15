use anyhow::{Context, Result};
use x11rb::{
    NONE,
    connection::Connection,
    protocol::{
        xproto::{BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt as XProtoConnectionExt, Window},
        xtest::ConnectionExt as XTestConnectionExt,
    },
    rust_connection::RustConnection,
};

pub struct X11InputInjector {
    connection: RustConnection,
    root: Window,
}

impl X11InputInjector {
    pub fn connect(display: &str) -> Result<Self> {
        let (connection, screen_num) =
            x11rb::connect(Some(display)).with_context(|| format!("failed to connect to X11 display {display}"))?;
        connection
            .xtest_get_version(2, 2)
            .context("failed to query XTEST extension")?
            .reply()
            .context("failed to negotiate XTEST extension")?;
        let root = connection
            .setup()
            .roots
            .get(screen_num)
            .context("X11 setup missing default screen")?
            .root;
        Ok(Self { connection, root })
    }

    pub fn queue_pointer_absolute(&self, x: i32, y: i32) -> Result<()> {
        self.connection
            .warp_pointer(NONE, self.root, 0, 0, 0, 0, clamp_i16(x), clamp_i16(y))
            .context("failed to warp X11 pointer absolutely")?;
        Ok(())
    }

    pub fn queue_pointer_relative(&self, dx: i32, dy: i32) -> Result<()> {
        self.connection
            .warp_pointer(NONE, NONE, 0, 0, 0, 0, clamp_i16(dx), clamp_i16(dy))
            .context("failed to warp X11 pointer relatively")?;
        Ok(())
    }

    pub fn pointer_button(&self, button: u8, down: bool) -> Result<()> {
        self.queue_pointer_button(button, down)?;
        self.flush()
    }

    pub fn pointer_click(&self, button: u8) -> Result<()> {
        self.queue_pointer_button(button, true)?;
        self.queue_pointer_button(button, false)?;
        self.flush()
    }

    pub fn release_all_buttons(&self) -> Result<()> {
        for button in 1..=5 {
            self.queue_pointer_button(button, false)?;
        }
        self.flush()
    }

    pub fn flush(&self) -> Result<()> {
        self.connection.flush().context("failed to flush X11 input requests")
    }

    fn queue_pointer_button(&self, button: u8, down: bool) -> Result<()> {
        let event_type = if down {
            BUTTON_PRESS_EVENT
        } else {
            BUTTON_RELEASE_EVENT
        };
        self.connection
            .xtest_fake_input(event_type, button, 0, self.root, 0, 0, 0)
            .context("failed to queue XTEST pointer button event")?;
        Ok(())
    }
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}
