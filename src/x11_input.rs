use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use x11rb::{
    NONE,
    connection::Connection,
    protocol::{
        xproto::{
            BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt as XProtoConnectionExt,
            KEY_PRESS_EVENT, KEY_RELEASE_EVENT, Keycode, Keysym, Window,
        },
        xtest::ConnectionExt as XTestConnectionExt,
    },
    rust_connection::RustConnection,
};

pub struct X11InputInjector {
    connection: RustConnection,
    root: Window,
    keycodes: HashMap<Keysym, Keycode>,
}

pub fn screen_size(display: &str) -> Result<(u16, u16)> {
    let (connection, screen_num) = x11rb::connect(Some(display))
        .with_context(|| format!("failed to connect to X11 display {display}"))?;
    let screen = connection
        .setup()
        .roots
        .get(screen_num)
        .context("X11 setup missing default screen")?;
    Ok((screen.width_in_pixels, screen.height_in_pixels))
}

impl X11InputInjector {
    pub fn connect(display: &str) -> Result<Self> {
        let (connection, screen_num) = x11rb::connect(Some(display))
            .with_context(|| format!("failed to connect to X11 display {display}"))?;
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
        let keycodes = load_keycodes(&connection)?;
        Ok(Self {
            connection,
            root,
            keycodes,
        })
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

    pub fn queue_pointer_click(&self, button: u8) -> Result<()> {
        self.queue_pointer_button(button, true)?;
        self.queue_pointer_button(button, false)
    }

    pub fn key_event(&self, key: &str, down: bool) -> Result<()> {
        self.queue_key_event(key, down)?;
        self.flush()
    }

    pub fn supports_key(&self, key: &str) -> bool {
        key_name_to_keysym(key).is_some_and(|keysym| self.keycodes.contains_key(&keysym))
    }

    pub fn queue_key_event(&self, key: &str, down: bool) -> Result<()> {
        let keysym =
            key_name_to_keysym(key).ok_or_else(|| anyhow!("unsupported X11 key {key}"))?;
        let keycode = self
            .keycodes
            .get(&keysym)
            .copied()
            .ok_or_else(|| anyhow!("X11 key {key} is not present in keyboard map"))?;
        let event_type = if down {
            KEY_PRESS_EVENT
        } else {
            KEY_RELEASE_EVENT
        };
        self.connection
            .xtest_fake_input(event_type, keycode, 0, self.root, 0, 0, 0)
            .with_context(|| format!("failed to queue XTEST key event for {key}"))?;
        Ok(())
    }

    pub fn release_all_buttons(&self) -> Result<()> {
        for button in 1..=5 {
            self.queue_pointer_button(button, false)?;
        }
        self.flush()
    }

    pub fn flush(&self) -> Result<()> {
        self.connection
            .flush()
            .context("failed to flush X11 input requests")
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

fn load_keycodes(connection: &RustConnection) -> Result<HashMap<Keysym, Keycode>> {
    let setup = connection.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;
    let count = max
        .checked_sub(min)
        .and_then(|value| value.checked_add(1))
        .context("invalid X11 keyboard keycode range")?;
    let mapping = connection
        .get_keyboard_mapping(min, count)
        .context("failed to request X11 keyboard map")?
        .reply()
        .context("failed to read X11 keyboard map")?;
    let per_keycode = usize::from(mapping.keysyms_per_keycode);
    if per_keycode == 0 {
        return Err(anyhow!("X11 keyboard map has no keysyms per keycode"));
    }
    let mut keycodes = HashMap::new();
    for (index, keysyms) in mapping.keysyms.chunks(per_keycode).enumerate() {
        let keycode = min + u8::try_from(index).context("X11 keycode index overflow")?;
        for keysym in keysyms.iter().copied().filter(|keysym| *keysym != 0) {
            keycodes.entry(keysym).or_insert(keycode);
        }
    }
    Ok(keycodes)
}

fn key_name_to_keysym(key: &str) -> Option<Keysym> {
    match key {
        "BackSpace" => Some(0xff08),
        "Tab" => Some(0xff09),
        "Return" => Some(0xff0d),
        "Escape" => Some(0xff1b),
        "Delete" => Some(0xffff),
        "Insert" => Some(0xff63),
        "Home" => Some(0xff50),
        "End" => Some(0xff57),
        "Page_Up" => Some(0xff55),
        "Page_Down" => Some(0xff56),
        "Left" => Some(0xff51),
        "Up" => Some(0xff52),
        "Right" => Some(0xff53),
        "Down" => Some(0xff54),
        "Shift_L" => Some(0xffe1),
        "Shift_R" => Some(0xffe2),
        "Control_L" => Some(0xffe3),
        "Control_R" => Some(0xffe4),
        "Caps_Lock" => Some(0xffe5),
        "Alt_L" => Some(0xffe9),
        "Alt_R" => Some(0xffea),
        "Super_L" => Some(0xffeb),
        "Super_R" => Some(0xffec),
        "KP_Enter" => Some(0xff8d),
        "KP_Decimal" => Some(0xffae),
        "space" => Some(0x0020),
        "grave" => Some(0x0060),
        "minus" => Some(0x002d),
        "equal" => Some(0x003d),
        "bracketleft" => Some(0x005b),
        "bracketright" => Some(0x005d),
        "backslash" => Some(0x005c),
        "semicolon" => Some(0x003b),
        "apostrophe" => Some(0x0027),
        "comma" => Some(0x002c),
        "period" => Some(0x002e),
        "slash" => Some(0x002f),
        _ => {
            if let Some(number) = key
                .strip_prefix('F')
                .and_then(|value| value.parse::<u32>().ok())
                && (1..=12).contains(&number)
            {
                return Some(0xffbd + number);
            }
            let mut chars = key.chars();
            let ch = chars.next()?;
            if chars.next().is_none() && ch.is_ascii() {
                return Some(ch as Keysym);
            }
            None
        }
    }
}
