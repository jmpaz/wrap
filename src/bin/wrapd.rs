use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrap::{transform_clipboard_for_paste, unwrap_auto, Format};

const WL_DISPLAY_ID: u32 = 1;
const WL_KEYBOARD_KEY_STATE_RELEASED: u32 = 0;
const WL_KEYBOARD_KEY_STATE_PRESSED: u32 = 1;
const WL_KEYMAP_FORMAT_XKB_V1: u32 = 1;

const KEY_LEFTCTRL: u32 = 29;
const KEY_LEFTSHIFT: u32 = 42;
const KEY_V: u32 = 47;
const MOD_SHIFT_CONTROL: u32 = 5;

const MIME_TEXT_UTF8: &str = "text/plain;charset=utf-8";
const MIME_TEXT: &str = "text/plain";
const MAX_ACTION_AGE_MS: u64 = 2_000;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut wayland = Wayland::connect()?;
    let listener = match activated_listener()? {
        Some(listener) => listener,
        None => match bind_listener() {
            Ok(listener) => listener,
            Err(err) => exit_err(err),
        },
    };

    eprintln!("wrapd ready at {}", socket_path()?.display());
    event_loop(&mut wayland, listener)
}

fn event_loop(wayland: &mut Wayland, listener: UnixListener) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set listener nonblocking: {err}"))?;

    loop {
        let mut fds = [
            PollFd {
                fd: listener.as_raw_fd(),
                events: POLLIN,
                revents: 0,
            },
            PollFd {
                fd: wayland.raw_fd(),
                events: POLLIN,
                revents: 0,
            },
        ];

        let ready = unsafe { poll(fds.as_mut_ptr(), fds.len() as _, -1) };
        if ready < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }

        if fds[0].revents & POLLIN != 0 {
            loop {
                match listener.accept() {
                    Ok((stream, _)) => handle_client(wayland, stream),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(err) => eprintln!("accept failed: {err}"),
                }
            }
        }

        if fds[1].revents & POLLIN != 0 {
            wayland.dispatch_ready()?;
        }
    }
}

fn handle_client(wayland: &mut Wayland, mut stream: UnixStream) {
    let started = Instant::now();
    let response = read_request(&mut stream).and_then(|request| {
        eprintln!("request start: {}", request_label(&request));
        handle_request(wayland, &request)
    });
    let elapsed = started.elapsed();

    let line = match response {
        Ok(message) if message.is_empty() => "OK\n".to_string(),
        Ok(message) => format!("OK {message}\n"),
        Err(err) => format!("ERR {err}\n"),
    };

    if let Err(err) = stream.write_all(line.as_bytes()) {
        eprintln!("failed to write client response: {err}");
    }
    eprintln!("request handled in {}ms", elapsed.as_millis());
}

fn read_request(stream: &mut UnixStream) -> Result<String, String> {
    let mut request = String::new();
    stream
        .read_to_string(&mut request)
        .map_err(|err| format!("failed to read request: {err}"))?;

    Ok(request)
}

fn handle_request(wayland: &mut Wayland, request: &str) -> Result<String, String> {
    if let Some((sent_at, content)) = parse_paste_text_request(request)? {
        reject_stale_action(sent_at)?;
        wayland.set_clipboard(content)?;
        wayland.paste_ctrl_shift_v()?;
        return Ok(String::new());
    }

    let request = request.trim();
    let parts = request.split_whitespace().collect::<Vec<_>>();

    match parts.as_slice() {
        ["STATUS"] => Ok(wayland.status()),
        ["EMIT_PASTE", sent_at] => {
            reject_stale_action(sent_at)?;
            wayland.paste_ctrl_shift_v()?;
            Ok(String::new())
        }
        ["UNWRAP_PASTE", sent_at] => {
            reject_stale_action(sent_at)?;
            let content = wayland.read_clipboard()?;
            let updated = unwrap_auto(&content);
            wayland.set_clipboard(updated)?;
            wayland.paste_ctrl_shift_v()?;
            Ok(String::new())
        }
        ["PASTE", format, sent_at] => {
            reject_stale_action(sent_at)?;
            let format = format
                .parse::<Format>()
                .map_err(|_| "invalid PASTE request".to_string())?;
            let content = wayland.read_clipboard()?;
            let updated = transform_clipboard_for_paste(&content, format);
            wayland.set_clipboard(updated)?;
            wayland.paste_ctrl_shift_v()?;
            Ok(String::new())
        }
        _ => Err("unknown request".to_string()),
    }
}

fn parse_paste_text_request(request: &str) -> Result<Option<(&str, String)>, String> {
    let Some(rest) = request.strip_prefix("PASTE_TEXT ") else {
        return Ok(None);
    };

    let (header, content) = rest
        .split_once('\n')
        .ok_or_else(|| "invalid PASTE_TEXT request".to_string())?;
    let mut fields = header.split_whitespace();
    let sent_at = fields
        .next()
        .ok_or_else(|| "invalid PASTE_TEXT request".to_string())?;
    let expected_len = fields
        .next()
        .ok_or_else(|| "invalid PASTE_TEXT request".to_string())?
        .parse::<usize>()
        .map_err(|_| "invalid PASTE_TEXT length".to_string())?;
    if fields.next().is_some() {
        return Err("invalid PASTE_TEXT request".to_string());
    }
    let actual_len = content.len();
    if actual_len != expected_len {
        return Err(format!(
            "invalid PASTE_TEXT length: expected {expected_len}, got {actual_len}"
        ));
    }

    Ok(Some((sent_at, content.to_string())))
}

fn request_label(request: &str) -> String {
    if request.starts_with("PASTE_TEXT ") {
        match parse_paste_text_request(request) {
            Ok(Some((_, content))) => format!("PASTE_TEXT len={}", content.len()),
            _ => "PASTE_TEXT invalid".to_string(),
        }
    } else {
        request.trim().to_string()
    }
}

fn reject_stale_action(sent_at: &str) -> Result<(), String> {
    let sent_at = sent_at
        .parse::<u64>()
        .map_err(|_| "action request has invalid timestamp".to_string())?;
    let age = now_ms_u64().saturating_sub(sent_at);
    if age > MAX_ACTION_AGE_MS {
        Err(format!("dropping stale action request, age={age}ms"))
    } else {
        Ok(())
    }
}

fn activated_listener() -> Result<Option<UnixListener>, String> {
    let listen_pid = std::env::var("LISTEN_PID").ok();
    let listen_fds = std::env::var("LISTEN_FDS").ok();

    if listen_pid.as_deref() != Some(&process::id().to_string())
        || listen_fds.as_deref() != Some("1")
    {
        return Ok(None);
    }

    let listener = unsafe { UnixListener::from_raw_fd(3) };
    Ok(Some(listener))
}

fn bind_listener() -> Result<UnixListener, String> {
    let socket = socket_path()?;
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }

    match fs::remove_file(&socket) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(format!(
                "failed to remove stale {}: {err}",
                socket.display()
            ))
        }
    }

    UnixListener::bind(&socket).map_err(|err| format!("failed to bind {}: {err}", socket.display()))
}

fn socket_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("WRAPD_SOCKET") {
        return Ok(PathBuf::from(path));
    }

    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "XDG_RUNTIME_DIR is not set and WRAPD_SOCKET was not provided".to_string())?;

    Ok(PathBuf::from(runtime_dir).join("wrap/wrapd.sock"))
}

fn exit_err(err: String) -> ! {
    eprintln!("{err}");
    process::exit(1);
}

struct Wayland {
    conn: WlConnection,
    registry: u32,
    seat: u32,
    data_manager: u32,
    data_device: u32,
    keyboard: u32,
    selection: Option<u32>,
    owned_selection: Option<u32>,
    owned_selection_text: Option<String>,
    awaiting_owned_selection: bool,
    offers: HashMap<u32, Offer>,
    sources: HashMap<u32, String>,
    next_id: u32,
}

#[derive(Default)]
struct Offer {
    mimes: Vec<String>,
}

impl Wayland {
    fn connect() -> Result<Self, String> {
        let conn = WlConnection::connect()?;
        let mut this = Self {
            conn,
            registry: 2,
            seat: 0,
            data_manager: 0,
            data_device: 0,
            keyboard: 0,
            selection: None,
            owned_selection: None,
            owned_selection_text: None,
            awaiting_owned_selection: false,
            offers: HashMap::new(),
            sources: HashMap::new(),
            next_id: 3,
        };

        this.get_registry()?;
        let globals = this.collect_globals()?;
        let seat = globals
            .iter()
            .find(|global| global.interface == "wl_seat")
            .ok_or_else(|| "compositor did not advertise wl_seat".to_string())?;
        let data_manager = globals
            .iter()
            .find(|global| global.interface == "zwlr_data_control_manager_v1")
            .ok_or_else(|| {
                "compositor did not advertise zwlr_data_control_manager_v1".to_string()
            })?;
        let keyboard_manager = globals
            .iter()
            .find(|global| global.interface == "zwp_virtual_keyboard_manager_v1")
            .ok_or_else(|| {
                "compositor did not advertise zwp_virtual_keyboard_manager_v1".to_string()
            })?;

        this.seat = this.bind_global(seat, "wl_seat", 1)?;
        this.data_manager = this.bind_global(data_manager, "zwlr_data_control_manager_v1", 2)?;
        let keyboard_manager =
            this.bind_global(keyboard_manager, "zwp_virtual_keyboard_manager_v1", 1)?;
        this.data_device = this.new_id();
        this.conn.send(
            this.data_manager,
            1,
            &[Arg::U32(this.data_device), Arg::U32(this.seat)],
            &[],
        )?;

        this.keyboard = this.new_id();
        this.conn.send(
            keyboard_manager,
            0,
            &[Arg::U32(this.seat), Arg::U32(this.keyboard)],
            &[],
        )?;
        this.upload_keymap()?;
        this.roundtrip()?;

        Ok(this)
    }

    fn status(&self) -> String {
        let selection = if self.selection.is_some() {
            "present"
        } else {
            "empty"
        };
        let owned = if self.owned_selection_text.is_some() {
            "yes"
        } else {
            "no"
        };
        format!(
            "wrapd ready; selection={selection}; owned={owned}; awaiting_owned={}; offers={}; sources={}",
            self.awaiting_owned_selection,
            self.offers.len(),
            self.sources.len()
        )
    }

    fn raw_fd(&self) -> RawFd {
        self.conn.raw_fd()
    }

    fn dispatch_ready(&mut self) -> Result<usize, String> {
        self.conn.recv_once()?;
        let mut handled = 0;
        while self.conn.has_buffered_message()? {
            let message = self.conn.next_message()?;
            self.handle_event(message)?;
            handled += 1;
        }
        Ok(handled)
    }

    fn read_clipboard(&mut self) -> Result<String, String> {
        let pumped = self.pump_for(Duration::from_millis(20))?;
        if pumped > 0 {
            eprintln!("clipboard read: pumped {pumped} pending wayland event(s)");
        }
        if let Some(text) = &self.owned_selection_text {
            eprintln!("clipboard read: using owned cache, len={}", text.len());
            return Ok(text.clone());
        }

        let offer_id = self
            .selection
            .ok_or_else(|| "clipboard is empty".to_string())?;

        let mime = self
            .offers
            .get(&offer_id)
            .and_then(|offer| choose_mime(&offer.mimes))
            .ok_or_else(|| "clipboard does not advertise text data".to_string())?;
        eprintln!("clipboard read: receiving external offer={offer_id}, mime={mime}");

        let mut fds = [0; 2];
        if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
            return Err(format!("pipe failed: {}", std::io::Error::last_os_error()));
        }

        self.conn
            .send(offer_id, 0, &[Arg::String(mime)], &[fds[1]])?;
        unsafe {
            close(fds[1]);
        }

        let mut file = unsafe { File::from_raw_fd(fds[0]) };
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|err| format!("failed to read clipboard transfer: {err}"))?;
        eprintln!(
            "clipboard read: external transfer complete, len={}",
            content.len()
        );

        Ok(content)
    }

    fn set_clipboard(&mut self, content: String) -> Result<(), String> {
        let source = self.new_id();
        eprintln!("clipboard set: source={source}, len={}", content.len());
        self.conn
            .send(self.data_manager, 0, &[Arg::U32(source)], &[])?;
        self.conn
            .send(source, 0, &[Arg::String(MIME_TEXT_UTF8)], &[])?;
        self.conn.send(source, 0, &[Arg::String(MIME_TEXT)], &[])?;
        self.sources.insert(source, content.clone());
        self.owned_selection_text = Some(content);
        self.awaiting_owned_selection = true;
        self.conn
            .send(self.data_device, 0, &[Arg::U32(source)], &[])?;
        self.roundtrip()?;
        eprintln!(
            "clipboard set: roundtrip complete, owned_selection={:?}, awaiting_owned={}",
            self.owned_selection, self.awaiting_owned_selection
        );
        Ok(())
    }

    fn paste_ctrl_shift_v(&mut self) -> Result<(), String> {
        eprintln!("paste: emitting ctrl+shift+v");
        self.key(KEY_LEFTCTRL, WL_KEYBOARD_KEY_STATE_PRESSED)?;
        self.key(KEY_LEFTSHIFT, WL_KEYBOARD_KEY_STATE_PRESSED)?;
        self.modifiers(MOD_SHIFT_CONTROL)?;
        self.key(KEY_V, WL_KEYBOARD_KEY_STATE_PRESSED)?;
        self.key(KEY_V, WL_KEYBOARD_KEY_STATE_RELEASED)?;
        self.key(KEY_LEFTSHIFT, WL_KEYBOARD_KEY_STATE_RELEASED)?;
        self.key(KEY_LEFTCTRL, WL_KEYBOARD_KEY_STATE_RELEASED)?;
        self.modifiers(0)?;
        self.conn.flush()?;
        let pumped = self.pump_available()?;
        eprintln!("paste: key emission flushed, pumped {pumped} immediate wayland event(s)");
        Ok(())
    }

    fn get_registry(&mut self) -> Result<(), String> {
        self.conn
            .send(WL_DISPLAY_ID, 1, &[Arg::U32(self.registry)], &[])
    }

    fn collect_globals(&mut self) -> Result<Vec<Global>, String> {
        let callback = self.new_id();
        self.conn
            .send(WL_DISPLAY_ID, 0, &[Arg::U32(callback)], &[])?;
        let mut globals = Vec::new();

        loop {
            let message = self.conn.next_message()?;
            if message.sender == self.registry && message.opcode == 0 {
                let mut reader = Reader::new(&message.data);
                globals.push(Global {
                    name: reader.u32()?,
                    interface: reader.string()?,
                    version: reader.u32()?,
                });
            } else if message.sender == callback && message.opcode == 0 {
                return Ok(globals);
            } else {
                self.handle_event(message)?;
            }
        }
    }

    fn bind_global(
        &mut self,
        global: &Global,
        interface: &'static str,
        max_version: u32,
    ) -> Result<u32, String> {
        let id = self.new_id();
        let version = global.version.min(max_version);

        self.conn.send(
            self.registry,
            0,
            &[
                Arg::U32(global.name),
                Arg::String(interface),
                Arg::U32(version),
                Arg::U32(id),
            ],
            &[],
        )?;

        Ok(id)
    }

    fn upload_keymap(&mut self) -> Result<(), String> {
        let keymap = keymap();
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = PathBuf::from(runtime).join(format!("wrap-keymap-{}", process::id()));
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|err| format!("failed to create keymap file: {err}"))?;
        file.write_all(keymap.as_bytes())
            .map_err(|err| format!("failed to write keymap: {err}"))?;
        let _ = fs::remove_file(&path);

        self.conn.send(
            self.keyboard,
            0,
            &[
                Arg::U32(WL_KEYMAP_FORMAT_XKB_V1),
                Arg::Fd,
                Arg::U32(keymap.len() as u32),
            ],
            &[file.as_raw_fd()],
        )
    }

    fn key(&mut self, key: u32, state: u32) -> Result<(), String> {
        self.conn.send(
            self.keyboard,
            1,
            &[Arg::U32(now_ms()), Arg::U32(key), Arg::U32(state)],
            &[],
        )
    }

    fn modifiers(&mut self, depressed: u32) -> Result<(), String> {
        self.conn.send(
            self.keyboard,
            2,
            &[Arg::U32(depressed), Arg::U32(0), Arg::U32(0), Arg::U32(0)],
            &[],
        )
    }

    fn roundtrip(&mut self) -> Result<(), String> {
        let callback = self.new_id();
        self.conn
            .send(WL_DISPLAY_ID, 0, &[Arg::U32(callback)], &[])?;

        loop {
            let message = self.conn.next_message()?;
            if message.sender == callback && message.opcode == 0 {
                return Ok(());
            }
            self.handle_event(message)?;
        }
    }

    fn pump_for(&mut self, duration: Duration) -> Result<usize, String> {
        let deadline = Instant::now() + duration;
        let mut handled = 0;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout = remaining.as_millis().min(10) as i32;
            if self.conn.poll_read(timeout)? {
                self.conn.recv_once()?;
                while self.conn.has_buffered_message()? {
                    let message = self.conn.next_message()?;
                    self.handle_event(message)?;
                    handled += 1;
                }
            }
        }
        Ok(handled)
    }

    fn pump_available(&mut self) -> Result<usize, String> {
        let mut handled = 0;
        while self.conn.poll_read(0)? {
            handled += self.dispatch_ready()?;
        }
        Ok(handled)
    }

    fn handle_event(&mut self, message: Message) -> Result<(), String> {
        if message.sender == WL_DISPLAY_ID && message.opcode == 0 {
            let mut reader = Reader::new(&message.data);
            let object_id = reader.u32()?;
            let code = reader.u32()?;
            let error = reader.string()?;
            return Err(format!(
                "wayland error on object {object_id}, code {code}: {error}"
            ));
        }

        if message.sender == self.data_device {
            match message.opcode {
                0 => {
                    let mut reader = Reader::new(&message.data);
                    let id = reader.u32()?;
                    self.offers.insert(id, Offer::default());
                }
                1 => {
                    let mut reader = Reader::new(&message.data);
                    let id = reader.u32()?;
                    self.selection = (id != 0).then_some(id);
                    if self.awaiting_owned_selection && id != 0 {
                        self.owned_selection = Some(id);
                        self.awaiting_owned_selection = false;
                        eprintln!("selection event: owned offer={id}");
                    } else if Some(id) != self.owned_selection {
                        eprintln!("selection event: external offer={id}");
                        self.owned_selection = None;
                        self.owned_selection_text = None;
                        self.awaiting_owned_selection = false;
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        if let Some(offer) = self.offers.get_mut(&message.sender) {
            if message.opcode == 0 {
                let mut reader = Reader::new(&message.data);
                offer.mimes.push(reader.string()?);
            }
            return Ok(());
        }

        if self.sources.contains_key(&message.sender) {
            match message.opcode {
                0 => {
                    let mut reader = Reader::new(&message.data);
                    let _mime = reader.string()?;
                    let fd = message
                        .fds
                        .first()
                        .copied()
                        .ok_or_else(|| "source send event did not include an fd".to_string())?;
                    let mut file = unsafe { File::from_raw_fd(fd) };
                    if let Some(content) = self.sources.get(&message.sender) {
                        file.write_all(content.as_bytes())
                            .map_err(|err| format!("failed to serve clipboard data: {err}"))?;
                    }
                }
                1 => {
                    self.sources.remove(&message.sender);
                    if self.sources.is_empty() {
                        eprintln!("source cancelled: clearing owned clipboard cache");
                        self.owned_selection = None;
                        self.owned_selection_text = None;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

fn choose_mime(mimes: &[String]) -> Option<&'static str> {
    if mimes.iter().any(|mime| mime == MIME_TEXT_UTF8) {
        Some(MIME_TEXT_UTF8)
    } else if mimes.iter().any(|mime| mime == MIME_TEXT) {
        Some(MIME_TEXT)
    } else {
        None
    }
}

struct Global {
    name: u32,
    interface: String,
    version: u32,
}

struct WlConnection {
    stream: UnixStream,
    bytes: Vec<u8>,
    fds: Vec<RawFd>,
}

impl WlConnection {
    fn connect() -> Result<Self, String> {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map_err(|_| "XDG_RUNTIME_DIR is not set".to_string())?;
        let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
        let path = if display.starts_with('/') {
            PathBuf::from(display)
        } else {
            PathBuf::from(runtime_dir).join(display)
        };

        let stream = UnixStream::connect(&path).map_err(|err| {
            format!(
                "failed to connect to Wayland display {}: {err}",
                path.display()
            )
        })?;

        Ok(Self {
            stream,
            bytes: Vec::new(),
            fds: Vec::new(),
        })
    }

    fn raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }

    fn send(
        &mut self,
        sender: u32,
        opcode: u16,
        args: &[Arg<'_>],
        fds: &[RawFd],
    ) -> Result<(), String> {
        let mut data = Vec::new();
        data.extend_from_slice(&sender.to_ne_bytes());
        data.extend_from_slice(&0u32.to_ne_bytes());

        for arg in args {
            match arg {
                Arg::U32(value) => data.extend_from_slice(&value.to_ne_bytes()),
                Arg::String(value) => push_string(&mut data, value),
                Arg::Fd => {}
            }
        }

        let size = data.len() as u32;
        let header = (size << 16) | opcode as u32;
        data[4..8].copy_from_slice(&header.to_ne_bytes());
        self.send_raw(&data, fds)
    }

    fn flush(&mut self) -> Result<(), String> {
        self.stream
            .flush()
            .map_err(|err| format!("failed to flush wayland socket: {err}"))
    }

    fn poll_read(&self, timeout_ms: i32) -> Result<bool, String> {
        let mut fd = PollFd {
            fd: self.raw_fd(),
            events: POLLIN,
            revents: 0,
        };
        let ready = unsafe { poll(&mut fd, 1, timeout_ms) };
        if ready < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }

        Ok(ready > 0 && fd.revents & POLLIN != 0)
    }

    fn has_buffered_message(&self) -> Result<bool, String> {
        if self.bytes.len() < 8 {
            return Ok(false);
        }

        Ok(self.bytes.len() >= message_size(&self.bytes)?)
    }

    fn next_message(&mut self) -> Result<Message, String> {
        while self.bytes.len() < 8 {
            self.recv_once()?;
        }

        let size = message_size(&self.bytes)?;
        while self.bytes.len() < size {
            self.recv_once()?;
        }

        let message_bytes = self.bytes.drain(..size).collect::<Vec<_>>();
        let sender = u32::from_ne_bytes(message_bytes[0..4].try_into().unwrap());
        let word = u32::from_ne_bytes(message_bytes[4..8].try_into().unwrap());
        let opcode = (word & 0xffff) as u16;
        let data = message_bytes[8..].to_vec();
        let mut fds = Vec::new();

        if source_send_event(opcode, &data) {
            if !self.fds.is_empty() {
                let fd = self.fds.remove(0);
                fds.push(fd);
            }
        }

        Ok(Message {
            sender,
            opcode,
            data,
            fds,
        })
    }

    fn recv_once(&mut self) -> Result<(), String> {
        let mut buffer = [0u8; 8192];
        let mut iov = Iovec {
            iov_base: buffer.as_mut_ptr(),
            iov_len: buffer.len(),
        };
        let mut control = [0u8; 256];
        let mut hdr = Msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: control.as_mut_ptr().cast(),
            msg_controllen: control.len(),
            msg_flags: 0,
        };

        let read = unsafe { recvmsg(self.raw_fd(), &mut hdr, 0) };
        if read < 0 {
            return Err(format!(
                "wayland recvmsg failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if read == 0 {
            return Err("wayland compositor closed the connection".to_string());
        }

        self.bytes.extend_from_slice(&buffer[..read as usize]);
        self.fds.extend(parse_fds(&control[..hdr.msg_controllen]));
        Ok(())
    }

    fn send_raw(&mut self, data: &[u8], fds: &[RawFd]) -> Result<(), String> {
        let iov = Iovec {
            iov_base: data.as_ptr() as *mut u8,
            iov_len: data.len(),
        };

        let mut control = vec![0u8; cmsg_space(mem::size_of_val(fds))];
        let (control_ptr, control_len) = if fds.is_empty() {
            (std::ptr::null_mut(), 0)
        } else {
            let cmsg = control.as_mut_ptr().cast::<Cmsghdr>();
            unsafe {
                (*cmsg).cmsg_len = cmsg_len(mem::size_of_val(fds));
                (*cmsg).cmsg_level = SOL_SOCKET;
                (*cmsg).cmsg_type = SCM_RIGHTS;
                std::ptr::copy_nonoverlapping(
                    fds.as_ptr().cast::<u8>(),
                    cmsg_data(cmsg),
                    mem::size_of_val(fds),
                );
            }
            (control.as_mut_ptr().cast(), control.len())
        };

        let hdr = Msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &iov as *const _ as *mut _,
            msg_iovlen: 1,
            msg_control: control_ptr,
            msg_controllen: control_len,
            msg_flags: 0,
        };

        let written = unsafe { sendmsg(self.raw_fd(), &hdr, 0) };
        if written < 0 {
            Err(format!(
                "wayland sendmsg failed: {}",
                std::io::Error::last_os_error()
            ))
        } else {
            Ok(())
        }
    }
}

fn source_send_event(opcode: u16, data: &[u8]) -> bool {
    if opcode != 0 || data.len() < 4 {
        return false;
    }

    let len = u32::from_ne_bytes(data[0..4].try_into().unwrap()) as usize;
    len > 0 && 4 + padded_len(len) == data.len()
}

fn padded_len(len: usize) -> usize {
    (len + 3) & !3
}

fn message_size(bytes: &[u8]) -> Result<usize, String> {
    let word = u32::from_ne_bytes(bytes[4..8].try_into().unwrap());
    let size = (word >> 16) as usize;
    if size < 8 {
        Err(format!("invalid wayland message size {size}"))
    } else {
        Ok(size)
    }
}

struct Message {
    sender: u32,
    opcode: u16,
    data: Vec<u8>,
    fds: Vec<RawFd>,
}

enum Arg<'a> {
    U32(u32),
    String(&'a str),
    Fd,
}

fn push_string(data: &mut Vec<u8>, value: &str) {
    let len = value.len() + 1;
    data.extend_from_slice(&(len as u32).to_ne_bytes());
    data.extend_from_slice(value.as_bytes());
    data.push(0);
    while data.len() % 4 != 0 {
        data.push(0);
    }
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn u32(&mut self) -> Result<u32, String> {
        if self.offset + 4 > self.data.len() {
            return Err("short wayland event".to_string());
        }

        let value = u32::from_ne_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        let len = self.u32()? as usize;
        if len == 0 || self.offset + len > self.data.len() {
            return Err("invalid wayland string".to_string());
        }

        let bytes = &self.data[self.offset..self.offset + len - 1];
        self.offset += len;
        while self.offset % 4 != 0 {
            self.offset += 1;
        }

        String::from_utf8(bytes.to_vec())
            .map_err(|err| format!("invalid utf-8 from compositor: {err}"))
    }
}

fn keymap() -> &'static str {
    r#"xkb_keymap {
xkb_keycodes "(unnamed)" {
    minimum = 8;
    maximum = 255;
    <LCTL> = 37;
    <LFSH> = 50;
    <K047> = 55;
};
xkb_types "(unnamed)" {
    include "complete"
};
xkb_compatibility "(unnamed)" {
    include "complete"
};
xkb_symbols "(unnamed)" {
    key <LCTL> { [ Control_L ] };
    key <LFSH> { [ Shift_L ] };
    key <K047> { [ v, V ] };
    modifier_map Control { <LCTL> };
    modifier_map Shift { <LFSH> };
};
};
"#
}

fn now_ms() -> u32 {
    now_ms_u64() as u32
}

fn now_ms_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn parse_fds(control: &[u8]) -> Vec<RawFd> {
    if control.len() < mem::size_of::<Cmsghdr>() {
        return Vec::new();
    }

    let cmsg = control.as_ptr().cast::<Cmsghdr>();
    let header = unsafe { &*cmsg };
    if header.cmsg_level != SOL_SOCKET || header.cmsg_type != SCM_RIGHTS {
        return Vec::new();
    }

    let data_len = header.cmsg_len.saturating_sub(cmsg_len(0));
    let count = data_len / mem::size_of::<RawFd>();
    let mut fds = Vec::with_capacity(count);
    let data = unsafe { cmsg_data(cmsg as *mut Cmsghdr).cast::<RawFd>() };
    for index in 0..count {
        fds.push(unsafe { *data.add(index) });
    }

    fds
}

fn cmsg_align(len: usize) -> usize {
    let align = mem::size_of::<usize>();
    (len + align - 1) & !(align - 1)
}

fn cmsg_space(len: usize) -> usize {
    cmsg_align(mem::size_of::<Cmsghdr>()) + cmsg_align(len)
}

fn cmsg_len(len: usize) -> usize {
    cmsg_align(mem::size_of::<Cmsghdr>()) + len
}

unsafe fn cmsg_data(cmsg: *mut Cmsghdr) -> *mut u8 {
    (cmsg as *mut u8).add(cmsg_align(mem::size_of::<Cmsghdr>()))
}

#[repr(C)]
struct Iovec {
    iov_base: *mut u8,
    iov_len: usize,
}

#[repr(C)]
struct Msghdr {
    msg_name: *mut std::ffi::c_void,
    msg_namelen: u32,
    msg_iov: *mut Iovec,
    msg_iovlen: usize,
    msg_control: *mut std::ffi::c_void,
    msg_controllen: usize,
    msg_flags: i32,
}

#[repr(C)]
struct Cmsghdr {
    cmsg_len: usize,
    cmsg_level: i32,
    cmsg_type: i32,
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

const SOL_SOCKET: i32 = 1;
const SCM_RIGHTS: i32 = 1;
const POLLIN: i16 = 0x001;

extern "C" {
    fn sendmsg(fd: i32, msg: *const Msghdr, flags: i32) -> isize;
    fn recvmsg(fd: i32, msg: *mut Msghdr, flags: i32) -> isize;
    fn pipe(fds: *mut i32) -> i32;
    fn close(fd: i32) -> i32;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::{parse_paste_text_request, request_label};

    #[test]
    fn parses_paste_text_payload_without_trimming_content() {
        let request = "PASTE_TEXT 123 12\nhello\nworld\n";
        let (sent_at, content) = parse_paste_text_request(request).unwrap().unwrap();

        assert_eq!(sent_at, "123");
        assert_eq!(content, "hello\nworld\n");
    }

    #[test]
    fn rejects_paste_text_length_mismatch() {
        let err = parse_paste_text_request("PASTE_TEXT 123 9\nhello").unwrap_err();

        assert!(err.contains("invalid PASTE_TEXT length"));
    }

    #[test]
    fn paste_text_request_label_hides_content() {
        assert_eq!(request_label("PASTE_TEXT 123 5\nhello"), "PASTE_TEXT len=5");
    }
}
