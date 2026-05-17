use std::error::Error;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::thread;

use clap::Parser;
use qspy::{DecodeError, FrameInterpreter, HdlcDecoder};

#[derive(Parser, Debug)]
#[command(author, version, about = "Rust reimplementation of Quantum Spy")]
struct Opts {
    #[arg(
        long = "udp",
        default_value = "0.0.0.0:7701",
        value_name = "ADDR",
        help = "UDP address to listen on when no serial path is provided"
    )]
    udp_addr: String,

    #[arg(
        long = "cmd",
        default_value = "127.0.0.1:6601",
        value_name = "ADDR",
        help = "TCP command-channel address"
    )]
    cmd_addr: String,

    #[arg(
        long = "serial",
        value_name = "PATH",
        conflicts_with = "serial_path",
        help = "Serial device to read QS frames from, for example /dev/ttyACM0"
    )]
    serial: Option<PathBuf>,

    #[arg(
        long = "baud",
        default_value_t = 115_200,
        value_name = "RATE",
        help = "Serial baud rate"
    )]
    baud: u32,

    #[arg(long = "no-cmd", help = "Disable the TCP command listener")]
    no_cmd: bool,

    #[arg(
        value_name = "SERIAL_PATH",
        help = "Serial device to read QS frames from, for example /dev/ttyACM0"
    )]
    serial_path: Option<PathBuf>,
}

impl Opts {
    fn command_address(&self) -> Option<&str> {
        if self.no_cmd {
            None
        } else {
            Some(&self.cmd_addr)
        }
    }

    fn serial_path(&self) -> Option<&Path> {
        self.serial
            .as_deref()
            .or_else(|| self.serial_path.as_deref())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts = Opts::parse();

    if let Some(addr) = opts.command_address() {
        let addr = addr.to_string();
        thread::spawn(move || {
            if let Err(err) = run_command_listener(&addr) {
                eprintln!("command listener error: {err}");
            }
        });
    }

    if let Some(path) = opts.serial_path() {
        return run_serial(path, opts.baud).map_err(Into::into);
    }

    run_udp(&opts.udp_addr).map_err(Into::into)
}

fn run_udp(addr: &str) -> io::Result<()> {
    let socket = UdpSocket::bind(addr)?;
    println!("qspy listening on udp://{addr}");

    let mut decoder = HdlcDecoder::new();
    let mut interpreter = FrameInterpreter::new();
    let mut buf = [0u8; 4096];
    let mut last_peer: Option<String> = None;

    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, peer)) => {
                let peer_str = peer.to_string();
                if last_peer.as_deref() != Some(peer_str.as_str()) {
                    println!("telemetry from {peer}");
                }
                last_peer = Some(peer_str);

                if let Err(err) = decode_and_print(&mut decoder, &mut interpreter, &buf[..len]) {
                    eprintln!("decoder error: {err}; resetting state");
                    decoder.reset();
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                eprintln!("socket error: {err}");
                break;
            }
        }
    }

    Ok(())
}

fn run_serial(path: &Path, baud: u32) -> io::Result<()> {
    let mut serial = serial::open(path, baud)?;
    println!(
        "qspy listening on serial://{} at {baud} baud",
        path.display()
    );

    let mut decoder = HdlcDecoder::new();
    let mut interpreter = FrameInterpreter::new();
    let mut buf = [0u8; 4096];

    loop {
        match serial.read(&mut buf) {
            Ok(0) => continue,
            Ok(len) => {
                if let Err(err) = decode_and_print(&mut decoder, &mut interpreter, &buf[..len]) {
                    eprintln!("decoder error: {err}; resetting state");
                    decoder.reset();
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
}

fn decode_and_print(
    decoder: &mut HdlcDecoder,
    interpreter: &mut FrameInterpreter,
    bytes: &[u8],
) -> Result<(), DecodeError> {
    for frame in decoder.push_bytes(bytes)? {
        for line in interpreter.interpret(&frame) {
            println!("{line}");
        }
    }
    Ok(())
}

fn run_command_listener(addr: &str) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("command listener on tcp://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr().ok();
                println!("command channel connected: {peer:?}");
                thread::spawn(move || {
                    if let Err(err) = handle_command_stream(stream) {
                        eprintln!("command channel error: {err}");
                    }
                });
            }
            Err(err) => eprintln!("command accept error: {err}"),
        }
    }

    Ok(())
}

fn handle_command_stream(mut stream: TcpStream) -> io::Result<()> {
    stream.set_nodelay(true).ok();
    let peer = stream.peer_addr().ok();
    let mut sink = io::sink();
    let result = io::copy(&mut stream, &mut sink);
    println!("command channel closed: {peer:?}");
    result.map(|_| ())
}

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
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(path)?;

        configure(file.as_raw_fd(), baud)?;
        Ok(file)
    }

    fn configure(fd: libc::c_int, baud: u32) -> io::Result<()> {
        let mut termios = current_termios(fd)?;
        let speed = baud_constant(baud)?;

        unsafe {
            libc::cfmakeraw(&mut termios);
            if libc::cfsetispeed(&mut termios, speed) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::cfsetospeed(&mut termios, speed) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        termios.c_cflag |= libc::CLOCAL | libc::CREAD;
        termios.c_cflag &= !libc::CSIZE;
        termios.c_cflag |= libc::CS8;
        termios.c_cflag &= !(libc::PARENB | libc::CSTOPB);
        #[cfg(any(target_os = "android", target_os = "linux"))]
        {
            termios.c_cflag &= !libc::CRTSCTS;
        }
        termios.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
        termios.c_cc[libc::VMIN] = 1;
        termios.c_cc[libc::VTIME] = 0;

        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) } != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    fn current_termios(fd: libc::c_int) -> io::Result<libc::termios> {
        let mut termios = MaybeUninit::uninit();
        if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { termios.assume_init() })
    }

    fn baud_constant(baud: u32) -> io::Result<libc::speed_t> {
        let speed = match baud {
            9_600 => libc::B9600,
            19_200 => libc::B19200,
            38_400 => libc::B38400,
            57_600 => libc::B57600,
            115_200 => libc::B115200,
            230_400 => libc::B230400,
            460_800 => libc::B460800,
            921_600 => libc::B921600,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unsupported serial baud rate: {baud}"),
                ));
            }
        };
        Ok(speed)
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
