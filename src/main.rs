mod capture;
mod vnc;

use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    miniquad::{EventHandler, KeyMods},
    prelude::*,
};

const EYE: Vec3 = vec3(0., 1.24, 1.62);
const REST_PITCH: f32 = -0.10;
const PANEL_HEIGHT: f32 = 0.336;
const PANEL_Y: f32 = 1.118;
// Match the cylindrical curve baked into workspace-mesh.bin.
const CURVE_RADIUS: f32 = 1.05;
const CURVE_Z: f32 = 0.848;
const PANEL_RADIUS: f32 = CURVE_RADIUS - 0.0143;
const SIDE_ANGLE: f32 = 0.704406;

struct Monitor {
    width: f32,
    azimuth: f32,
    panorama_uv: [f32; 2],
}

impl Monitor {
    fn panel_width(&self) -> f32 {
        self.width - 0.014
    }

    fn panel_point(&self, u: f32, v: f32) -> Vec3 {
        let angle = self.azimuth + ((u - 0.5) * self.panel_width() / CURVE_RADIUS).asin();
        vec3(
            PANEL_RADIUS * angle.sin(),
            PANEL_Y + (0.5 - v) * PANEL_HEIGHT,
            CURVE_Z - PANEL_RADIUS * angle.cos(),
        )
    }

    fn intersect(&self, eye: Vec3, direction: Vec3) -> Option<(f32, Vec2)> {
        let origin = vec2(eye.x, eye.z - CURVE_Z);
        let ray = vec2(direction.x, direction.z);
        let a = ray.length_squared();
        let b = origin.dot(ray);
        let discriminant = b * b - a * (origin.length_squared() - PANEL_RADIUS.powi(2));
        if a <= f32::EPSILON || discriminant < 0. {
            return None;
        }
        let root = discriminant.sqrt();
        let half_arc = (self.panel_width() / (2. * CURVE_RADIUS)).asin();
        [(-b - root) / a, (-b + root) / a]
            .into_iter()
            .filter(|distance| *distance > 0.)
            .find_map(|distance| {
                let hit = origin + ray * distance;
                let angle = hit.x.atan2(-hit.y) - self.azimuth;
                let v = 0.5 - (eye.y + direction.y * distance - PANEL_Y) / PANEL_HEIGHT;
                (angle.abs() <= half_arc + 1e-5 && (-1e-5..=1.00001).contains(&v)).then(|| {
                    let u = 0.5 + CURVE_RADIUS * angle.sin() / self.panel_width();
                    (distance, vec2(u.clamp(0., 1.), v.clamp(0., 1.)))
                })
            })
    }
}

static MONITORS: [Monitor; 3] = [
    Monitor {
        width: 0.662,
        azimuth: -SIDE_ANGLE,
        panorama_uv: [0., 0.3154],
    },
    Monitor {
        width: 0.775,
        azimuth: 0.,
        panorama_uv: [0.3154, 0.6846],
    },
    Monitor {
        width: 0.662,
        azimuth: SIDE_ANGLE,
        panorama_uv: [0.6846, 1.],
    },
];

fn monitor_hit(direction: Vec3) -> Option<(&'static Monitor, Vec2)> {
    MONITORS
        .iter()
        .filter_map(|monitor| {
            monitor
                .intersect(EYE, direction)
                .map(|(distance, uv)| (distance, monitor, uv))
        })
        .min_by(|(a, ..), (b, ..)| a.total_cmp(b))
        .map(|(_, monitor, uv)| (monitor, uv))
}

fn monitor_at(direction: Vec3) -> Option<&'static Monitor> {
    monitor_hit(direction).map(|(monitor, _)| monitor)
}

struct CameraLook {
    yaw: f32,
    pitch: f32,
    half_fov_tangent: f32,
}

impl Default for CameraLook {
    fn default() -> Self {
        Self {
            yaw: 0.,
            pitch: REST_PITCH,
            half_fov_tangent: 0.75,
        }
    }
}

impl CameraLook {
    fn turn(&mut self, delta: Vec2) {
        // Stop before looking past the outer screens so they remain focusable.
        self.yaw = (self.yaw - delta.x * 0.002).clamp(-0.52, 0.52);
        // Soften vertical input during sideways drags to avoid accidental tilt.
        let vertical = (delta.y.abs() - delta.x.abs() * 0.25)
            .max(0.)
            .copysign(delta.y);
        self.pitch = (self.pitch + vertical * 0.002).clamp(REST_PITCH - 0.4, REST_PITCH + 0.4);
    }

    fn zoom(&mut self, steps: f32) {
        self.half_fov_tangent = (self.half_fov_tangent * 0.87_f32.powf(steps)).clamp(0.075, 1.0);
    }

    fn fovy(&self, aspect: f32) -> f32 {
        2. * (self.half_fov_tangent / aspect.max(0.8)).atan()
    }

    fn direction(&self) -> Vec3 {
        vec3(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    fn pointer_ray(&self, mouse: Vec2) -> Vec3 {
        let aspect = screen_width() / screen_height().max(1.);
        let tangent = (self.fovy(aspect) / 2.).tan();
        let direction = self.direction();
        let right = direction.cross(Vec3::Y).normalize();
        let up = right.cross(direction);
        direction
            + right * ((2. * mouse.x / screen_width() - 1.) * tangent * aspect)
            + up * ((1. - 2. * mouse.y / screen_height()) * tangent)
    }
}

#[derive(Default)]
struct CaptureControls {
    super_keys: [bool; 2],
    focus_held: bool,
    zoom_keys: std::collections::HashSet<u32>,
    drag: Option<Vec2>,
}

impl CaptureControls {
    fn super_held(&self) -> bool {
        self.super_keys.contains(&true)
    }
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
    const STRIPS: usize = 40;
    for strip in 0..STRIPS {
        let left = strip as f32 / STRIPS as f32;
        let right = (strip + 1) as f32 / STRIPS as f32;
        push_quad(
            screens,
            [
                monitor.panel_point(left, 1.),
                monitor.panel_point(right, 1.),
                monitor.panel_point(right, 0.),
                monitor.panel_point(left, 0.),
            ],
            [
                uv[0].lerp(uv[1], left),
                uv[0].lerp(uv[1], right),
                uv[3].lerp(uv[2], right),
                uv[3].lerp(uv[2], left),
            ],
            [WHITE; 4],
        );
    }
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

#[derive(Default)]
struct CapturePrompt {
    pressed: Option<bool>,
}

impl CapturePrompt {
    fn buttons() -> (Rect, Rect, Rect) {
        let width = (screen_width() - 24.).clamp(1., 418.);
        let panel = Rect::new(
            (screen_width() - width) / 2.,
            (screen_height() - 154.) / 2.,
            width,
            154.,
        );
        let yes = Rect::new(panel.x + panel.w - 152., panel.y + 98., 132., 36.);
        let cancel = Rect::new(yes.x - 102., yes.y, 92., yes.h);
        (panel, yes, cancel)
    }

    fn response(&mut self, clicks: &[(MouseButton, bool, f32, f32)]) -> Option<bool> {
        if is_key_pressed(KeyCode::Escape) {
            return Some(false);
        }
        if is_key_released(KeyCode::Enter) || is_key_released(KeyCode::KpEnter) {
            return Some(true);
        }
        let (_, yes, cancel) = Self::buttons();
        let scale = screen_dpi_scale();
        for &(button, down, x, y) in clicks {
            if button != MouseButton::Left {
                continue;
            }
            let position = vec2(x / scale, y / scale);
            let choice = yes
                .contains(position)
                .then_some(true)
                .or_else(|| cancel.contains(position).then_some(false));
            if down {
                self.pressed = choice;
            } else if self
                .pressed
                .take()
                .is_some_and(|pressed| Some(pressed) == choice)
            {
                return choice;
            }
        }
        None
    }

    fn draw(&self) {
        let (panel, yes, cancel) = Self::buttons();
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            Color::new(0., 0., 0., 0.58),
        );
        draw_rectangle(panel.x, panel.y, panel.w, panel.h, rgb(24, 25, 31));
        draw_text(
            "Capture keyboard and mouse?",
            panel.x + 20.,
            panel.y + 37.,
            25.,
            WHITE,
        );
        draw_text(
            "Press Super + Esc to return input to the host.",
            panel.x + 20.,
            panel.y + 66.,
            18.,
            LIGHTGRAY,
        );

        let (x, y) = mouse_position();
        for (bounds, label, normal, hover) in [
            (yes, "Yes, capture", rgb(46, 79, 180), rgb(58, 99, 204)),
            (cancel, "Cancel", rgb(54, 55, 65), rgb(70, 72, 82)),
        ] {
            let color = if bounds.contains(vec2(x, y)) {
                hover
            } else {
                normal
            };
            draw_rectangle(bounds.x, bounds.y, bounds.w, bounds.h, color);
            let width = measure_text(label, None, 18, 1.).width;
            draw_text(
                label,
                bounds.x + (bounds.w - width) / 2.,
                bounds.y + 24.,
                18.,
                WHITE,
            );
        }
    }
}

#[cfg(target_os = "linux")]
mod desktop;

fn config() -> Conf {
    #[cfg(target_os = "linux")]
    if let Err(error) = desktop::handle_command() {
        eprintln!("Presidio: {error}");
        std::process::exit(1);
    }
    Conf {
        platform: macroquad::miniquad::conf::Platform {
            linux_wm_class: "presidio",
            linux_backend: if capture::wayland_session() {
                macroquad::miniquad::conf::LinuxBackend::WaylandWithX11Fallback
            } else {
                macroquad::miniquad::conf::LinuxBackend::X11WithWaylandFallback
            },
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
        high_dpi: true,
        ..Default::default()
    }
}

#[derive(Default)]
struct FocusShortcut {
    triggered: bool,
    super_keys: [bool; 2],
    zoom: f32,
    minimized: bool,
    keys: Vec<(KeyCode, bool)>,
    clicks: Vec<(MouseButton, bool, f32, f32)>,
    scroll: i32,
}

impl EventHandler for FocusShortcut {
    fn update(&mut self) {}

    fn draw(&mut self) {}

    fn key_down_event(&mut self, key: KeyCode, modifiers: KeyMods, repeat: bool) {
        if let Some(index) = super_index(key) {
            self.super_keys[index] = true;
        } else if modifiers.logo || self.super_keys.contains(&true) {
            if !repeat && matches!(key, KeyCode::Enter | KeyCode::KpEnter) {
                self.triggered = true;
            } else if !repeat {
                self.zoom += zoom_delta(key as u32).unwrap_or(0.);
            }
        } else if !repeat {
            self.keys.push((key, true));
        }
    }

    fn key_up_event(&mut self, key: KeyCode, _: KeyMods) {
        if let Some(index) = super_index(key) {
            self.super_keys[index] = false;
        } else {
            self.keys.push((key, false));
        }
    }

    fn window_minimized_event(&mut self) {
        self.super_keys = [false; 2];
        self.minimized = true;
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.clicks.push((button, true, x, y));
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.clicks.push((button, false, x, y));
    }

    fn mouse_wheel_event(&mut self, _: f32, y: f32) {
        if self.super_keys.contains(&true) {
            self.zoom += y.signum();
        } else {
            self.scroll += y.signum() as i32;
        }
    }
}

fn super_index(key: KeyCode) -> Option<usize> {
    match key {
        KeyCode::LeftSuper => Some(0),
        KeyCode::RightSuper => Some(1),
        _ => None,
    }
}

fn zoom_delta(symbol: u32) -> Option<f32> {
    match symbol {
        0x2b | 0x3d | 0xffab => Some(1.),
        0x2d | 0xffad => Some(-1.),
        _ => None,
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
    repeating_key: Option<(u32, std::time::Instant)>,
    pointer: Option<(u16, u16, u8)>,
}

impl RemoteInput {
    fn key(&mut self, connection: &vnc::Connection, symbol: u32, down: bool) {
        if down {
            if self.held_keys.insert(symbol) {
                connection.key(symbol, true);
                if matches!(symbol, 0xff08 | 0xff51..=0xff54) {
                    self.repeating_key = Some((
                        symbol,
                        std::time::Instant::now() + std::time::Duration::from_millis(250),
                    ));
                }
            }
        } else if self.held_keys.remove(&symbol) {
            connection.key(symbol, false);
            if self.repeating_key.is_some_and(|(key, _)| key == symbol) {
                self.repeating_key = None;
            }
        }
    }

    fn repeat(&mut self, connection: &vnc::Connection) {
        let now = std::time::Instant::now();
        if let Some((symbol, due)) = &mut self.repeating_key {
            if now >= *due {
                connection.repeat_key(*symbol);
                *due = now + std::time::Duration::from_millis(45);
            }
        }
    }

    fn release(&mut self, connection: &vnc::Connection) {
        self.repeating_key = None;
        for key in self.held_keys.drain() {
            connection.key(key, false);
        }
        self.release_pointer(connection);
    }

    fn release_pointer(&mut self, connection: &vnc::Connection) {
        if let Some((x, y, buttons)) = self.pointer.take() {
            if buttons != 0 {
                connection.pointer(x, y, 0);
            }
        }
    }

    fn captured_pointer(
        &mut self,
        connection: &vnc::Connection,
        position: Option<(u16, u16)>,
        button: Option<(u32, bool)>,
    ) {
        let (x, y) = match (position, button, self.pointer) {
            (Some(position), _, _) => position,
            // A guest drag may end outside any of the rendered monitors.
            (None, Some((1..=3, false)), Some((x, y, _))) => (x, y),
            _ => return,
        };
        let mut buttons = self.pointer.map_or(0, |(_, _, buttons)| buttons);
        match button {
            Some((button @ 1..=3, down)) => {
                let bit = 1 << (button - 1);
                if down {
                    buttons |= bit;
                } else {
                    buttons &= !bit;
                }
            }
            Some((button @ 4..=7, true)) => connection.pointer(x, y, buttons | (1 << (button - 1))),
            Some(_) => return,
            None if self.pointer == Some((x, y, buttons)) => return,
            None => {}
        }
        connection.pointer(x, y, buttons);
        self.pointer = Some((x, y, buttons));
    }

    fn position(source: Rect, bounds: Rect, x: f32, y: f32) -> (u16, u16) {
        Self::position_uv(
            source,
            vec2((x - bounds.x) / bounds.w, (y - bounds.y) / bounds.h),
        )
    }

    fn position_uv(source: Rect, uv: Vec2) -> (u16, u16) {
        let x = (uv.x * source.w).clamp(0., source.w - 1.);
        let y = (uv.y * source.h).clamp(0., source.h - 1.);
        ((source.x + x) as u16, (source.y + y) as u16)
    }

    fn update(
        &mut self,
        connection: &vnc::Connection,
        source: Option<Rect>,
        events: &mut FocusShortcut,
    ) {
        let Some(source) = source else {
            self.release(connection);
            events.keys.clear();
            events.clicks.clear();
            events.scroll = 0;
            return;
        };
        for (key, down) in events.keys.drain(..) {
            if let Some(symbol) = keysym(key) {
                self.key(connection, symbol, down);
            }
        }
        let bounds = focus_bounds(source.w / source.h);
        let position = |x, y| Self::position(source, bounds, x, y);
        let mut buttons = self.pointer.map_or(0, |(_, _, buttons)| buttons);
        for (button, down, x, y) in events.clicks.drain(..) {
            let scale = screen_dpi_scale();
            let (x, y) = (x / scale, y / scale);
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
    let mut prompt: Option<CapturePrompt> = None;
    let mut capture = None;
    let mut capture_error = None;
    let keyboard = register_input_subscriber();
    let mut focus_shortcut = FocusShortcut::default();
    let mut input = RemoteInput::default();
    let mut camera = CameraLook::default();
    let mut previous = None;
    let mut controls = CaptureControls::default();

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
                    prompt = None;
                }
            }
            Some(Err(error)) => {
                vnc_status = Some(format!("VNC disconnected: {error}"));
                capture_error = None;
                live = None;
                focused = None;
                prompt = None;
                screens = screen_mesh(&panorama, None);
            }
            None => {}
        }
        let mouse = Vec2::from(mouse_position());
        let super_held = is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
        let desk_mode = capture.is_none() && focused.is_none() && prompt.is_none();
        if desk_mode && super_held && is_mouse_button_down(MouseButton::Right) {
            if let Some(last) = previous {
                camera.turn(mouse - last);
            }
            previous = Some(mouse);
        } else {
            previous = None;
        }
        if desk_mode && is_key_pressed(KeyCode::Home) {
            camera = CameraLook::default();
        }

        // Use the event's modifiers so a quick tap still works between frames.
        repeat_all_miniquad_input(&mut focus_shortcut, keyboard);
        let toggled = std::mem::take(&mut focus_shortcut.triggered);
        let zoom = std::mem::take(&mut focus_shortcut.zoom);
        let minimized = std::mem::take(&mut focus_shortcut.minimized);
        if desk_mode && zoom != 0. {
            camera.zoom(zoom);
        }
        let mut suppress = false;
        if capture.is_none() {
            if let Some(dialog) = &mut prompt {
                suppress = true;
                if let Some(accepted) = dialog.response(&focus_shortcut.clicks) {
                    if accepted {
                        match capture::Capture::start() {
                            Ok(grab) => {
                                focus_shortcut.super_keys = [false; 2];
                                capture = Some(grab);
                            }
                            Err(error) => capture_error = Some(error),
                        }
                    }
                    prompt = None;
                }
            } else if toggled {
                capture_error = None;
                focused = if focused.is_some() {
                    None
                } else {
                    monitor_at(camera.direction())
                        .filter(|monitor| panel_source(monitor, &panorama, live.as_ref()).is_some())
                };
            } else if focused.is_none()
                && live.is_some()
                && is_mouse_button_pressed(MouseButton::Left)
            {
                capture_error = None;
                let monitor = monitor_at(camera.pointer_ray(mouse))
                    .or_else(|| monitor_at(camera.direction()))
                    .unwrap_or(&MONITORS[1]);
                if panel_source(monitor, &panorama, live.as_ref()).is_some() {
                    prompt = Some(CapturePrompt::default());
                }
            }
        }

        if let Some(connection) = &updates {
            let source = focused
                .filter(|_| live.is_some())
                .and_then(|monitor| panel_source(monitor, &panorama, live.as_ref()));
            if let Some(grab) = &capture {
                let mut release = live.is_none();
                if !release {
                    for event in grab.events() {
                        match event {
                            capture::Event::Key(symbol @ (0xffeb | 0xffec), down) => {
                                controls.super_keys[(symbol - 0xffeb) as usize] = down;
                            }
                            capture::Event::Key(symbol @ (0xff0d | 0xff8d), down)
                                if controls.focus_held
                                    || (down
                                        && controls.super_held()
                                        && !input.held_keys.contains(&symbol)) =>
                            {
                                if down && !controls.focus_held && controls.drag.is_none() {
                                    input.release_pointer(connection);
                                    focused = if focused.is_some() {
                                        None
                                    } else {
                                        monitor_at(camera.direction()).filter(|monitor| {
                                            panel_source(monitor, &panorama, live.as_ref())
                                                .is_some()
                                        })
                                    };
                                }
                                controls.focus_held = down;
                            }
                            capture::Event::Key(0xff1b, true) if controls.super_held() => {
                                release = true;
                                break;
                            }
                            capture::Event::Key(symbol, down)
                                if controls.zoom_keys.contains(&symbol)
                                    || (down
                                        && controls.super_held()
                                        && zoom_delta(symbol).is_some()
                                        && !input.held_keys.contains(&symbol)) =>
                            {
                                if down {
                                    if controls.zoom_keys.insert(symbol) && focused.is_none() {
                                        camera.zoom(zoom_delta(symbol).unwrap());
                                    }
                                } else {
                                    controls.zoom_keys.remove(&symbol);
                                }
                            }
                            capture::Event::Key(symbol, down) => {
                                input.key(connection, symbol, down);
                            }
                            capture::Event::Pointer(x, y, button) => {
                                let mouse = vec2(x as f32, y as f32) / screen_dpi_scale();
                                if let Some((wheel @ (4 | 5), down)) =
                                    button.filter(|_| controls.super_held())
                                {
                                    if down && focused.is_none() {
                                        camera.zoom(if wheel == 4 { 1. } else { -1. });
                                    }
                                } else if let Some(last) = controls
                                    .drag
                                    .filter(|_| matches!(button, None | Some((3, _))))
                                {
                                    if focused.is_none() {
                                        camera.turn(mouse - last);
                                    }
                                    // Own the right release even if Super was released first.
                                    controls.drag = (button != Some((3, false))).then_some(mouse);
                                } else if button == Some((3, true)) && controls.super_held() {
                                    input.release_pointer(connection);
                                    controls.drag = Some(mouse);
                                } else {
                                    let position = if let Some(monitor) = focused {
                                        panel_source(monitor, &panorama, live.as_ref()).map(
                                            |source| {
                                                let bounds = focus_bounds(source.w / source.h);
                                                RemoteInput::position(
                                                    source, bounds, mouse.x, mouse.y,
                                                )
                                            },
                                        )
                                    } else {
                                        monitor_hit(camera.pointer_ray(mouse)).and_then(
                                            |(monitor, uv)| {
                                                let source = panel_source(
                                                    monitor,
                                                    &panorama,
                                                    live.as_ref(),
                                                )?;
                                                Some(RemoteInput::position_uv(source, uv))
                                            },
                                        )
                                    };
                                    input.captured_pointer(connection, position, button);
                                }
                            }
                            capture::Event::Failed(error) => {
                                capture_error = Some(error);
                                release = true;
                                break;
                            }
                            capture::Event::Release => {
                                release = true;
                                break;
                            }
                        }
                    }
                }
                if release {
                    capture = None;
                    controls = CaptureControls::default();
                    focused = None;
                    input.release(connection);
                }
            } else {
                input.update(
                    connection,
                    source.filter(|_| !toggled && !suppress && !minimized),
                    &mut focus_shortcut,
                );
            }
            input.repeat(connection);
        }
        focus_shortcut.keys.clear();
        focus_shortcut.clicks.clear();
        focus_shortcut.scroll = 0;

        let hint = if capture.as_ref().is_some_and(capture::Capture::pending) {
            "Waiting for Wayland to grant capture   |   Super + Esc to cancel"
        } else if capture.is_some() {
            "Captured   |   Super + right-drag to look   |   Super + wheel to zoom   |   Super + Enter to toggle monitor view   |   Super + Esc to release"
        } else if focused.is_some() {
            "Super + Enter to return to desk"
        } else if live.is_some() {
            "Click to capture input   |   Super + right-drag to look   |   Super + Enter to focus only"
        } else {
            "Super + right-drag to look   |   Super + Enter to focus   |   Home to recenter"
        };
        if let Some(monitor) = focused {
            clear_background(BLACK);
            let source = panel_source(monitor, &panorama, live.as_ref()).unwrap();
            let (texture, aspect) = live
                .as_ref()
                .map_or((&panorama, monitor.panel_width() / PANEL_HEIGHT), |live| {
                    (&live.texture, source.w / source.h)
                });
            draw_focused(texture, source, aspect);
        } else {
            clear_background(rgb(32, 30, 29));
            let aspect = screen_width() / screen_height().max(1.);
            let fovy = camera.fovy(aspect);
            set_camera(&Camera3D {
                position: EYE,
                target: EYE + camera.direction(),
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
        }
        let hint = capture_error.map(str::to_owned).unwrap_or_else(|| {
            vnc_status.as_ref().map_or_else(
                || hint.to_owned(),
                |status| format!("{status}   |   {hint}"),
            )
        });
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
        if let Some(dialog) = &prompt {
            dialog.draw();
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curved_monitor_hit_tracks_the_texture_across_each_panel() {
        for monitor in &MONITORS {
            for u in [0.01, 0.25, 0.5, 0.75, 0.99] {
                let point = monitor.panel_point(u, 0.31);
                let (hit, uv) = monitor_hit(point - EYE).unwrap();
                assert!(std::ptr::eq(hit, monitor));
                assert!((uv - vec2(u, 0.31)).length() < 0.001, "{uv}");
            }
        }
    }

    #[test]
    fn camera_pitches_on_deliberate_vertical_drags_without_sideways_drift() {
        let mut camera = CameraLook::default();
        let original = camera.direction();
        camera.turn(vec2(60., -8.));
        assert!(camera.direction().x < 0.);
        assert_eq!(camera.direction().y, original.y);

        // Drag the desk with the pointer, as with horizontal panning.
        camera.turn(vec2(0., -150.));
        assert!(camera.direction().y < original.y);
        camera.turn(vec2(-30., 300.));
        assert!(camera.direction().y > original.y);

        camera.turn(vec2(0., 5000.));
        let upper_limit = camera.direction();
        camera.turn(vec2(0., 5000.));
        assert_eq!(camera.direction(), upper_limit);
        assert!(upper_limit.z < -0.5);
    }

    #[test]
    fn camera_zooms_closer_without_changing_aim() {
        let mut camera = CameraLook::default();
        camera.turn(vec2(120., 0.));
        let aim = camera.direction();
        let fov = camera.fovy(1.6);

        camera.zoom(8.);
        assert!(camera.half_fov_tangent < 0.30);
        assert!(camera.fovy(1.6) < fov);
        camera.zoom(100.);
        assert_eq!(camera.half_fov_tangent, 0.075);
        assert_eq!(camera.direction(), aim);
    }

    #[test]
    fn camera_turn_limits_keep_the_outer_panels_focusable() {
        let mut camera = CameraLook::default();
        for (delta, index) in [(5000., 0), (-5000., 2)] {
            camera.turn(vec2(delta, 40.));
            let panel = monitor_at(camera.direction()).unwrap();
            assert!(std::ptr::eq(panel, &MONITORS[index]));
        }
    }

    #[test]
    fn vnc_uses_x_keysyms_for_typing_and_navigation() {
        assert_eq!(keysym(KeyCode::A), Some(u32::from(b'a')));
        assert_eq!(keysym(KeyCode::Backspace), Some(0xff08));
        assert_eq!(keysym(KeyCode::LeftSuper), None);
    }

    #[test]
    fn captured_desk_pointer_maps_to_the_live_monitor_offset() {
        let monitor = &MONITORS[2];
        let direction =
            (monitor.panel_point(0.5, 0.5) + vec3(0., PANEL_HEIGHT / 4., 0.) - EYE).normalize();
        let (hit, uv) = monitor_hit(direction).unwrap();
        assert!(std::ptr::eq(hit, monitor));
        let (x, y) = RemoteInput::position_uv(Rect::new(3840., 0., 1920., 1080.), uv);
        assert!((4798..=4802).contains(&x));
        assert!((260..300).contains(&y));
    }

    #[test]
    fn focusing_selects_the_looked_at_panel_and_ignores_empty_space() {
        for monitor in &MONITORS {
            let direction = (monitor.panel_point(0.5, 0.5) - EYE).normalize();
            assert_eq!(
                monitor_at(direction).map(|hit| hit.panorama_uv),
                Some(monitor.panorama_uv)
            );
        }
        assert!(monitor_at(Vec3::Y).is_none());
        let below_panel = MONITORS[1].panel_point(0.5, 0.5) - EYE - Vec3::Y * PANEL_HEIGHT;
        assert!(monitor_at(below_panel.normalize()).is_none());
    }
}
