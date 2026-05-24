use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct OutputSinks {
    quiet:    bool,
    text_out: Option<BufWriter<File>>,
    bin_out:  Option<BufWriter<File>>,
}

impl OutputSinks {
    pub fn new(quiet: bool) -> Self {
        Self { quiet, text_out: None, bin_out: None }
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
    pub fn write_line(&mut self, line: &str) {
        if !self.quiet {
            println!("{line}");
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
}

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
