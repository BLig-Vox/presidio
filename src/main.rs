mod vnc;

use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    miniquad::{EventHandler, KeyMods},
    prelude::*,
};

const SCREEN_HEIGHT: f32 = 0.355;
const REST_PITCH: f32 = -0.073;
const HORIZONTAL_FOV: f32 = 53.13;
const EYE: Vec3 = vec3(0., 1.17, 0.60);
const PANEL_HEIGHT: f32 = SCREEN_HEIGHT - 0.019;
const PANEL_DEPTH: f32 = 0.0143;

struct Monitor {
    center: Vec3,
    width: f32,
    angle: f32,
    panorama_uv: [f32; 2],
}

impl Monitor {
    fn panel_width(&self) -> f32 {
        self.width - 0.014
    }

    fn distance_along(&self, eye: Vec3, direction: Vec3) -> Option<f32> {
        let inverse = Quat::from_rotation_y(-self.angle.to_radians());
        let origin = inverse * (eye - self.center);
        let direction = inverse * direction;
        if direction.z >= -f32::EPSILON {
            return None;
        }
        let distance = (PANEL_DEPTH - origin.z) / direction.z;
        let hit = origin + direction * distance;
        (distance > 0.
            && hit.x.abs() <= self.panel_width() / 2.
            && hit.y.abs() <= PANEL_HEIGHT / 2.)
            .then_some(distance)
    }
}

static MONITORS: [Monitor; 3] = [
    Monitor {
        center: vec3(-0.702, 1.118, -0.073),
        width: 0.662,
        angle: 23.,
        panorama_uv: [0., 0.3154],
    },
    Monitor {
        center: vec3(0., 1.118, -0.202),
        width: 0.775,
        angle: 0.,
        panorama_uv: [0.3154, 0.6846],
    },
    Monitor {
        center: vec3(0.702, 1.118, -0.073),
        width: 0.662,
        angle: -23.,
        panorama_uv: [0.6846, 1.],
    },
];

fn monitor_at(direction: Vec3) -> Option<&'static Monitor> {
    MONITORS
        .iter()
        .filter_map(|monitor| {
            monitor
                .distance_along(EYE, direction)
                .map(|distance| (distance, monitor))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, monitor)| monitor)
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

fn push_quad(mesh: &mut Mesh, corners: [Vec3; 4], uv: [Vec2; 4], colors: [Color; 4]) {
    let start = mesh.vertices.len() as u16;
    for ((position, uv), color) in corners.into_iter().zip(uv).zip(colors) {
        mesh.vertices.push(Vertex::new2(position, uv, color));
    }
    mesh.indices
        .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn mesh(texture: Option<Texture2D>) -> Mesh {
    Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        texture,
    }
}

fn uv_slice(start: f32, end: f32) -> [Vec2; 4] {
    [
        vec2(start, 1.),
        vec2(end, 1.),
        vec2(end, 0.),
        vec2(start, 0.),
    ]
}

fn add_box(mesh: &mut Mesh, center: Vec3, size: Vec3, angle: f32, color: Color) {
    let half = size * 0.5;
    let rotation = Quat::from_rotation_y(angle);
    let p = [
        vec3(-half.x, -half.y, -half.z),
        vec3(half.x, -half.y, -half.z),
        vec3(half.x, half.y, -half.z),
        vec3(-half.x, half.y, -half.z),
        vec3(-half.x, -half.y, half.z),
        vec3(half.x, -half.y, half.z),
        vec3(half.x, half.y, half.z),
        vec3(-half.x, half.y, half.z),
    ];
    let faces = [
        ([0, 1, 2, 3], 0.66),
        ([4, 5, 6, 7], 0.88),
        ([0, 4, 7, 3], 0.76),
        ([5, 1, 2, 6], 0.71),
        ([7, 6, 2, 3], 1.12),
        ([0, 1, 5, 4], 0.53),
    ];
    for (face, light) in faces {
        let shaded = Color::new(
            (color.r * light).min(1.),
            (color.g * light).min(1.),
            (color.b * light).min(1.),
            1.,
        );
        push_quad(
            mesh,
            face.map(|i| center + rotation * p[i]),
            uv_slice(0., 1.),
            [shaded; 4],
        );
    }
}

fn add_screen(screens: &mut Mesh, monitor: &Monitor, uv: [Vec2; 4]) {
    let center = monitor.center;
    let rotation = Quat::from_rotation_y(monitor.angle.to_radians());
    let x = monitor.panel_width() * 0.5;
    let y = PANEL_HEIGHT * 0.5;
    let panel = [
        vec3(-x, -y, PANEL_DEPTH),
        vec3(x, -y, PANEL_DEPTH),
        vec3(x, y, PANEL_DEPTH),
        vec3(-x, y, PANEL_DEPTH),
    ];
    push_quad(
        screens,
        panel.map(|p| center + rotation * p),
        uv,
        [WHITE; 4],
    );
}

fn room() -> Mesh {
    let mut room = mesh(None);
    add_box(
        &mut room,
        vec3(0., -0.055, 0.),
        vec3(8., 0.1, 6.),
        0.,
        rgb(44, 42, 41),
    );
    // A soft falloff on the backdrop, baked into vertices instead of lighting each frame.
    let wall_color = |x: f32, y: f32| {
        let distance = ((x + 0.20) * 0.38).powi(2) + ((y - 1.65) * 0.64).powi(2);
        let glow = (-distance).exp();
        rgb(
            (70. + 74. * glow) as u8,
            (66. + 71. * glow) as u8,
            (63. + 68. * glow) as u8,
        )
    };
    for col in -32..32 {
        for row in 0..10 {
            let x = col as f32 * 0.4;
            let y = row as f32 * 0.4;
            let corners = [
                vec3(x, y, -2.95),
                vec3(x + 0.4, y, -2.95),
                vec3(x + 0.4, y + 0.4, -2.95),
                vec3(x, y + 0.4, -2.95),
            ];
            push_quad(
                &mut room,
                corners,
                uv_slice(0., 1.),
                corners.map(|p| wall_color(p.x, p.y)),
            );
        }
    }
    room
}

fn workspace() -> (Vec<Mesh>, Texture2D) {
    let furniture = Texture2D::from_file_with_format(
        include_bytes!("../assets/workspace-lighting.png"),
        Some(ImageFormat::Png),
    );
    // Teton Range at sunrise, National Park Service (public domain).
    // https://commons.wikimedia.org/wiki/File:Teton_Range_at_sunrise_(31836612560).jpg
    let panorama = Texture2D::from_file_with_format(
        include_bytes!("../assets/teton-panorama.png"),
        Some(ImageFormat::Png),
    );

    let mut scene = vec![room()];
    // The Blender export stores position and UV per triangle corner.
    const VERTEX_BYTES: usize = 5 * size_of::<f32>();
    const TRIANGLE_BYTES: usize = 3 * VERTEX_BYTES;
    let baked = include_bytes!("../assets/workspace-mesh.bin");
    assert!(!baked.is_empty() && baked.len() % TRIANGLE_BYTES == 0);
    // Keep each batch below Macroquad's default index buffer size.
    for batch in baked.chunks(1500 * TRIANGLE_BYTES) {
        let mut part = mesh(Some(furniture.clone()));
        for vertex in batch.chunks_exact(VERTEX_BYTES) {
            let component = |index: usize| {
                let offset = index * size_of::<f32>();
                f32::from_le_bytes(vertex[offset..offset + 4].try_into().unwrap())
            };
            part.indices.push(part.vertices.len() as u16);
            part.vertices.push(Vertex::new2(
                vec3(component(0), component(1), component(2)),
                vec2(component(3), component(4)),
                WHITE,
            ));
        }
        scene.push(part);
    }

    (scene, panorama)
}

fn panel_source(
    monitor: &Monitor,
    panorama: &Texture2D,
    live: Option<&LiveDesktop>,
) -> Option<Rect> {
    let Some(live) = live else {
        let [start, end] = monitor.panorama_uv;
        return Some(Rect::new(
            start * panorama.width(),
            0.,
            (end - start) * panorama.width(),
            panorama.height(),
        ));
    };
    let index = MONITORS
        .iter()
        .position(|candidate| std::ptr::eq(candidate, monitor))
        .unwrap();
    let index = if live.screens.len() == 1 {
        index.checked_sub(1)?
    } else {
        index
    };
    live.screens.get(index).map(|head| {
        Rect::new(
            head.x.into(),
            head.y.into(),
            head.width.into(),
            head.height.into(),
        )
    })
}

struct LiveDesktop {
    texture: Texture2D,
    screens: Vec<vnc::Screen>,
}

fn screen_mesh(panorama: &Texture2D, live: Option<&LiveDesktop>) -> Mesh {
    let texture = live.map_or(panorama, |live| &live.texture);
    let mut screens = mesh(Some(texture.clone()));
    for monitor in &MONITORS {
        if let Some(source) = panel_source(monitor, panorama, live) {
            let (left, right) = (
                source.x / texture.width(),
                (source.x + source.w) / texture.width(),
            );
            let (top, bottom) = (
                source.y / texture.height(),
                (source.y + source.h) / texture.height(),
            );
            add_screen(
                &mut screens,
                monitor,
                [
                    vec2(left, bottom),
                    vec2(right, bottom),
                    vec2(right, top),
                    vec2(left, top),
                ],
            );
        }
    }
    screens
}

fn focus_bounds(aspect: f32) -> Rect {
    let width = screen_width().min(screen_height() * aspect);
    let height = width / aspect;
    Rect::new(
        (screen_width() - width) / 2.,
        (screen_height() - height) / 2.,
        width,
        height,
    )
}

fn draw_focused(texture: &Texture2D, source: Rect, aspect: f32) {
    let bounds = focus_bounds(aspect);
    draw_texture_ex(
        texture,
        bounds.x,
        bounds.y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(bounds.w, bounds.h)),
            source: Some(source),
            ..Default::default()
        },
    );
}

#[cfg(target_os = "linux")]
mod desktop;

fn config() -> Conf {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--desktop")) {
        #[cfg(target_os = "linux")]
        let result = desktop::install();
        #[cfg(not(target_os = "linux"))]
        let result: std::io::Result<()> = Err(std::io::ErrorKind::Unsupported.into());

        match result {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("Could not install the Presidio desktop entry: {error}");
                std::process::exit(1);
            }
        }
    }
    Conf {
        platform: macroquad::miniquad::conf::Platform {
            linux_wm_class: "presidio",
            ..Default::default()
        },
        window_title: "Presidio".into(),
        icon: Some(macroquad::miniquad::conf::Icon {
            small: *include_bytes!("../assets/icon-16.rgba"),
            medium: *include_bytes!("../assets/icon-32.rgba"),
            big: *include_bytes!("../assets/icon-64.rgba"),
        }),
        window_width: 1280,
        window_height: 800,
        sample_count: 4,
        ..Default::default()
    }
}

#[derive(Default)]
struct FocusShortcut {
    triggered: bool,
    keys: Vec<(KeyCode, bool)>,
    clicks: Vec<(MouseButton, bool, f32, f32)>,
    scroll: i32,
}

impl EventHandler for FocusShortcut {
    fn update(&mut self) {}

    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, modifiers: KeyMods, repeat: bool) {
        if modifiers.logo && !repeat && matches!(key, KeyCode::Enter | KeyCode::KpEnter) {
            self.triggered = true;
        } else if !modifiers.logo {
            self.keys.push((key, true));
        }
    }

    fn key_up_event(&mut self, key: KeyCode, _: KeyMods) {
        self.keys.push((key, false));
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.clicks.push((button, true, x, y));
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.clicks.push((button, false, x, y));
    }

    fn mouse_wheel_event(&mut self, _: f32, y: f32) {
        self.scroll += y.signum() as i32;
    }
}

fn keysym(key: KeyCode) -> Option<u32> {
    use KeyCode::*;
    Some(match key {
        A | B | C | D | E | F | G | H | I | J | K | L | M | N | O | P | Q | R | S | T | U | V
        | W | X | Y | Z => key as u32 + 32,
        Escape => 0xff1b,
        Enter | KpEnter => 0xff0d,
        Backspace => 0xff08,
        Tab => 0xff09,
        Left => 0xff51,
        Up => 0xff52,
        Right => 0xff53,
        Down => 0xff54,
        Home => 0xff50,
        PageUp => 0xff55,
        PageDown => 0xff56,
        End => 0xff57,
        Insert => 0xff63,
        Delete => 0xffff,
        LeftShift => 0xffe1,
        RightShift => 0xffe2,
        LeftControl => 0xffe3,
        RightControl => 0xffe4,
        CapsLock => 0xffe5,
        LeftAlt => 0xffe9,
        RightAlt => 0xffea,
        F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12 => {
            0xffbe + key as u32 - F1 as u32
        }
        _ if (32..=126).contains(&(key as u32)) => key as u32,
        _ => return None,
    })
}

#[derive(Default)]
struct RemoteInput {
    held_keys: std::collections::HashSet<u32>,
    pointer: Option<(u16, u16, u8)>,
}

impl RemoteInput {
    fn update(
        &mut self,
        connection: &vnc::Connection,
        source: Option<Rect>,
        events: &mut FocusShortcut,
    ) {
        let Some(source) = source else {
            for key in self.held_keys.drain() {
                connection.key(key, false);
            }
            if let Some((x, y, _)) = self.pointer.take() {
                connection.pointer(x, y, 0);
            }
            events.keys.clear();
            events.clicks.clear();
            events.scroll = 0;
            return;
        };
        for (key, down) in events.keys.drain(..) {
            if let Some(symbol) = keysym(key) {
                if down {
                    self.held_keys.insert(symbol);
                    connection.key(symbol, true);
                } else if self.held_keys.remove(&symbol) {
                    connection.key(symbol, false);
                }
            }
        }
        let bounds = focus_bounds(source.w / source.h);
        let position = |x: f32, y: f32| {
            let x = ((x - bounds.x) * source.w / bounds.w).clamp(0., source.w - 1.);
            let y = ((y - bounds.y) * source.h / bounds.h).clamp(0., source.h - 1.);
            ((source.x + x) as u16, (source.y + y) as u16)
        };
        let mut buttons = self.pointer.map_or(0, |(_, _, buttons)| buttons);
        for (button, down, x, y) in events.clicks.drain(..) {
            let bit = match button {
                MouseButton::Left => 1,
                MouseButton::Middle => 2,
                MouseButton::Right => 4,
                _ => continue,
            };
            if down {
                if !bounds.contains(vec2(x, y)) {
                    continue;
                }
                buttons |= bit;
            } else {
                buttons &= !bit;
            }
            let (x, y) = position(x, y);
            connection.pointer(x, y, buttons);
            self.pointer = Some((x, y, buttons));
        }
        let (x, y) = mouse_position();
        if bounds.contains(vec2(x, y)) || buttons != 0 {
            let (x, y) = position(x, y);
            if self.pointer != Some((x, y, buttons)) {
                connection.pointer(x, y, buttons);
                self.pointer = Some((x, y, buttons));
            }
            let scroll = std::mem::take(&mut events.scroll);
            for _ in 0..scroll.unsigned_abs().min(12) {
                connection.pointer(x, y, buttons | if scroll > 0 { 8 } else { 16 });
                connection.pointer(x, y, buttons);
            }
        } else {
            events.scroll = 0;
        }
    }
}

#[macroquad::main(config)]
async fn main() {
    let (scene, panorama) = workspace();
    let mut args = std::env::args().skip(1);
    let updates = match args.next().as_deref() {
        None => None,
        Some("--vnc") => {
            let Some(address) = args.next() else {
                eprintln!("Usage: presidio [--desktop] or presidio [--vnc unix:PATH|127.0.0.1:PORT [--request-triple-layout]]");
                return;
            };
            let triple = match args.next().as_deref() {
                None => false,
                Some("--request-triple-layout") if args.next().is_none() => true,
                _ => {
                    eprintln!("Unknown VNC option");
                    return;
                }
            };
            Some(vnc::connect(address, triple))
        }
        _ => {
            eprintln!(
                "Usage: presidio [--desktop] or presidio [--vnc unix:PATH|127.0.0.1:PORT [--request-triple-layout]]"
            );
            return;
        }
    };
    let mut live: Option<LiveDesktop> = None;
    let mut screens = screen_mesh(&panorama, None);
    let mut vnc_status = updates.as_ref().map(|_| String::from("Connecting to VNC…"));
    let mut focused = None;
    let keyboard = register_input_subscriber();
    let mut focus_shortcut = FocusShortcut::default();
    let mut input = RemoteInput::default();
    let (mut yaw, mut pitch) = (0.0_f32, REST_PITCH);
    let mut previous: Option<(f32, f32)> = None;

    loop {
        let update = updates
            .as_ref()
            .and_then(|connection| connection.updates.try_lock().ok()?.take());
        match update {
            Some(Ok(frame)) => {
                if let Some(current) = live.as_ref().filter(|current| {
                    current.screens == frame.screens
                        && current.texture.width() == f32::from(frame.width)
                        && current.texture.height() == f32::from(frame.height)
                }) {
                    current.texture.update_from_bytes(
                        frame.width.into(),
                        frame.height.into(),
                        &frame.pixels,
                    );
                } else {
                    eprintln!(
                        "VNC: {} reported displays: {:?}",
                        frame.screens.len(),
                        frame.screens
                    );
                    vnc_status = Some(format!("VNC: {} live displays", frame.screens.len()));
                    live = Some(LiveDesktop {
                        texture: Texture2D::from_rgba8(frame.width, frame.height, &frame.pixels),
                        screens: frame.screens,
                    });
                    screens = screen_mesh(&panorama, live.as_ref());
                    focused = None;
                }
            }
            Some(Err(error)) => {
                vnc_status = Some(format!("VNC disconnected: {error}"));
                live = None;
                focused = None;
                screens = screen_mesh(&panorama, None);
            }
            None => {}
        }
        let mouse = mouse_position();
        let super_held = is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
        let look = focused.is_none() && super_held && is_mouse_button_down(MouseButton::Right);
        if look {
            if let Some((x, y)) = previous {
                yaw = (yaw - (mouse.0 - x) * 0.003).clamp(-1.1, 1.1);
                pitch = (pitch - (mouse.1 - y) * 0.003).clamp(-0.65, 0.65);
            }
            previous = Some(mouse);
        } else {
            previous = None;
        }
        if focused.is_none() && is_key_pressed(KeyCode::Home) {
            yaw = 0.;
            pitch = REST_PITCH;
        }

        let direction = vec3(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        // Use the event's modifiers so a quick tap still works between frames.
        repeat_all_miniquad_input(&mut focus_shortcut, keyboard);
        let toggled = std::mem::take(&mut focus_shortcut.triggered);
        if toggled {
            focused = if focused.is_some() {
                None
            } else {
                monitor_at(direction)
                    .filter(|monitor| panel_source(monitor, &panorama, live.as_ref()).is_some())
            };
        }

        if let Some(connection) = &updates {
            let source = if !toggled && live.is_some() {
                focused.and_then(|monitor| panel_source(monitor, &panorama, live.as_ref()))
            } else {
                None
            };
            input.update(connection, source, &mut focus_shortcut);
        } else {
            focus_shortcut.keys.clear();
            focus_shortcut.clicks.clear();
            focus_shortcut.scroll = 0;
        }

        let hint = if let Some(monitor) = focused {
            clear_background(BLACK);
            let source = panel_source(monitor, &panorama, live.as_ref()).unwrap();
            let (texture, aspect) = live
                .as_ref()
                .map_or((&panorama, monitor.panel_width() / PANEL_HEIGHT), |live| {
                    (&live.texture, source.w / source.h)
                });
            draw_focused(texture, source, aspect);
            "Super + Enter to release input and return to desk"
        } else {
            clear_background(rgb(32, 30, 29));
            let aspect = screen_width() / screen_height().max(1.);
            let fovy = 2. * ((HORIZONTAL_FOV.to_radians() / 2.).tan() / aspect.max(0.8)).atan();
            set_camera(&Camera3D {
                position: EYE,
                target: EYE + direction,
                up: Vec3::Y,
                fovy,
                z_near: 0.03,
                z_far: 20.,
                ..Default::default()
            });
            for mesh in &scene {
                draw_mesh(mesh);
            }
            draw_mesh(&screens);
            set_default_camera();
            "Super + right-drag to look   |   Super + Enter to focus   |   Home to recenter"
        };
        let hint = vnc_status.as_ref().map_or_else(
            || hint.to_owned(),
            |status| format!("{status}   |   {hint}"),
        );
        let label = measure_text(&hint, None, 16, 1.);
        let y = screen_height() - 15.;
        draw_rectangle(
            14.,
            y - 19.,
            label.width + 18.,
            26.,
            Color::new(0.05, 0.05, 0.05, 0.86),
        );
        draw_text(&hint, 23., y, 16., rgb(225, 222, 218));
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vnc_uses_x_keysyms_for_typing_and_navigation() {
        assert_eq!(keysym(KeyCode::A), Some(u32::from(b'a')));
        assert_eq!(keysym(KeyCode::Backspace), Some(0xff08));
        assert_eq!(keysym(KeyCode::LeftSuper), None);
    }

    #[test]
    fn focusing_selects_the_looked_at_panel_and_ignores_empty_space() {
        for monitor in &MONITORS {
            let direction = (monitor.center - EYE).normalize();
            assert_eq!(
                monitor_at(direction).map(|hit| hit.panorama_uv),
                Some(monitor.panorama_uv)
            );
        }
        assert!(monitor_at(Vec3::Y).is_none());
        let below_panel = MONITORS[1].center - EYE - Vec3::Y * PANEL_HEIGHT;
        assert!(monitor_at(below_panel.normalize()).is_none());
    }
}
