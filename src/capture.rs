pub enum Event {
    Key(u32, bool),
    Pointer(i32, i32, Option<(u32, bool)>),
    Release,
}

#[cfg(target_os = "linux")]
mod platform {
    use super::Event;
    use std::{ffi::CStr, ptr};
    use x11_dl::{keysym, xlib};

    pub struct Capture {
        xlib: xlib::Xlib,
        display: *mut xlib::Display,
    }

    impl Capture {
        pub fn start() -> Result<Self, &'static str> {
            if std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
            {
                return Err("Full input capture requires an X11 session.");
            }

            let xlib = xlib::Xlib::open().map_err(|_| "Could not load X11 input support.")?;
            // A separate client receives the grabbed events instead of the window toolkit.
            let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
            if display.is_null() {
                return Err("Could not connect to the X11 display.");
            }
            let capture = Self { xlib, display };
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

                (xlib.XSelectInput)(display, window, xlib::StructureNotifyMask);
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
                            let group = ((key.state >> 13) & 3) as i32;
                            let lookup = |group| {
                                (self.xlib.XkbKeycodeToKeysym)(
                                    self.display,
                                    key.keycode as u8,
                                    group,
                                    0,
                                ) as u32
                            };
                            let symbol = match lookup(group) {
                                0 if group != 0 => lookup(0),
                                symbol => symbol,
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

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::Event;

    pub struct Capture;

    impl Capture {
        pub fn start() -> Result<Self, &'static str> {
            Err("Full input capture requires a Linux X11 session.")
        }

        pub fn events(&self) -> impl Iterator<Item = Event> {
            std::iter::empty()
        }
    }
}

pub use platform::Capture;
