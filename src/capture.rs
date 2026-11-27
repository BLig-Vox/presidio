pub enum Event {
    Key(u32, bool),
    Pointer(i32, i32, Option<(u32, bool)>),
    Release,
    Failed(&'static str),
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::Event;
    use std::{cell::Cell, ffi::CStr, ptr};
    use x11_dl::{keysym, xlib};

    pub struct Capture {
        xlib: xlib::Xlib,
        display: *mut xlib::Display,
        keys: [Cell<u32>; 256],
    }

    impl Capture {
        pub fn start() -> Result<Self, &'static str> {
            let xlib = xlib::Xlib::open().map_err(|_| "Could not load X11 input support.")?;
            // A separate client receives the grabbed events instead of the window toolkit.
            let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
            if display.is_null() {
                return Err("Could not connect to the X11 display.");
            }
            let capture = Self {
                xlib,
                display,
                keys: std::array::from_fn(|_| Cell::new(0)),
            };
            capture.grab()?;
            Ok(capture)
        }

        fn grab(&self) -> Result<(), &'static str> {
            let xlib = &self.xlib;
            let display = self.display;
            unsafe {
                let mut window = 0;
                let mut revert = 0;
                (xlib.XGetInputFocus)(display, &mut window, &mut revert);
                // None and PointerRoot are special focus values, not windows.
                if window <= 1 {
                    return Err("Focus Presidio before capturing input.");
                }
                let mut class: xlib::XClassHint = std::mem::zeroed();
                if (xlib.XGetClassHint)(display, window, &mut class) == 0 {
                    return Err("Focus Presidio before capturing input.");
                }
                let is_presidio = !class.res_class.is_null()
                    && CStr::from_ptr(class.res_class).to_bytes() == b"presidio";
                (xlib.XFree)(class.res_name.cast());
                (xlib.XFree)(class.res_class.cast());
                if !is_presidio {
                    return Err("Focus Presidio before capturing input.");
                }

                (xlib.XSelectInput)(
                    display,
                    window,
                    xlib::StructureNotifyMask | xlib::FocusChangeMask,
                );
                (xlib.XkbSetDetectableAutoRepeat)(display, xlib::True, ptr::null_mut());
                let keyboard = (xlib.XGrabKeyboard)(
                    display,
                    window,
                    xlib::False,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                    xlib::CurrentTime,
                );
                if keyboard != xlib::GrabSuccess {
                    return Err("The keyboard is unavailable for capture.");
                }
                let mask =
                    xlib::ButtonPressMask | xlib::ButtonReleaseMask | xlib::PointerMotionMask;
                let pointer = (xlib.XGrabPointer)(
                    display,
                    window,
                    xlib::False,
                    mask as u32,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                    window,
                    0,
                    xlib::CurrentTime,
                );
                if pointer != xlib::GrabSuccess {
                    return Err("The mouse is unavailable for capture.");
                }
                Ok(())
            }
        }

        pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
            (0..256)
                .map_while(|_| unsafe {
                    if (self.xlib.XPending)(self.display) == 0 {
                        return None;
                    }
                    let mut event = std::mem::zeroed::<xlib::XEvent>();
                    (self.xlib.XNextEvent)(self.display, &mut event);
                    Some(match event.get_type() {
                        xlib::KeyPress | xlib::KeyRelease => {
                            let key = event.key;
                            let down = key.type_ == xlib::KeyPress;
                            let keycode = key.keycode as u8;
                            let held = &self.keys[keycode as usize];
                            let symbol = if down {
                                if held.get() != 0 {
                                    return Some(None);
                                }
                                let mut symbol = 0;
                                (self.xlib.XkbLookupKeySym)(
                                    self.display,
                                    keycode,
                                    key.state,
                                    &mut 0,
                                    &mut symbol,
                                );
                                let symbol = symbol as u32;
                                held.set(symbol);
                                symbol
                            } else {
                                held.replace(0)
                            };
                            if down
                                && symbol == keysym::XK_Escape
                                && key.state & xlib::Mod4Mask != 0
                            {
                                Some(Event::Release)
                            } else if symbol != 0 {
                                Some(Event::Key(symbol, down))
                            } else {
                                None
                            }
                        }
                        xlib::MotionNotify => {
                            Some(Event::Pointer(event.motion.x, event.motion.y, None))
                        }
                        xlib::ButtonPress | xlib::ButtonRelease => {
                            let button = event.button;
                            Some(Event::Pointer(
                                button.x,
                                button.y,
                                Some((button.button, button.type_ == xlib::ButtonPress)),
                            ))
                        }
                        xlib::FocusOut
                            if matches!(
                                event.focus_change.mode,
                                xlib::NotifyNormal | xlib::NotifyWhileGrabbed
                            ) && event.focus_change.detail != xlib::NotifyInferior =>
                        {
                            Some(Event::Release)
                        }
                        xlib::UnmapNotify | xlib::DestroyNotify => Some(Event::Release),
                        _ => None,
                    })
                })
                .flatten()
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            // Closing the grab client releases both devices, including a partial grab.
            unsafe { (self.xlib.XCloseDisplay)(self.display) };
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{x11, Event};
    use macroquad::miniquad::native::linux_wayland::input_capture::{self, Status};
    use std::time::{Duration, Instant};

    pub enum Capture {
        X11(x11::Capture),
        Wayland(Instant),
    }

    impl Capture {
        pub fn start() -> Result<Self, &'static str> {
            match input_capture::status() {
                Status::NotWayland if super::wayland_session() => {
                    Err("Full input capture needs a native Wayland window, not XWayland.")
                }
                Status::NotWayland => x11::Capture::start().map(Self::X11),
                Status::Unsupported => Err(
                    "This compositor does not support pointer confinement and shortcut inhibition.",
                ),
                Status::Idle if input_capture::request(true) => Ok(Self::Wayland(Instant::now())),
                _ => Err("Could not request Wayland input capture."),
            }
        }

        pub fn pending(&self) -> bool {
            matches!(self, Self::Wayland(_)) && input_capture::status() == Status::Pending
        }

        pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
            let empty = Vec::new().into_iter();
            let (x11, wayland) = match self {
                Self::X11(grab) => (Some(grab.events()), empty),
                Self::Wayland(started) => {
                    let mut events: Vec<_> = input_capture::events()
                        .into_iter()
                        .map(|event| match event {
                            input_capture::Event::Key(symbol, down) => Event::Key(symbol, down),
                            input_capture::Event::Pointer(x, y, button) => {
                                Event::Pointer(x, y, button)
                            }
                            input_capture::Event::Release => Event::Release,
                        })
                        .collect();
                    if self.pending() && started.elapsed() >= Duration::from_secs(1) {
                        events.push(Event::Failed(
                            "Wayland capture was not granted. Click the monitor to retry.",
                        ));
                    }
                    (None, events.into_iter())
                }
            };
            x11.into_iter().flatten().chain(wayland)
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            if matches!(self, Self::Wayland(_)) {
                input_capture::request(false);
            }
        }
    }
}

pub fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var_os("WAYLAND_SOCKET").is_some()
        || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::Event;

    pub struct Capture;

    impl Capture {
        pub fn start() -> Result<Self, &'static str> {
            Err("Full input capture is supported on Linux.")
        }

        pub fn pending(&self) -> bool {
            false
        }

        pub fn events(&self) -> impl Iterator<Item = Event> {
            std::iter::empty()
        }
    }
}

pub use platform::Capture;
