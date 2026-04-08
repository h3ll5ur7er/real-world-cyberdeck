//! cyberdeck-kbd – Keyboard daemon for the real-world-cyberdeck.
//!
//! Reads physical key events via evdev, applies key mappings (remap / macro /
//! special functions such as OTP), and writes USB HID reports to the gadget
//! device.

mod config;
mod hid;
mod keymapper;
mod otp;

use clap::Parser;
use log::{error, info, warn};
use std::fs::File;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::process;

/// Command-line arguments.
#[derive(Parser, Debug)]
#[command(name = "cyberdeck-kbd", about = "Real-world cyberdeck keyboard daemon")]
struct Args {
    /// Path to the keymap configuration file.
    #[arg(short, long, default_value = "/etc/cyberdeck/keymap.toml")]
    config: PathBuf,
}

/// Size of a Linux `input_event` struct on a 64-bit system (see linux/input.h).
const INPUT_EVENT_SIZE: usize = 24;

/// EV_KEY event type (linux/input-event-codes.h).
const EV_KEY: u16 = 0x01;

/// Key press value (linux/input-event-codes.h: value=1 means key down).
const KEY_PRESS: i32 = 1;

/// Key release value (linux/input-event-codes.h: value=0 means key up).
const KEY_RELEASE: i32 = 0;

/// EVIOCGRAB ioctl number: _IOW('E', 0x90, int) — exclusively grabs an input
/// device so events are delivered only to this process.  Without this the Pi's
/// local TTY/desktop would also receive every keystroke, which is wrong when
/// the keyboard is being forwarded to the USB HID gadget.
const EVIOCGRAB: libc::c_ulong = 0x40044590;

fn main() {
    env_logger::init();

    let args = Args::parse();

    let cfg = match config::Config::load(&args.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config {:?}: {}", args.config, e);
            process::exit(1);
        }
    };

    info!("Loaded config from {:?}", args.config);
    info!("Keyboard device: {}", cfg.keyboard_device);
    info!("HID device: {}", cfg.hid_device);

    let mapper = keymapper::KeyMapper::new(&cfg);
    info!("KeyMapper ready ({} remaps, {} macros, {} specials)",
        cfg.remap.len(), cfg.macros.len(), cfg.special.len());

    // Open evdev device.
    let mut evdev = match File::open(&cfg.keyboard_device) {
        Ok(f) => f,
        Err(e) => {
            error!("Cannot open keyboard device {}: {}", cfg.keyboard_device, e);
            process::exit(1);
        }
    };

    // Exclusively grab the keyboard so events are delivered only to this
    // daemon and not also to the Pi's local TTY/desktop.
    // Safety: EVIOCGRAB is a well-defined ioctl on any evdev fd.
    let grab_result = unsafe { libc::ioctl(evdev.as_raw_fd(), EVIOCGRAB, 1 as libc::c_int) };
    if grab_result != 0 {
        warn!(
            "EVIOCGRAB failed on {} (errno {}). Keystrokes will also reach \
             the Pi's local console — this is normally wrong when running as \
             a USB keyboard.",
            cfg.keyboard_device,
            std::io::Error::last_os_error()
        );
    } else {
        info!("Exclusively grabbed {}", cfg.keyboard_device);
    }

    // Open HID gadget device.
    let mut hid_writer = match hid::HidWriter::open(&cfg.hid_device) {
        Ok(w) => w,
        Err(e) => {
            error!("Cannot open HID device {}: {}", cfg.hid_device, e);
            process::exit(1);
        }
    };

    info!("Entering event loop...");

    let mut buf = [0u8; INPUT_EVENT_SIZE];
    loop {
        if let Err(e) = evdev.read_exact(&mut buf) {
            error!("Read error from evdev: {}", e);
            break;
        }

        // Parse input_event: { tv_sec: u64, tv_usec: u64, type: u16, code: u16, value: i32 }
        let ev_type = u16::from_ne_bytes([buf[16], buf[17]]);
        let ev_code = u16::from_ne_bytes([buf[18], buf[19]]);
        let ev_value = i32::from_ne_bytes([buf[20], buf[21], buf[22], buf[23]]);

        if ev_type != EV_KEY {
            continue;
        }

        let is_press = ev_value == KEY_PRESS;
        let is_release = ev_value == KEY_RELEASE;

        if !is_press && !is_release {
            // Ignore repeat events for now.
            continue;
        }

        let actions = mapper.translate(ev_code, is_press);

        for action in &actions {
            match action {
                keymapper::Action::Key { code, pressed } => {
                    if let Err(e) = hid_writer.send_key(*code, *pressed) {
                        error!("HID write error: {}", e);
                    }
                }
                keymapper::Action::TypeString(s) => {
                    if let Err(e) = hid_writer.type_string(s) {
                        error!("HID type_string error: {}", e);
                    }
                }
            }
        }
    }
}
