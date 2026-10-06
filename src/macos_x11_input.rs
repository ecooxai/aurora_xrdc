use std::{ffi::c_void, ptr};

use anyhow::{Result, anyhow};
use core_graphics::{
    display::CGDisplay,
    event::{CGEvent, CGEventTapLocation, CGEventType, CGKeyCode, CGMouseButton},
    event_source::{CGEventSource, CGEventSourceStateID},
    geometry::CGPoint,
};

/// macOS input backend. The historic type name is retained so the transport/session
/// layer can stay platform-neutral.
///
/// Do not store a CGEventSource in this struct: core-graphics intentionally marks
/// that wrapper !Send/!Sync, while session futures are spawned on Tokio's
/// multithreaded runtime. Creating a lightweight source per event keeps the
/// backend safe to move between tasks and matches Quartz's ownership model.
#[derive(Debug, Default, Clone, Copy)]
pub struct X11InputInjector;

pub fn screen_size(_display: &str) -> Result<(u16, u16)> {
    let bounds = CGDisplay::main().bounds();
    let width = bounds.size.width.round().clamp(1.0, u16::MAX as f64) as u16;
    let height = bounds.size.height.round().clamp(1.0, u16::MAX as f64) as u16;
    Ok((width, height))
}

impl X11InputInjector {
    pub fn connect(_display: &str) -> Result<Self> {
        // Verify Quartz event creation now so permission/runtime failures are
        // reported when the input session starts, not on the first click.
        let _ = source()?;
        Ok(Self)
    }

    pub fn queue_pointer_absolute(&self, x: i32, y: i32) -> Result<()> {
        post_mouse(CGEventType::MouseMoved, x, y, CGMouseButton::Left)
    }

    pub fn queue_pointer_relative(&self, dx: i32, dy: i32) -> Result<()> {
        let current = CGEvent::new(source()?)
            .map_err(|_| anyhow!("failed to create CoreGraphics event"))?
            .location();
        self.queue_pointer_absolute(current.x.round() as i32 + dx, current.y.round() as i32 + dy)
    }

    pub fn pointer_button(&self, button: u8, down: bool) -> Result<()> {
        let current = CGEvent::new(source()?)
            .map_err(|_| anyhow!("failed to create CoreGraphics event"))?
            .location();
        let (mouse, down_ty, up_ty) = mouse_button(button)?;
        let ty = if down { down_ty } else { up_ty };
        post_mouse(
            ty,
            current.x.round() as i32,
            current.y.round() as i32,
            mouse,
        )
    }

    pub fn pointer_click(&self, button: u8) -> Result<()> {
        self.queue_pointer_click(button)?;
        self.flush()
    }

    pub fn queue_pointer_click(&self, button: u8) -> Result<()> {
        if matches!(button, 4..=7) {
            let (vertical, horizontal) = match button {
                4 => (1, 0),
                5 => (-1, 0),
                6 => (0, -1),
                7 => (0, 1),
                _ => unreachable!(),
            };
            post_scroll(vertical, horizontal)?;
            return Ok(());
        }
        self.pointer_button(button, true)?;
        self.pointer_button(button, false)
    }

    pub fn key_event(&self, key: &str, down: bool) -> Result<()> {
        self.queue_key_event(key, down)?;
        self.flush()
    }

    pub fn supports_key(&self, key: &str) -> bool {
        key_name_to_keycode(key).is_some()
    }

    pub fn queue_key_event(&self, key: &str, down: bool) -> Result<()> {
        let code = key_name_to_keycode(key).ok_or_else(|| anyhow!("unsupported macOS key {key}"))?;
        let event = CGEvent::new_keyboard_event(source()?, code as CGKeyCode, down)
            .map_err(|_| anyhow!("failed to create CoreGraphics keyboard event"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    pub fn release_all_buttons(&self) -> Result<()> {
        for button in 1..=3 {
            let _ = self.pointer_button(button, false);
        }
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        Ok(())
    }
}

fn source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("failed to create CoreGraphics event source"))
}

fn post_mouse(
    ty: CGEventType,
    x: i32,
    y: i32,
    button: CGMouseButton,
) -> Result<()> {
    let event = CGEvent::new_mouse_event(
        source()?,
        ty,
        CGPoint::new(x as f64, y as f64),
        button,
    )
    .map_err(|_| anyhow!("failed to create CoreGraphics mouse event"))?;
    event.post(CGEventTapLocation::HID);
    Ok(())
}

// core-graphics 0.24 does not wrap CGEventCreateScrollWheelEvent, so use the
// stable Quartz C API directly. 1 = kCGScrollEventUnitLine and 0 = kCGHIDEventTap.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        ...
    ) -> *mut c_void;
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

fn post_scroll(vertical: i32, horizontal: i32) -> Result<()> {
    let event = unsafe {
        CGEventCreateScrollWheelEvent(
            ptr::null(),
            1,
            2,
            vertical,
            horizontal,
        )
    };
    if event.is_null() {
        return Err(anyhow!("failed to create CoreGraphics scroll event"));
    }
    unsafe {
        CGEventPost(0, event);
        CFRelease(event.cast_const());
    }
    Ok(())
}

fn mouse_button(button: u8) -> Result<(CGMouseButton, CGEventType, CGEventType)> {
    match button {
        1 => Ok((
            CGMouseButton::Left,
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
        )),
        2 => Ok((
            CGMouseButton::Center,
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
        )),
        3 => Ok((
            CGMouseButton::Right,
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
        )),
        _ => Err(anyhow!("unsupported macOS mouse button {button}")),
    }
}

fn key_name_to_keycode(key: &str) -> Option<u16> {
    Some(match key {
        "a" | "A" => 0,
        "s" | "S" => 1,
        "d" | "D" => 2,
        "f" | "F" => 3,
        "h" | "H" => 4,
        "g" | "G" => 5,
        "z" | "Z" => 6,
        "x" | "X" => 7,
        "c" | "C" => 8,
        "v" | "V" => 9,
        "b" | "B" => 11,
        "q" | "Q" => 12,
        "w" | "W" => 13,
        "e" | "E" => 14,
        "r" | "R" => 15,
        "y" | "Y" => 16,
        "t" | "T" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "equal" => 24,
        "9" => 25,
        "7" => 26,
        "minus" => 27,
        "8" => 28,
        "0" => 29,
        "bracketright" => 30,
        "o" | "O" => 31,
        "u" | "U" => 32,
        "bracketleft" => 33,
        "i" | "I" => 34,
        "p" | "P" => 35,
        "Return" | "KP_Enter" => 36,
        "l" | "L" => 37,
        "j" | "J" => 38,
        "apostrophe" => 39,
        "k" | "K" => 40,
        "semicolon" => 41,
        "backslash" => 42,
        "comma" => 43,
        "slash" => 44,
        "n" | "N" => 45,
        "m" | "M" => 46,
        "period" | "KP_Decimal" => 47,
        "Tab" => 48,
        "space" => 49,
        "grave" => 50,
        "Delete" | "BackSpace" => 51,
        "Escape" => 53,
        "Super_L" | "Super_R" => 55,
        "Shift_L" => 56,
        "Caps_Lock" => 57,
        "Alt_L" | "Alt_R" => 58,
        "Control_L" | "Control_R" => 59,
        "Shift_R" => 60,
        "F1" => 122,
        "F2" => 120,
        "F3" => 99,
        "F4" => 118,
        "F5" => 96,
        "F6" => 97,
        "F7" => 98,
        "F8" => 100,
        "F9" => 101,
        "F10" => 109,
        "F11" => 103,
        "F12" => 111,
        "Home" => 115,
        "End" => 119,
        "Page_Up" => 116,
        "Page_Down" => 121,
        "Left" => 123,
        "Right" => 124,
        "Down" => 125,
        "Up" => 126,
        _ => return None,
    })
}
