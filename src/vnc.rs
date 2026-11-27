use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
    time::Duration,
};

const EXTENDED_DESKTOP: i32 = -308;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Screen {
    fn fits(self, width: u16, height: u16) -> bool {
        self.width != 0
            && self.height != 0
            && u32::from(self.x) + u32::from(self.width) <= u32::from(width)
            && u32::from(self.y) + u32::from(self.height) <= u32::from(height)
    }
}

pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
    pub screens: Vec<Screen>,
}

type Updates = Arc<Mutex<Option<Result<Frame, String>>>>;
type Writer = Arc<Mutex<Option<Box<dyn Write + Send>>>>;

pub struct Connection {
    pub updates: Updates,
    writer: Writer,
}

impl Connection {
    fn send(&self, packet: &[u8]) {
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            let _ = writer.write_all(packet);
        }
    }

    pub fn key(&self, symbol: u32, down: bool) {
        let mut packet = [4, u8::from(down), 0, 0, 0, 0, 0, 0];
        packet[4..].copy_from_slice(&symbol.to_be_bytes());
        self.send(&packet);
    }

    pub fn pointer(&self, x: u16, y: u16, buttons: u8) {
        let mut packet = [5, buttons, 0, 0, 0, 0];
        packet[2..4].copy_from_slice(&x.to_be_bytes());
        packet[4..].copy_from_slice(&y.to_be_bytes());
        self.send(&packet);
    }
}

pub fn connect(address: String, triple: bool) -> Connection {
    let updates = Updates::default();
    let writer = Writer::default();
    let output = Arc::clone(&updates);
    let input = Arc::clone(&writer);
    std::thread::spawn(move || {
        let result = open(&address, triple, &output, &input);
        *input.lock().unwrap() = None;
        if let Err(error) = result {
            eprintln!("VNC: {error}");
            *output.lock().unwrap() = Some(Err(error.to_string()));
        }
    });
    Connection { updates, writer }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read<const N: usize>(wire: &mut dyn Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    wire.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn word(bytes: &[u8], position: usize) -> u16 {
    u16::from_be_bytes([bytes[position], bytes[position + 1]])
}

fn screen(bytes: &[u8]) -> Screen {
    Screen {
        x: word(bytes, 0),
        y: word(bytes, 2),
        width: word(bytes, 4),
        height: word(bytes, 6),
    }
}

fn discard(wire: &mut dyn Read, count: u32) -> io::Result<()> {
    if count > 1024 * 1024 {
        return Err(invalid("oversized VNC metadata"));
    }
    let count = u64::from(count);
    if io::copy(&mut wire.take(count), &mut io::sink())? != count {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(())
}

fn pixels(width: u16, height: u16) -> io::Result<Vec<u8>> {
    let area = usize::from(width) * usize::from(height);
    if width > 8192 || height > 8192 || area == 0 || area > 8192 * 2160 {
        return Err(invalid("unsupported VNC framebuffer size"));
    }
    Ok(vec![0; area * 4])
}

const WIDTH: u16 = 1920;
const HEIGHT: u16 = 1080;

fn request_triple(wire: &mut dyn Write, existing: &[([u8; 4], Screen, [u8; 4])]) -> io::Result<()> {
    let mut packet = vec![251, 0];
    packet.extend((WIDTH * 3).to_be_bytes());
    packet.extend(HEIGHT.to_be_bytes());
    packet.extend([3, 0]);
    for index in 0..3_u32 {
        let (id, flags) = existing.get(index as usize).map_or_else(
            || {
                let new_id = (0..)
                    .filter(|id: &u32| !existing.iter().any(|(old, _, _)| *old == id.to_be_bytes()))
                    .nth(index as usize)
                    .unwrap();
                (new_id.to_be_bytes(), [0; 4])
            },
            |(id, _, flags)| (*id, *flags),
        );
        packet.extend(id);
        packet.extend(((index as u16) * WIDTH).to_be_bytes());
        packet.extend(0_u16.to_be_bytes());
        packet.extend(WIDTH.to_be_bytes());
        packet.extend(HEIGHT.to_be_bytes());
        packet.extend(flags);
    }
    wire.write_all(&packet)
}

fn open(address: &str, triple: bool, output: &Updates, input: &Writer) -> io::Result<()> {
    let timeout = Some(Duration::from_secs(5));
    if let Some(path) = address.strip_prefix("unix:") {
        let wire = UnixStream::connect(path)?;
        wire.set_read_timeout(timeout)?;
        wire.set_write_timeout(timeout)?;
        return receive(&wire, wire.try_clone()?, triple, output, input, || {
            wire.set_read_timeout(None)
        });
    }
    // Unauthenticated TCP is limited to a local listener or pre-established tunnel.
    let address: SocketAddr = address
        .parse()
        .map_err(|_| invalid("use unix:PATH or a loopback IP and port"))?;
    if !address.ip().is_loopback() {
        return Err(invalid("use a loopback VNC listener or tunnel"));
    }
    let wire = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    wire.set_read_timeout(timeout)?;
    wire.set_write_timeout(timeout)?;
    receive(&wire, wire.try_clone()?, triple, output, input, || {
        wire.set_read_timeout(None)
    })
}

fn receive(
    mut wire: impl Read + Write,
    writer: impl Write + Send + 'static,
    mut triple: bool,
    output: &Updates,
    input: &Writer,
    allow_idle: impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    if &read::<12>(&mut wire)? != b"RFB 003.008\n" {
        return Err(invalid("server must support RFB 3.8"));
    }
    wire.write_all(b"RFB 003.008\n")?;
    let [count] = read::<1>(&mut wire)?;
    let mut methods = vec![0; count as usize];
    wire.read_exact(&mut methods)?;
    if !methods.contains(&1) {
        return Err(invalid("local VNC listener must offer security type None"));
    }
    wire.write_all(&[1])?;
    if read::<4>(&mut wire)? != [0; 4] {
        return Err(invalid("VNC server refused the connection"));
    }
    wire.write_all(&[1])?; // Shared session.
    let init = read::<24>(&mut wire)?;
    let (mut width, mut height) = (word(&init, 0), word(&init, 2));
    let mut buffer = pixels(width, height)?;
    discard(
        &mut wire,
        u32::from_be_bytes(init[20..24].try_into().unwrap()),
    )?;
    wire.write_all(&[
        0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0,
    ])?;
    let mut encodings = vec![2, 0, 0, 2, 0, 0, 0, 0]; // Raw, ExtendedDesktopSize.
    encodings.extend(EXTENDED_DESKTOP.to_be_bytes());
    wire.write_all(&encodings)?;

    let mut screens = vec![Screen {
        x: 0,
        y: 0,
        width,
        height,
    }];
    let mut incremental = false;
    let mut writer = Some(writer);
    loop {
        let mut request = vec![3, u8::from(incremental), 0, 0, 0, 0];
        request.extend(width.to_be_bytes());
        request.extend(height.to_be_bytes());
        with_writer(input, &mut wire, |wire| wire.write_all(&request))?;
        let rectangle_count = loop {
            match read::<1>(&mut wire)?[0] {
                0 => break word(&read::<3>(&mut wire)?, 1),
                2 => {} // Bell.
                3 => {
                    let header = read::<7>(&mut wire)?;
                    discard(
                        &mut wire,
                        u32::from_be_bytes(header[3..].try_into().unwrap()),
                    )?;
                }
                _ => return Err(invalid("unexpected VNC server message")),
            }
        };
        incremental = true;
        let mut changed = false;
        let mut awaiting_resize = false;
        for _ in 0..rectangle_count {
            let header = read::<12>(&mut wire)?;
            let rect = screen(&header);
            match i32::from_be_bytes(header[8..].try_into().unwrap()) {
                0 if rect.fits(width, height) => {
                    let pitch = usize::from(width) * 4;
                    let start = usize::from(rect.x) * 4;
                    let row_len = usize::from(rect.width) * 4;
                    for y in usize::from(rect.y)..usize::from(rect.y) + usize::from(rect.height) {
                        let row = &mut buffer[y * pitch + start..][..row_len];
                        wire.read_exact(row)?;
                        for pixel in row.chunks_exact_mut(4) {
                            pixel[3] = 255;
                        }
                    }
                    changed = true;
                }
                EXTENDED_DESKTOP => {
                    let [count, ..] = read::<4>(&mut wire)?;
                    let mut layout = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        let entry = read::<16>(&mut wire)?;
                        layout.push((
                            entry[..4].try_into().unwrap(),
                            screen(&entry[4..]),
                            entry[12..].try_into().unwrap(),
                        ));
                    }
                    if rect.x == 1 && rect.y != 0 {
                        return Err(invalid("server refused the requested three-monitor layout"));
                    }
                    if layout.is_empty()
                        || layout
                            .iter()
                            .any(|(_, head, _)| !head.fits(rect.width, rect.height))
                    {
                        return Err(invalid("invalid VNC monitor layout"));
                    }
                    let matches_triple = layout.len() == 3
                        && (0..3).all(|index| {
                            layout.iter().any(|(_, head, _)| {
                                head.x == index * WIDTH
                                    && head.y == 0
                                    && head.width == WIDTH
                                    && head.height == HEIGHT
                            })
                        });
                    if triple && !matches_triple {
                        with_writer(input, &mut wire, |wire| request_triple(wire, &layout))?;
                        awaiting_resize = true;
                    }
                    triple = false;
                    let mut next: Vec<_> = layout.into_iter().map(|(_, head, _)| head).collect();
                    next.sort_by_key(|head| (head.x, head.y));
                    if (width, height) != (rect.width, rect.height) {
                        (width, height) = (rect.width, rect.height);
                        buffer = pixels(width, height)?;
                        incremental = false;
                    } else {
                        changed |= screens != next;
                    }
                    screens = next;
                }
                _ => return Err(invalid("invalid or unsupported VNC rectangle")),
            }
        }
        if changed && incremental && !awaiting_resize {
            if let Some(writer) = writer.take() {
                allow_idle()?; // An unchanged desktop can be idle indefinitely.
                *input.lock().unwrap() = Some(Box::new(writer));
            }
            *output.lock().unwrap() = Some(Ok(Frame {
                width,
                height,
                pixels: buffer.clone(),
                screens: screens.clone(),
            }));
        }
    }
}

fn with_writer(
    input: &Writer,
    initial_wire: &mut dyn Write,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    let mut writer = input.lock().unwrap();
    match writer.as_mut() {
        Some(writer) => write(writer.as_mut()),
        None => write(initial_wire),
    }
}
