#![allow(non_camel_case_types, non_upper_case_globals)]
use super::{extensions::cursor::*, libwayland_client::*, WaylandPayload};
use crate::{wayland_interface, wl_request, wl_request_constructor};
use std::{cell::RefCell, ffi::c_void, ptr};

wayland_interface!(
    inhibit_manager,
    zwp_keyboard_shortcuts_inhibit_manager_v1,
    1,
    [(destroy, "", ()), (inhibit_shortcuts, "noo", (inhibitor))],
    []
);
wayland_interface!(
    inhibitor,
    zwp_keyboard_shortcuts_inhibitor_v1,
    1,
    [(destroy, "", ())],
    [("active", ""), ("inactive", "")]
);
wayland_interface!(
    focus_manager,
    hyprland_focus_grab_manager_v1,
    1,
    [(create_grab, "n", (focus_grab)), (destroy, "", ())],
    []
);
wayland_interface!(
    focus_grab,
    hyprland_focus_grab_v1,
    1,
    [
        (add_surface, "o", ()),
        (remove_surface, "o", ()),
        (commit, "", ()),
        (destroy, "", ())
    ],
    [("cleared", "")]
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    NotWayland,
    Unsupported,
    Idle,
    Pending,
    Active,
    Lost,
}

pub enum Event {
    Key(u32, bool),
    Pointer(i32, i32, Option<(u32, bool)>),
    Release,
}

struct Input {
    status: Status,
    events: Vec<Event>,
}

thread_local! {
    static INPUT: RefCell<Input> = const { RefCell::new(Input {
        status: Status::NotWayland,
        events: Vec::new(),
    }) };
}

pub fn status() -> Status {
    INPUT.with(|input| input.borrow().status)
}

pub fn request(enabled: bool) -> bool {
    if enabled && status() != Status::Idle {
        return false;
    }
    if enabled {
        set_status(Status::Pending);
    } else {
        set_status(Status::Idle);
        INPUT.with(|input| input.borrow_mut().events.clear());
    }
    crate::native_display()
        .lock()
        .unwrap()
        .native_requests
        .send(crate::native::Request::SetInputCapture(enabled))
        .is_ok()
}

pub fn events() -> Vec<Event> {
    INPUT.with(|input| std::mem::take(&mut input.borrow_mut().events))
}

fn set_status(status: Status) {
    INPUT.with(|input| input.borrow_mut().status = status);
}

fn emit(event: Event) {
    INPUT.with(|input| input.borrow_mut().events.push(event));
}

pub(super) fn key(symbol: u32, down: bool, logo: bool) {
    if matches!(status(), Status::Pending | Status::Active) && down && logo && symbol == 0xff1b {
        lose_capture();
    } else if status() == Status::Active && symbol != 0 {
        emit(Event::Key(symbol, down));
    }
}

pub(super) fn pointer(position: (f32, f32), button: Option<(u32, bool)>) {
    if status() == Status::Active {
        emit(Event::Pointer(position.0 as i32, position.1 as i32, button));
    }
}

pub(super) fn lose_capture() {
    if matches!(status(), Status::Pending | Status::Active) {
        set_status(Status::Lost);
        emit(Event::Release);
    }
}

#[derive(Default)]
pub(super) struct Backend {
    inhibit_manager: *mut zwp_keyboard_shortcuts_inhibit_manager_v1,
    focus_manager: *mut hyprland_focus_grab_manager_v1,
    inhibitor: *mut zwp_keyboard_shortcuts_inhibitor_v1,
    confinement: *mut zwp_confined_pointer_v1,
    focus_grab: *mut hyprland_focus_grab_v1,
    confined: bool,
    inhibited: bool,
}

impl Backend {
    pub(super) unsafe fn bind(
        &mut self,
        client: &mut LibWaylandClient,
        registry: *mut wl_registry,
        name: u32,
        interface: &str,
    ) {
        match interface {
            "zwp_keyboard_shortcuts_inhibit_manager_v1" => {
                self.inhibit_manager =
                    client.wl_registry_bind(registry, name, &inhibit_manager, 1) as _;
            }
            "hyprland_focus_grab_manager_v1" => {
                self.focus_manager =
                    client.wl_registry_bind(registry, name, &focus_manager, 1) as _;
            }
            _ => {}
        }
    }

    pub(super) fn init(&self, constraints_available: bool) {
        set_status(
            if !self.inhibit_manager.is_null() && constraints_available {
                Status::Idle
            } else {
                Status::Unsupported
            },
        );
    }

    pub(super) unsafe fn apply(data: *mut WaylandPayload, enabled: bool) {
        let display = &mut *data;
        let capture = &mut display.input_capture;
        let client = &display.client;
        if !enabled {
            capture.stop(client);
            return;
        }
        if status() != Status::Pending {
            return;
        }
        if display.keyboard_context.enter_serial.is_none()
            || display.pointer_context.enter_serial.is_none()
            || display.pointer_context.pointer.is_null()
        {
            lose_capture();
            return;
        }
        capture.confinement = wl_request_constructor!(
            client,
            display.pointer_context.pointer_constraints,
            2,
            &zwp_confined_pointer_v1_interface,
            display.surface,
            display.pointer_context.pointer,
            ptr::null::<c_void>(),
            zwp_pointer_constraints_v1_lifetime_ONESHOT
        );
        capture.inhibitor = wl_request_constructor!(
            client,
            capture.inhibit_manager,
            1,
            &inhibitor,
            display.surface,
            display.seat
        );
        listen(client, capture.confinement.cast(), &POINTER_LISTENER, data);
        listen(client, capture.inhibitor.cast(), &INHIBITOR_LISTENER, data);
        if !capture.focus_manager.is_null() {
            capture.focus_grab =
                wl_request_constructor!(client, capture.focus_manager, 0, &focus_grab);
            listen(client, capture.focus_grab.cast(), &FOCUS_LISTENER, data);
            wl_request!(client, capture.focus_grab, 0, display.surface);
            wl_request!(client, capture.focus_grab, 2);
        }
        // Apply the new confinement without waiting for the next EGL swap.
        wl_request!(client, display.surface, WL_SURFACE_COMMIT);
    }

    unsafe fn stop(&mut self, client: &LibWaylandClient) {
        for (proxy, opcode) in [
            (
                self.focus_grab.cast::<wl_proxy>(),
                hyprland_focus_grab_v1::destroy,
            ),
            (
                self.inhibitor.cast(),
                zwp_keyboard_shortcuts_inhibitor_v1::destroy,
            ),
            (self.confinement.cast(), 0),
        ] {
            if !proxy.is_null() {
                wl_request!(client, proxy, opcode);
                (client.wl_proxy_destroy)(proxy);
            }
        }
        self.focus_grab = ptr::null_mut();
        self.inhibitor = ptr::null_mut();
        self.confinement = ptr::null_mut();
        self.confined = false;
        self.inhibited = false;
    }

    fn maybe_activate(&self) {
        if self.confined && self.inhibited && status() == Status::Pending {
            set_status(Status::Active);
        }
    }
}

type Callback = unsafe extern "C" fn(*mut WaylandPayload, *mut wl_proxy);
static POINTER_LISTENER: [Callback; 2] = [confined, revoked];
static INHIBITOR_LISTENER: [Callback; 2] = [inhibited, revoked];
static FOCUS_LISTENER: [Callback; 1] = [revoked];

unsafe fn listen(
    client: &LibWaylandClient,
    proxy: *mut wl_proxy,
    callbacks: &'static [Callback],
    data: *mut WaylandPayload,
) {
    (client.wl_proxy_add_listener)(proxy, callbacks.as_ptr() as _, data.cast());
}

unsafe extern "C" fn confined(data: *mut WaylandPayload, _: *mut wl_proxy) {
    (*data).input_capture.confined = true;
    (*data).input_capture.maybe_activate();
}

unsafe extern "C" fn inhibited(data: *mut WaylandPayload, _: *mut wl_proxy) {
    (*data).input_capture.inhibited = true;
    (*data).input_capture.maybe_activate();
}

unsafe extern "C" fn revoked(_: *mut WaylandPayload, _: *mut wl_proxy) {
    lose_capture();
}
