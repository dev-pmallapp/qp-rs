use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct OutputSinks {
    quiet:    bool,
    color:    bool,
    text_out: Option<BufWriter<File>>,
    bin_out:  Option<BufWriter<File>>,
}

impl OutputSinks {
    pub fn new(quiet: bool, color: bool) -> Self {
        Self { quiet, color, text_out: None, bin_out: None }
    }

    /// Open a text output file. `path = None` means auto-generate a timestamped name.
    pub fn open_text(&mut self, path: Option<&Path>) -> io::Result<()> {
        let p = match path {
            Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
            _ => PathBuf::from(timestamped_name("txt")),
        };
        let f = OpenOptions::new().create(true).append(true).open(&p)?;
        println!("text output: {}", p.display());
        self.text_out = Some(BufWriter::with_capacity(64 * 1024, f));
        Ok(())
    }

    /// Open a binary save file. `path = None` means auto-generate a timestamped name.
    pub fn open_binary(&mut self, path: Option<&Path>) -> io::Result<()> {
        let p = match path {
            Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
            _ => PathBuf::from(timestamped_name("qs")),
        };
        let f = OpenOptions::new().create(true).write(true).truncate(true).open(&p)?;
        println!("binary save: {}", p.display());
        self.bin_out = Some(BufWriter::with_capacity(64 * 1024, f));
        Ok(())
    }

    /// Write a decoded text line to console (unless quiet) and to the text file.
    /// ANSI colors are applied to the console only; the text file always gets plain text.
    pub fn write_line(&mut self, line: &str) {
        if !self.quiet {
            if self.color {
                println!("{}", colorize_line(line));
            } else {
                println!("{line}");
            }
        }
        if let Some(f) = &mut self.text_out {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Write raw input bytes (before HDLC decoding) to the binary save file.
    pub fn write_raw(&mut self, bytes: &[u8]) {
        if let Some(f) = &mut self.bin_out {
            let _ = f.write_all(bytes);
        }
    }

    pub fn flush(&mut self) {
        if let Some(f) = &mut self.text_out { let _ = f.flush(); }
        if let Some(f) = &mut self.bin_out  { let _ = f.flush(); }
    }

    /// Toggle text output file: close it if open, open a new auto-named one if closed.
    pub fn toggle_text(&mut self) {
        if self.text_out.is_some() {
            self.text_out = None;
            println!("text output: closed");
        } else {
            let _ = self.open_text(None);
        }
    }

    /// Toggle quiet mode; returns the new state.
    pub fn toggle_quiet(&mut self) -> bool {
        self.quiet = !self.quiet;
        self.quiet
    }

    /// Toggle binary save file: close it if open, open a new auto-named one if closed.
    pub fn toggle_binary(&mut self) {
        if self.bin_out.is_some() {
            self.bin_out = None;
            println!("binary save: closed");
        } else {
            let _ = self.open_binary(None);
        }
    }
}

// ── ANSI color codes ──────────────────────────────────────────────────────────

const RESET:        &str = "\x1b[0m";
const BOLD:         &str = "\x1b[1m";
const DIM:          &str = "\x1b[2m";
const CYAN:         &str = "\x1b[36m";
const GREEN:        &str = "\x1b[32m";
const YELLOW:       &str = "\x1b[33m";
const BLUE:         &str = "\x1b[34m";
const MAGENTA:      &str = "\x1b[35m";
const BRIGHT_WHITE: &str = "\x1b[97m";
const RED:          &str = "\x1b[31m";

/// Apply ANSI color codes to a single decoded line.
///
/// Console output only — the text file always receives the plain string.
///
/// Color scheme (matches reference QSPY):
/// - Cyan timestamps + cyan/bold `===RTC===>` prefix
/// - Yellow: AO-Post / AO-Get / Disp===>
/// - Green:  =>Intern / ===>Tran / Init===> / =>Ignore / =>UnHndl
/// - Blue:   Sch-*
/// - Magenta: TE* / QF-* / MP-*
/// - Dim:    dict / info entries (11-space or `##########` prefix)
/// - Bright white: user records
pub fn colorize_line(line: &str) -> String {
    // ===RTC===> prefix (state machine RTC records, no timestamp)
    if line.starts_with("===RTC===>") {
        let (pfx, rest) = line.split_at(10);
        return format!("{BOLD}{CYAN}{pfx}{RESET}{rest}");
    }

    // Indented entries: dict records, target info, no-timestamp TE records
    if line.starts_with("           ") || line.starts_with("########## ") {
        return format!("{DIM}{line}{RESET}");
    }

    // Timestamped records: 10-digit timestamp + space + content
    if line.len() >= 11 {
        let ts_bytes = &line.as_bytes()[..10];
        if ts_bytes.iter().all(|b| b.is_ascii_digit()) && line.as_bytes()[10] == b' ' {
            let ts   = &line[..10];
            let rest = &line[10..]; // leading space included
            let kw   = rest.trim_start();
            let color = keyword_color(kw);
            return format!("{CYAN}{ts}{RESET}{color}{rest}{RESET}");
        }
    }

    line.to_string()
}

fn keyword_color(kw: &str) -> &'static str {
    if kw.starts_with("=>Intern") || kw.starts_with("===>Tran")
       || kw.starts_with("Init===>") || kw.starts_with("=>Ignore")
       || kw.starts_with("=>UnHndl")
    {
        GREEN
    } else if kw.starts_with("Disp===>") || kw.starts_with("AO-") {
        YELLOW
    } else if kw.starts_with("Sch-") {
        BLUE
    } else if kw.starts_with("TE") || kw.starts_with("QF-") || kw.starts_with("MP-")
              || kw.starts_with("EP-") || kw.starts_with("QS-")
              || kw.starts_with("New-Ref") || kw.starts_with("EQ-")
    {
        MAGENTA
    } else if kw.starts_with("=ASSERT=") || kw.starts_with("rec=") {
        RED
    } else {
        BRIGHT_WHITE
    }
}

/// Returns `true` when stdout is connected to a real terminal.
pub fn stdout_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDOUT_FILENO) != 0 }
}

/// Returns `true` when stdin is connected to a real terminal.
pub fn stdin_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) != 0 }
}

// ── Filename helper ───────────────────────────────────────────────────────────

/// Generate `qspyYYMMDD_HHMMSS.<ext>` from the current wall-clock time.
pub fn timestamped_name(ext: &str) -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Rough but good-enough calendar decomposition (ignores leap seconds / leap years).
    let s   = secs % 60;
    let m   = (secs / 60) % 60;
    let h   = (secs / 3600) % 24;
    let days = secs / 86400;

    // Days since 1970-01-01; approximate month/day (good enough for a filename).
    let year400 = days / 146097;
    let rem     = days % 146097;
    let year100 = rem / 36524;
    let rem     = rem % 36524;
    let year4   = rem / 1461;
    let rem     = rem % 1461;
    let year1   = rem / 365;
    let year    = 1970 + year400 * 400 + year100 * 100 + year4 * 4 + year1;
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut doy = (rem % 365) as u32;

    let days_in_month: [u32; 12] = [31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30,
                                     31, 31, 30, 31, 30, 31];
    let mut month = 1u32;
    for d in &days_in_month {
        if doy < *d { break; }
        doy -= d;
        month += 1;
    }
    let day = doy + 1;

    format!("qspy{:02}{month:02}{day:02}_{h:02}{m:02}{s:02}.{ext}", year % 100)
}
