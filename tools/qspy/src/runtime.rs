use std::error::Error;
use std::io::{self, BufRead, Read};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use clap::Parser;
use crate::commands::{try_send, CommandSender, SharedSender};
use crate::frontend::{FrontendCmd, FrontendServer};
use crate::output::{stdout_is_tty, OutputSinks};
use crate::{DecodeError, FrameInterpreter, HdlcDecoder, TargetSizes};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(author, version, about = "QSpy host-side decoder and tracing console")]
struct Opts {
    // ── Telemetry input (pick one; default = UDP) ──
    /// TCP telemetry listen address or port (target connects here to send QS frames).
    #[arg(short = 't', long = "tcp", value_name = "ADDR",
          conflicts_with_all = ["serial", "serial_path", "file", "tcp_remote"])]
    tcp: Option<String>,

    /// TCP remote server to connect to as client (e.g. Renode's CreateServerSocketTerminal).
    #[arg(long = "tcp-remote", value_name = "ADDR",
          conflicts_with_all = ["serial", "serial_path", "tcp", "file"])]
    tcp_remote: Option<String>,

    /// Replay a previously saved binary .qs file.
    #[arg(short = 'f', long = "file", value_name = "FILE",
          conflicts_with_all = ["serial", "serial_path", "tcp"])]
    file: Option<PathBuf>,

    /// UDP telemetry listen address (target sends datagrams here).
    #[arg(long = "udp", value_name = "ADDR", default_value = "0.0.0.0:7701")]
    udp_addr: String,

    /// Serial device path (e.g. /dev/ttyACM0).
    #[arg(short = 'c', long = "serial", value_name = "PATH",
          conflicts_with_all = ["tcp", "file"])]
    serial: Option<PathBuf>,

    /// Serial baud rate.
    #[arg(short = 'b', long = "baud", default_value_t = 115_200)]
    baud: u32,

    /// Serial device (positional shorthand).
    #[arg(value_name = "SERIAL_PATH",
          conflicts_with_all = ["serial", "tcp", "file"])]
    serial_path: Option<PathBuf>,

    // ── Output ──
    /// Write decoded text to file (auto-named when no path given).
    #[arg(short = 'o', value_name = "FILE", num_args = 0..=1,
          default_missing_value = "",
          help = "Text output file (auto-named if no argument)")]
    text_out: Option<String>,

    /// Save raw binary stream to a .qs file (auto-named when no path given).
    #[arg(short = 's', value_name = "FILE", num_args = 0..=1,
          default_missing_value = "",
          help = "Binary save file (auto-named if no argument)")]
    bin_out: Option<String>,

    /// Suppress console output.
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    // ── Channels ──
    /// TCP command-channel listen address (target connects here to receive QS-RX frames).
    #[arg(long = "cmd", default_value = "127.0.0.1:6601", value_name = "ADDR")]
    cmd_addr: String,

    /// Disable the TCP command-channel listener.
    #[arg(long = "no-cmd")]
    no_cmd: bool,

    /// UDP front-end server port for QView / QUTest (default 7701 when flag is given).
    #[arg(short = 'u', value_name = "PORT", num_args = 0..=1,
          default_missing_value = "7701",
          help = "UDP front-end server port for QView/QUTest")]
    frontend_port: Option<String>,

    // ── Dictionary ──
    /// Load dictionaries from file at startup; save with the `d` keyboard command.
    #[arg(short = 'd', value_name = "FILE", num_args = 0..=1,
          default_missing_value = "",
          help = "Load dictionary file at startup")]
    dict_file: Option<String>,

    // ── Output ──
    /// Disable ANSI color output (default: auto-detected from TTY).
    #[arg(long = "no-color")]
    no_color: bool,

    // ── Scripted / CI options ──
    /// Suppress keyboard input thread (for piped/CI use).
    #[arg(short = 'k', long = "no-kbd")]
    no_kbd: bool,

    /// Backwards-compatible QS version (e.g. 700 = "7.0.0", default 700).
    #[arg(short = 'v', value_name = "VER", default_value_t = 700)]
    qs_version: u16,

    // ── Target type sizes ──
    /// QS_TIME_SIZE in bytes.
    #[arg(short = 'T', value_name = "N", default_value_t = 4)] time_size:    u8,
    /// QS_OBJ_PTR_SIZE in bytes.
    #[arg(short = 'O', value_name = "N", default_value_t = 4)] obj_ptr_size: u8,
    /// QS_FUN_PTR_SIZE in bytes.
    #[arg(short = 'F', value_name = "N", default_value_t = 4)] fun_ptr_size: u8,
    /// QF_EVENT_SIZ_SIZE in bytes.
    #[arg(short = 'E', value_name = "N", default_value_t = 2)] event_size:   u8,
    /// QF_EQUEUE_CTR_SIZE in bytes.
    #[arg(short = 'Q', value_name = "N", default_value_t = 1)] equeue_ctr:   u8,
    /// QF_MPOOL_CTR_SIZE in bytes.
    #[arg(short = 'P', value_name = "N", default_value_t = 2)] mpool_ctr:    u8,
    /// QF_MPOOL_SIZ_SIZE in bytes.
    #[arg(short = 'B', value_name = "N", default_value_t = 2)] mpool_siz:    u8,
    /// QF_TIMEEVT_CTR_SIZE in bytes.
    #[arg(short = 'C', value_name = "N", default_value_t = 2)] timeevt_ctr:  u8,
}

// ── User command enum ─────────────────────────────────────────────────────────

enum UserCmd {
    Info,
    Reset,
    Tick(u8),
    SendCommand { id: u8, p1: u32, p2: u32, p3: u32 },
    SaveDict(PathBuf),
    ClearScreen,
    ToggleQuiet,
    Help,
    ToggleTextOut,
    ToggleBinOut,
    Quit,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run the qspy console.
///
/// Parses CLI options, builds the [`FrameInterpreter`], invokes `register`
/// so the caller can install project-specific user-record formatters via
/// [`FrameInterpreter::add_user_formatter`], then enters the transport loop.
/// qspy itself registers nothing, keeping the tool domain-agnostic.
///
/// `cmd_aliases` maps typed keyboard words (line mode only) to a QS-RX
/// `COMMAND` id, sent with `p1=p2=p3=0` — e.g. `[("tx", 1)]` lets a caller
/// type `tx` instead of `c 1 0 0 0`. qspy itself registers none (pass `&[]`);
/// a project-specific binary supplies its own table, keeping the generic
/// tool domain-agnostic (mirrors [`FrameInterpreter::add_user_formatter`]).
pub fn run<F>(register: F, cmd_aliases: &'static [(&'static str, u8)]) -> Result<(), Box<dyn Error>>
where
    F: FnOnce(&mut FrameInterpreter),
{
    let opts = Opts::parse();

    let sizes = TargetSizes {
        time_size:    opts.time_size,
        obj_ptr_size: opts.obj_ptr_size,
        fun_ptr_size: opts.fun_ptr_size,
        signal_size:  2,
        event_size:   opts.event_size,
        equeue_ctr:   opts.equeue_ctr,
        mpool_ctr:    opts.mpool_ctr,
        mpool_siz:    opts.mpool_siz,
        timeevt_ctr:  opts.timeevt_ctr,
    };

    let color = !opts.no_color
        && std::env::var_os("NO_COLOR").is_none()
        && stdout_is_tty();
    let mut sinks = OutputSinks::new(opts.quiet, color);
    if let Some(ref arg) = opts.text_out {
        let p = if arg.is_empty() { None } else { Some(Path::new(arg.as_str())) };
        sinks.open_text(p)?;
    }
    if let Some(ref arg) = opts.bin_out {
        let p = if arg.is_empty() { None } else { Some(Path::new(arg.as_str())) };
        sinks.open_binary(p)?;
    }

    let mut interpreter = FrameInterpreter::with_sizes(sizes);
    interpreter.set_qs_version(opts.qs_version);

    if let Some(ref arg) = opts.dict_file {
        if !arg.is_empty() {
            match interpreter.load_dictionaries(Path::new(arg)) {
                Ok(())  => eprintln!("dictionaries loaded from {arg}"),
                Err(e)  => eprintln!("dict load error: {e}"),
            }
        }
    }

    // Let the caller install project-specific record formatters.
    register(&mut interpreter);

    let shared_sender: SharedSender = Arc::new(Mutex::new(None));
    if !opts.no_cmd {
        let addr   = opts.cmd_addr.clone();
        let sender = Arc::clone(&shared_sender);
        thread::spawn(move || cmd_listener(&addr, sender));
    }

    let (kbd_tx, kbd_rx) = mpsc::channel::<UserCmd>();
    if !opts.no_kbd {
        thread::spawn(move || keyboard_loop(kbd_tx, cmd_aliases));
    }

    let mut frontend: Option<FrontendServer> = opts.frontend_port.as_ref().and_then(|port| {
        let addr = format!("0.0.0.0:{port}");
        match FrontendServer::bind(&addr) {
            Ok(fe) => Some(fe),
            Err(e) => { eprintln!("front-end server error: {e}"); None }
        }
    });

    let serial_path = opts.serial.clone().or_else(|| opts.serial_path.clone());

    if let Some(ref path) = serial_path {
        let s = serial::open(path, opts.baud)?;
        // Serial is inherently duplex — register a cloned handle as the
        // command sender so keyboard commands reach real hardware over the
        // same link, matching the `--tcp-remote` self-registration above.
        if let Ok(cmd_handle) = s.try_clone() {
            *shared_sender.lock().unwrap() = Some(CommandSender::new(Box::new(cmd_handle)));
        }
        run_reader(s, &mut interpreter, &mut sinks, &mut frontend, &shared_sender, &kbd_rx);
    } else if let Some(ref path) = opts.file {
        println!("qspy replaying {}", path.display());
        let f = std::fs::File::open(path)?;
        run_reader(f, &mut interpreter, &mut sinks, &mut frontend, &shared_sender, &kbd_rx);
    } else if let Some(ref addr) = opts.tcp {
        let bind_addr = if addr.contains(':') { addr.clone() } else { format!("0.0.0.0:{addr}") };
        let listener = TcpListener::bind(&bind_addr)?;
        println!("qspy listening on tcp://{bind_addr}");
        loop {
            match listener.accept() {
                Ok((stream, peer)) => {
                    println!("telemetry connected: {peer}");
                    if let Ok(cmd_stream) = stream.try_clone() {
                        if cmd_stream.set_nodelay(true).is_ok() {
                            *shared_sender.lock().unwrap() =
                                Some(CommandSender::new(Box::new(cmd_stream)));
                        }
                    }
                    run_reader(stream, &mut interpreter, &mut sinks, &mut frontend,
                               &shared_sender, &kbd_rx);
                    println!("telemetry disconnected: {peer}");
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
    } else if let Some(ref addr) = opts.tcp_remote {
        let addr = if addr.contains(':') { addr.clone() } else { format!("127.0.0.1:{addr}") };
        println!("qspy connecting to tcp://{addr}");
        let stream = TcpStream::connect(&addr)?;
        println!("qspy connected to {addr}");
        // The connected socket is already full-duplex (e.g. Renode's
        // CreateServerSocketTerminal bridges USB-serial-JTOG both ways over
        // one TCP connection) — register a clone as the command sender so
        // keyboard commands work without requiring a *second* connection
        // into `--cmd`'s listener, which nothing dials into in that setup.
        if let Ok(cmd_stream) = stream.try_clone() {
            if cmd_stream.set_nodelay(true).is_ok() {
                *shared_sender.lock().unwrap() = Some(CommandSender::new(Box::new(cmd_stream)));
            }
        }
        run_reader(stream, &mut interpreter, &mut sinks, &mut frontend, &shared_sender, &kbd_rx);
        println!("qspy disconnected from {addr}");
    } else {
        let socket = UdpSocket::bind(&opts.udp_addr)?;
        println!("qspy listening on udp://{}", opts.udp_addr);
        run_udp(socket, &mut interpreter, &mut sinks, &mut frontend, &shared_sender, &kbd_rx);
    }

    Ok(())
}

// ── Generic streaming reader ──────────────────────────────────────────────────

fn run_reader<R: Read>(
    mut source:  R,
    interpreter: &mut FrameInterpreter,
    sinks:       &mut OutputSinks,
    frontend:    &mut Option<FrontendServer>,
    sender:      &SharedSender,
    kbd_rx:      &mpsc::Receiver<UserCmd>,
) {
    let mut decoder = HdlcDecoder::new();
    let mut buf = [0u8; 4096];

    loop {
        match source.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = process_chunk(&buf[..n], &mut decoder, interpreter, sinks, frontend) {
                    eprintln!("decode error: {e}; resetting");
                    decoder.reset();
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => { eprintln!("read error: {e}"); break; }
        }

        if poll_commands(kbd_rx, frontend, interpreter, sender, sinks) {
            break;
        }
        sinks.flush();
    }
}

// ── UDP telemetry reader ──────────────────────────────────────────────────────

fn run_udp(
    socket:      UdpSocket,
    interpreter: &mut FrameInterpreter,
    sinks:       &mut OutputSinks,
    frontend:    &mut Option<FrontendServer>,
    sender:      &SharedSender,
    kbd_rx:      &mpsc::Receiver<UserCmd>,
) {
    socket.set_read_timeout(Some(std::time::Duration::from_millis(100))).ok();

    let mut decoder   = HdlcDecoder::new();
    let mut last_peer = String::new();
    let mut buf = [0u8; 4096];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                let peer_s = peer.to_string();
                if peer_s != last_peer {
                    println!("telemetry from {peer}");
                    last_peer = peer_s;
                }
                sinks.write_raw(&buf[..n]);
                if let Err(e) = process_chunk(&buf[..n], &mut decoder, interpreter, sinks, frontend) {
                    eprintln!("decode error: {e}; resetting");
                    decoder.reset();
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock
                   || e.kind() == io::ErrorKind::TimedOut => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => { eprintln!("udp error: {e}"); break; }
        }

        if poll_commands(kbd_rx, frontend, interpreter, sender, sinks) {
            break;
        }
        sinks.flush();
    }
}

// ── Core frame processing ─────────────────────────────────────────────────────

fn process_chunk(
    raw:         &[u8],
    decoder:     &mut HdlcDecoder,
    interpreter: &mut FrameInterpreter,
    sinks:       &mut OutputSinks,
    frontend:    &mut Option<FrontendServer>,
) -> Result<(), DecodeError> {
    sinks.write_raw(raw);
    for frame in decoder.push_bytes(raw)? {
        for line in interpreter.interpret(&frame) {
            sinks.write_line(&line);
            if let Some(fe) = frontend.as_mut() {
                fe.forward_text(&line);
            }
        }
        if let Some(fe) = frontend.as_mut() {
            fe.forward_frame(frame.record_type, &frame.payload);
        }
    }
    Ok(())
}

// ── Command polling (called at end of each loop iteration) ────────────────────

/// Drain all pending user and front-end commands.  Returns `true` if the
/// caller should exit its loop (Quit command received).
fn poll_commands(
    kbd_rx:   &mpsc::Receiver<UserCmd>,
    frontend: &mut Option<FrontendServer>,
    interp:   &mut FrameInterpreter,
    sender:   &SharedSender,
    sinks:    &mut OutputSinks,
) -> bool {
    while let Ok(cmd) = kbd_rx.try_recv() {
        if dispatch_cmd(cmd, sender, interp, sinks) {
            return true;
        }
    }
    if let Some(fe) = frontend {
        for fe_cmd in fe.poll() {
            dispatch_fe_cmd(fe_cmd, sender, sinks);
        }
    }
    false
}

/// Dispatch a user command.  Returns `true` if the caller should quit.
fn dispatch_cmd(
    cmd:    UserCmd,
    sender: &SharedSender,
    interp: &mut FrameInterpreter,
    sinks:  &mut OutputSinks,
) -> bool {
    match cmd {
        UserCmd::Info            => try_send(sender, |s| s.send_info()),
        UserCmd::Reset           => try_send(sender, |s| s.send_reset()),
        UserCmd::Tick(n)         => try_send(sender, |s| s.send_tick(n)),
        UserCmd::SendCommand { id, p1, p2, p3 } =>
            try_send(sender, |s| s.send_command(id, p1, p2, p3)),
        UserCmd::SaveDict(ref p) => match interp.save_dictionaries(p) {
            Ok(())  => println!("dictionaries saved to {}", p.display()),
            Err(e)  => eprintln!("dict save error: {e}"),
        },
        UserCmd::ClearScreen   => print!("\x1B[2J\x1B[H"),
        UserCmd::ToggleQuiet   => {
            let now_quiet = sinks.toggle_quiet();
            println!("quiet: {}", if now_quiet { "on" } else { "off" });
        }
        UserCmd::Help          => print_help(),
        UserCmd::ToggleTextOut => sinks.toggle_text(),
        UserCmd::ToggleBinOut  => sinks.toggle_binary(),
        UserCmd::Quit          => return true,
    }
    false
}

fn print_help() {
    println!("           Keys (raw mode): X=Quit  Q=Quiet  C=Clear  H=Help");
    println!("                           R=Reset  I=Info   T=Tick(0)  U=Tick(1)");
    println!("                           O=TextOut(toggle)  S/B=BinOut(toggle)  D=SaveDict");
    println!("           Line mode cmds: r/i/t/u/d/c/cls/quiet/help/text/bin/q");
}

fn dispatch_fe_cmd(cmd: FrontendCmd, sender: &SharedSender, sinks: &mut OutputSinks) {
    match cmd {
        FrontendCmd::Command { id, p1, p2, p3 } =>
            try_send(sender, |s| s.send_command(id, p1, p2, p3)),
        FrontendCmd::RawQsRx { id, payload } =>
            try_send(sender, |s| s.send_raw(id, &payload)),
        FrontendCmd::Info =>
            try_send(sender, |s| s.send_info()),
        FrontendCmd::ToggleTextOut  => sinks.toggle_text(),
        FrontendCmd::ToggleBinOut   => sinks.toggle_binary(),
        FrontendCmd::ShowNote(note) => sinks.write_line(&format!("           {note}")),
        FrontendCmd::SaveDict | FrontendCmd::ClearScreen => {}
    }
}

// ── Command-channel listener thread ──────────────────────────────────────────

fn cmd_listener(addr: &str, sender: SharedSender) {
    let listener = match TcpListener::bind(addr) {
        Ok(l)  => { println!("command listener on tcp://{addr}"); l }
        Err(e) => { eprintln!("command listener bind error: {e}"); return; }
    };
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "unknown".into());
                match stream.set_nodelay(true) {
                    Ok(()) => {
                        println!("command channel connected: {peer}");
                        *sender.lock().unwrap() = Some(CommandSender::new(Box::new(stream)));
                    }
                    Err(e) => eprintln!("command channel setup error: {e}"),
                }
            }
            Err(e) => eprintln!("command accept error: {e}"),
        }
    }
}

// ── Keyboard / stdin thread ───────────────────────────────────────────────────

fn keyboard_loop(tx: mpsc::Sender<UserCmd>, cmd_aliases: &'static [(&'static str, u8)]) {
    #[cfg(unix)]
    if crate::output::stdin_is_tty() {
        keyboard_loop_raw(tx, cmd_aliases);
        return;
    }
    keyboard_loop_line(tx, cmd_aliases);
}

fn keyboard_loop_line(tx: mpsc::Sender<UserCmd>, cmd_aliases: &'static [(&'static str, u8)]) {
    for line in io::stdin().lock().lines() {
        let raw = match line { Ok(l) => l, Err(_) => break };
        let cmd = parse_keyboard_cmd(raw.trim(), cmd_aliases);
        let quit = matches!(cmd, Some(UserCmd::Quit));
        if let Some(c) = cmd {
            if tx.send(c).is_err() { break; }
        }
        if quit { break; }
    }
}

#[cfg(unix)]
fn keyboard_loop_raw(tx: mpsc::Sender<UserCmd>, cmd_aliases: &'static [(&'static str, u8)]) {
    let _raw = match raw_terminal::RawTerminal::enter() {
        Some(r) => r,
        None    => { keyboard_loop_line(tx, cmd_aliases); return; }
    };
    let stdin = io::stdin();
    let mut locked = stdin.lock();
    let mut buf = [0u8; 1];
    loop {
        match locked.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let cmd = map_raw_key(buf[0]);
                let quit = matches!(cmd, Some(UserCmd::Quit));
                if let Some(c) = cmd {
                    if tx.send(c).is_err() { break; }
                }
                if quit { break; }
            }
        }
    }
}

#[cfg(unix)]
fn map_raw_key(b: u8) -> Option<UserCmd> {
    match b {
        b'X' | b'x' | 0x1B => Some(UserCmd::Quit),
        b'Q'                => Some(UserCmd::ToggleQuiet),
        b'C' | b'c'         => Some(UserCmd::ClearScreen),
        b'H' | b'h' | b'?' => Some(UserCmd::Help),
        b'O' | b'o'         => Some(UserCmd::ToggleTextOut),
        b'S' | b's' | b'B' | b'b' => Some(UserCmd::ToggleBinOut),
        b'R' | b'r'         => Some(UserCmd::Reset),
        b'I' | b'i'         => Some(UserCmd::Info),
        b'T' | b't'         => Some(UserCmd::Tick(0)),
        b'U' | b'u'         => Some(UserCmd::Tick(1)),
        b'D' | b'd'         => Some(UserCmd::SaveDict(
            PathBuf::from(crate::output::timestamped_name("dic"))
        )),
        _                   => None,
    }
}

fn parse_keyboard_cmd(line: &str, cmd_aliases: &'static [(&'static str, u8)]) -> Option<UserCmd> {
    let mut parts = line.splitn(5, ' ');
    let word = parts.clone().next()?;
    if let Some(&(_, id)) = cmd_aliases.iter().find(|&&(alias, _)| alias == word) {
        return Some(UserCmd::SendCommand { id, p1: 0, p2: 0, p3: 0 });
    }
    match parts.next()? {
        "r" | "reset"  => Some(UserCmd::Reset),
        "i" | "info"   => Some(UserCmd::Info),
        "t" | "tick"   => {
            let rate = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u8);
            Some(UserCmd::Tick(rate))
        }
        "u"            => Some(UserCmd::Tick(1)),
        "d" | "dict"   => {
            let p = parts.next().map(|s| s.trim()).unwrap_or("");
            let path = if p.is_empty() {
                PathBuf::from(crate::output::timestamped_name("dic"))
            } else {
                PathBuf::from(p)
            };
            Some(UserCmd::SaveDict(path))
        }
        "c" | "cmd"    => {
            let id = parts.next()?.parse::<u8>().ok()?;
            let p1 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u32);
            let p2 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u32);
            let p3 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u32);
            Some(UserCmd::SendCommand { id, p1, p2, p3 })
        }
        "cls"              => Some(UserCmd::ClearScreen),
        "quiet"            => Some(UserCmd::ToggleQuiet),
        "help"             => Some(UserCmd::Help),
        "text"             => Some(UserCmd::ToggleTextOut),
        "bin"              => Some(UserCmd::ToggleBinOut),
        "q" | "quit"       => Some(UserCmd::Quit),
        ""                 => None,
        other              => {
            eprintln!("unknown command: {other}  (r/i/t/u/d/c/cls/quiet/help/text/bin/q)");
            None
        }
    }
}

// ── Raw terminal mode (Unix only) ────────────────────────────────────────────

#[cfg(unix)]
mod raw_terminal {
    use std::mem::MaybeUninit;

    /// Sets stdin to non-canonical, no-echo mode; restores on drop.
    pub struct RawTerminal {
        fd:    libc::c_int,
        saved: libc::termios,
    }

    impl RawTerminal {
        pub fn enter() -> Option<Self> {
            let fd = libc::STDIN_FILENO;
            if unsafe { libc::isatty(fd) } == 0 { return None; }
            let mut saved_uninit = MaybeUninit::<libc::termios>::uninit();
            if unsafe { libc::tcgetattr(fd, saved_uninit.as_mut_ptr()) } != 0 { return None; }
            let saved = unsafe { saved_uninit.assume_init() };
            let mut raw = saved;
            // Disable canonical mode and echo; keep output post-processing (OPOST).
            raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ECHOE
                             | libc::ECHOK | libc::ECHONL);
            raw.c_cc[libc::VMIN]  = 1;
            raw.c_cc[libc::VTIME] = 0;
            if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 { return None; }
            Some(Self { fd, saved })
        }
    }

    impl Drop for RawTerminal {
        fn drop(&mut self) {
            unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved); }
        }
    }
}

// ── Serial port (Unix only) ───────────────────────────────────────────────────

#[cfg(unix)]
mod serial {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::mem::MaybeUninit;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;

    pub fn open(path: &Path, baud: u32) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true).write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(path)?;
        configure(file.as_raw_fd(), baud)?;
        Ok(file)
    }

    fn configure(fd: libc::c_int, baud: u32) -> io::Result<()> {
        let mut t = current_termios(fd)?;
        let speed = baud_constant(baud)?;
        unsafe {
            libc::cfmakeraw(&mut t);
            if libc::cfsetispeed(&mut t, speed) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::cfsetospeed(&mut t, speed) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        t.c_cflag |= libc::CLOCAL | libc::CREAD;
        t.c_cflag &= !libc::CSIZE;
        t.c_cflag |= libc::CS8;
        t.c_cflag &= !(libc::PARENB | libc::CSTOPB);
        #[cfg(any(target_os = "android", target_os = "linux"))]
        { t.c_cflag &= !libc::CRTSCTS; }
        t.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
        t.c_cc[libc::VMIN]  = 1;
        t.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &t) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn current_termios(fd: libc::c_int) -> io::Result<libc::termios> {
        let mut t = MaybeUninit::uninit();
        if unsafe { libc::tcgetattr(fd, t.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { t.assume_init() })
    }

    fn baud_constant(baud: u32) -> io::Result<libc::speed_t> {
        Ok(match baud {
            9_600   => libc::B9600,
            19_200  => libc::B19200,
            38_400  => libc::B38400,
            57_600  => libc::B57600,
            115_200 => libc::B115200,
            230_400 => libc::B230400,
            460_800 => libc::B460800,
            921_600 => libc::B921600,
            _ => return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported baud rate: {baud}"),
            )),
        })
    }
}

#[cfg(not(unix))]
mod serial {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    pub fn open(_path: &Path, _baud: u32) -> io::Result<File> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "serial devices are only supported on Unix targets",
        ))
    }
}
