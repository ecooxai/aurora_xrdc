use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    mem,
    os::fd::AsRawFd,
    slice, thread,
    time::Duration,
};

use anyhow::{Context, Result};

const UINPUT_PATH: &str = "/dev/uinput";
const UINPUT_IOCTL_BASE: u64 = b'U' as u64;
const IOC_NRBITS: u64 = 8;
const IOC_TYPEBITS: u64 = 8;
const IOC_SIZEBITS: u64 = 14;
const IOC_NRSHIFT: u64 = 0;
const IOC_TYPESHIFT: u64 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u64 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u64 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_NONE: u64 = 0;
const IOC_WRITE: u64 = 1;

const EV_SYN: i32 = 0x00;
const EV_KEY: i32 = 0x01;
const EV_REL: i32 = 0x02;
const SYN_REPORT: i32 = 0;
const REL_X: i32 = 0x00;
const REL_Y: i32 = 0x01;
const REL_HWHEEL: i32 = 0x06;
const REL_WHEEL: i32 = 0x08;
const REL_WHEEL_HI_RES: i32 = 0x0b;
const REL_HWHEEL_HI_RES: i32 = 0x0c;
const BTN_LEFT: i32 = 0x110;
const BTN_RIGHT: i32 = 0x111;
const BTN_MIDDLE: i32 = 0x112;
const BUS_USB: u16 = 0x03;
const HI_RES_UNITS_PER_DETENT: i32 = 120;

fn ioc(dir: u64, type_: u64, nr: u64, size: u64) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT)
        | (type_ << IOC_TYPESHIFT)
        | (nr << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)) as libc::c_ulong
}

fn io(type_: u64, nr: u64) -> libc::c_ulong {
    ioc(IOC_NONE, type_, nr, 0)
}

fn iow<T>(type_: u64, nr: u64) -> libc::c_ulong {
    ioc(IOC_WRITE, type_, nr, mem::size_of::<T>() as u64)
}

fn ui_dev_create() -> libc::c_ulong {
    io(UINPUT_IOCTL_BASE, 1)
}

fn ui_dev_destroy() -> libc::c_ulong {
    io(UINPUT_IOCTL_BASE, 2)
}

fn ui_set_evbit() -> libc::c_ulong {
    iow::<libc::c_int>(UINPUT_IOCTL_BASE, 100)
}

fn ui_set_keybit() -> libc::c_ulong {
    iow::<libc::c_int>(UINPUT_IOCTL_BASE, 101)
}

fn ui_set_relbit() -> libc::c_ulong {
    iow::<libc::c_int>(UINPUT_IOCTL_BASE, 102)
}

pub struct UInputPointerInjector {
    device: File,
}

impl UInputPointerInjector {
    pub fn connect() -> Result<Self> {
        let mut device = OpenOptions::new()
            .read(true)
            .write(true)
            .open(UINPUT_PATH)
            .with_context(|| format!("failed to open {UINPUT_PATH}"))?;
        let fd = device.as_raw_fd();
        ioctl(fd, ui_set_evbit(), EV_KEY).context("failed to enable uinput EV_KEY")?;
        ioctl(fd, ui_set_evbit(), EV_REL).context("failed to enable uinput EV_REL")?;
        ioctl(fd, ui_set_keybit(), BTN_LEFT).context("failed to enable uinput BTN_LEFT")?;
        ioctl(fd, ui_set_keybit(), BTN_RIGHT).context("failed to enable uinput BTN_RIGHT")?;
        ioctl(fd, ui_set_keybit(), BTN_MIDDLE).context("failed to enable uinput BTN_MIDDLE")?;
        ioctl(fd, ui_set_relbit(), REL_X).context("failed to enable uinput REL_X")?;
        ioctl(fd, ui_set_relbit(), REL_Y).context("failed to enable uinput REL_Y")?;

        let mut user_dev: libc::uinput_user_dev = unsafe { mem::zeroed() };
        write_device_name(&mut user_dev.name, "vibe-rdesk pointer");
        user_dev.id.bustype = BUS_USB;
        user_dev.id.vendor = 0x5652;
        user_dev.id.product = 0x4454;
        user_dev.id.version = 1;
        write_struct(&mut device, &user_dev).context("failed to write uinput device setup")?;
        ioctl(fd, ui_dev_create(), 0).context("failed to create uinput device")?;
        thread::sleep(Duration::from_millis(100));

        Ok(Self { device })
    }

    pub fn emit_motion(&mut self, dx: i32, dy: i32) -> Result<()> {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        if dx != 0 {
            write_input_event(&mut self.device, EV_REL, REL_X, dx)?;
        }
        if dy != 0 {
            write_input_event(&mut self.device, EV_REL, REL_Y, dy)?;
        }
        write_input_event(&mut self.device, EV_SYN, SYN_REPORT, 0)?;
        self.device
            .flush()
            .context("failed to flush uinput pointer motion")
    }
}

impl Drop for UInputPointerInjector {
    fn drop(&mut self) {
        let _ = ioctl(self.device.as_raw_fd(), ui_dev_destroy(), 0);
    }
}

pub struct UInputWheelInjector {
    device: File,
    vertical_remainder: i32,
    horizontal_remainder: i32,
}

impl UInputWheelInjector {
    pub fn connect() -> Result<Self> {
        let mut device = OpenOptions::new()
            .read(true)
            .write(true)
            .open(UINPUT_PATH)
            .with_context(|| format!("failed to open {UINPUT_PATH}"))?;
        let fd = device.as_raw_fd();
        ioctl(fd, ui_set_evbit(), EV_REL).context("failed to enable uinput EV_REL")?;
        ioctl(fd, ui_set_relbit(), REL_WHEEL).context("failed to enable uinput REL_WHEEL")?;
        ioctl(fd, ui_set_relbit(), REL_WHEEL_HI_RES)
            .context("failed to enable uinput REL_WHEEL_HI_RES")?;
        ioctl(fd, ui_set_relbit(), REL_HWHEEL).context("failed to enable uinput REL_HWHEEL")?;
        ioctl(fd, ui_set_relbit(), REL_HWHEEL_HI_RES)
            .context("failed to enable uinput REL_HWHEEL_HI_RES")?;

        let mut user_dev: libc::uinput_user_dev = unsafe { mem::zeroed() };
        write_device_name(&mut user_dev.name, "vibe-rdesk smooth wheel");
        user_dev.id.bustype = BUS_USB;
        user_dev.id.vendor = 0x5652;
        user_dev.id.product = 0x4453;
        user_dev.id.version = 1;
        write_struct(&mut device, &user_dev).context("failed to write uinput device setup")?;
        ioctl(fd, ui_dev_create(), 0).context("failed to create uinput device")?;
        thread::sleep(Duration::from_millis(100));

        Ok(Self {
            device,
            vertical_remainder: 0,
            horizontal_remainder: 0,
        })
    }

    pub fn emit_scroll(&mut self, horizontal_hi_res: i32, vertical_hi_res: i32) -> Result<()> {
        if horizontal_hi_res == 0 && vertical_hi_res == 0 {
            return Ok(());
        }
        if horizontal_hi_res != 0 {
            self.write_event(EV_REL, REL_HWHEEL_HI_RES, horizontal_hi_res)?;
            let detents = detents_for_hi_res(&mut self.horizontal_remainder, horizontal_hi_res);
            if detents != 0 {
                self.write_event(EV_REL, REL_HWHEEL, detents)?;
            }
        }
        if vertical_hi_res != 0 {
            self.write_event(EV_REL, REL_WHEEL_HI_RES, vertical_hi_res)?;
            let detents = detents_for_hi_res(&mut self.vertical_remainder, vertical_hi_res);
            if detents != 0 {
                self.write_event(EV_REL, REL_WHEEL, detents)?;
            }
        }
        self.write_event(EV_SYN, SYN_REPORT, 0)?;
        self.device.flush().context("failed to flush uinput scroll")
    }

    fn write_event(&mut self, type_: i32, code: i32, value: i32) -> Result<()> {
        write_input_event(&mut self.device, type_, code, value)
    }
}

impl Drop for UInputWheelInjector {
    fn drop(&mut self) {
        let _ = ioctl(self.device.as_raw_fd(), ui_dev_destroy(), 0);
    }
}

fn detents_for_hi_res(remainder: &mut i32, value: i32) -> i32 {
    *remainder += value;
    let detents = *remainder / HI_RES_UNITS_PER_DETENT;
    *remainder -= detents * HI_RES_UNITS_PER_DETENT;
    detents
}

fn write_device_name(target: &mut [libc::c_char], name: &str) {
    for (dst, src) in target.iter_mut().zip(name.bytes()) {
        *dst = src as libc::c_char;
    }
}

fn write_struct<T>(writer: &mut File, value: &T) -> io::Result<()> {
    let bytes =
        unsafe { slice::from_raw_parts((value as *const T).cast::<u8>(), mem::size_of::<T>()) };
    writer.write_all(bytes)
}

fn write_input_event(device: &mut File, type_: i32, code: i32, value: i32) -> Result<()> {
    let event = libc::input_event {
        time: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        type_: type_ as u16,
        code: code as u16,
        value,
    };
    write_struct(device, &event).context("failed to write uinput event")
}

fn ioctl(fd: libc::c_int, request: libc::c_ulong, value: libc::c_int) -> io::Result<()> {
    let result = unsafe { libc::ioctl(fd, request, value) };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
