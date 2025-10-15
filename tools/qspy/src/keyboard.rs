//! Keyboard Input Handler
//!
//! Interactive keyboard control for QSPY

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, ClearType},
    execute,
};
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

/// Keyboard command types
#[derive(Debug, Clone, Copy)]
pub enum KeyCommand {
    /// Exit QSPY
    Exit,
    /// Display help
    Help,
    /// Clear screen
    Clear,
    /// Toggle quiet mode
    ToggleQuiet,
    /// Send RESET to target
    Reset,
    /// Send INFO to target
    Info,
    /// Send TICK[0] to target
    Tick0,
    /// Send TICK[1] to target
    Tick1,
    /// Save dictionaries
    SaveDictionaries,
    /// Toggle screen output
    ToggleScreenOutput,
    /// Toggle binary output
    ToggleBinaryOutput,
    /// Toggle Matlab output
    ToggleMatlabOutput,
    /// Toggle sequence diagram output
    ToggleSequenceOutput,
}

impl KeyCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Exit => "Exit",
            Self::Help => "Help",
            Self::Clear => "Clear Screen",
            Self::ToggleQuiet => "Toggle Quiet Mode",
            Self::Reset => "Reset Target",
            Self::Info => "Request Target Info",
            Self::Tick0 => "Send TICK[0]",
            Self::Tick1 => "Send TICK[1]",
            Self::SaveDictionaries => "Save Dictionaries",
            Self::ToggleScreenOutput => "Toggle Screen Output",
            Self::ToggleBinaryOutput => "Toggle Binary Output",
            Self::ToggleMatlabOutput => "Toggle Matlab Output",
            Self::ToggleSequenceOutput => "Toggle Sequence Output",
        }
    }
}

/// Start keyboard listener thread
pub fn start_keyboard_listener(enable_keyboard: bool) -> Receiver<KeyCommand> {
    let (tx, rx) = mpsc::channel();
    
    if !enable_keyboard {
        return rx;
    }
    
    thread::spawn(move || {
        if let Err(e) = keyboard_thread(tx) {
            eprintln!("Keyboard thread error: {}", e);
        }
    });
    
    rx
}

/// Keyboard thread main loop
fn keyboard_thread(tx: Sender<KeyCommand>) -> io::Result<()> {
    // Enable raw mode for immediate key capture
    terminal::enable_raw_mode()?;
    
    let result = loop {
        // Poll for events with timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => {
                    if let Some(cmd) = map_key_to_command(key_event) {
                        if tx.send(cmd).is_err() {
                            break Ok(());
                        }
                        
                        // Exit on exit command
                        if matches!(cmd, KeyCommand::Exit) {
                            break Ok(());
                        }
                    }
                }
                _ => {}
            }
        }
    };
    
    // Restore terminal on exit
    terminal::disable_raw_mode()?;
    result
}

/// Map keyboard event to command
fn map_key_to_command(key: KeyEvent) -> Option<KeyCommand> {
    match key.code {
        KeyCode::Esc => Some(KeyCommand::Exit),
        KeyCode::Char('x') | KeyCode::Char('X') => Some(KeyCommand::Exit),
        KeyCode::Char('h') => Some(KeyCommand::Help),
        KeyCode::Char('c') => Some(KeyCommand::Clear),
        KeyCode::Char('q') => Some(KeyCommand::ToggleQuiet),
        KeyCode::Char('r') => Some(KeyCommand::Reset),
        KeyCode::Char('i') => Some(KeyCommand::Info),
        KeyCode::Char('t') => Some(KeyCommand::Tick0),
        KeyCode::Char('u') => Some(KeyCommand::Tick1),
        KeyCode::Char('d') => Some(KeyCommand::SaveDictionaries),
        KeyCode::Char('o') => Some(KeyCommand::ToggleScreenOutput),
        KeyCode::Char('s') | KeyCode::Char('b') => Some(KeyCommand::ToggleBinaryOutput),
        KeyCode::Char('m') => Some(KeyCommand::ToggleMatlabOutput),
        KeyCode::Char('g') => Some(KeyCommand::ToggleSequenceOutput),
        _ => None,
    }
}

/// Display keyboard help
pub fn display_help() {
    println!("\n{}", "=".repeat(70));
    println!("QSPY Keyboard Shortcuts:");
    println!("{}", "=".repeat(70));
    println!("KEY(s)             ACTION");
    println!("{}", "-".repeat(70));
    println!("<Esc>/x/X          Exit QSPY");
    println!("    h              Display keyboard help and QSPY status");
    println!("    c              Clear the screen");
    println!("    q              Toggle quiet mode (no Target data from QS)");
    println!("    r              Send RESET   command to the target");
    println!("    i              Send INFO    command to the target");
    println!("    t              Send TICK[0] command to the target");
    println!("    u              Send TICK[1] command to the target");
    println!("    d              Trigger saving dictionaries to a file");
    println!("    o              Toggle screen file output (close/re-open)");
    println!("    s/b            Toggle binary file output (close/re-open)");
    println!("    m              Toggle Matlab file output (close/re-open)");
    println!("    g              Toggle Message sequence output (close/re-open)");
    println!("{}", "=".repeat(70));
    println!();
}

/// Clear the screen
pub fn clear_screen() -> io::Result<()> {
    execute!(
        io::stdout(),
        terminal::Clear(ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )
}

/// Display QSPY status
pub fn display_status(
    quiet: bool,
    screen_output: bool,
    binary_output: bool,
    matlab_output: bool,
    sequence_output: bool,
) {
    println!("\n{}", "=".repeat(70));
    println!("QSPY Status:");
    println!("{}", "=".repeat(70));
    println!("Quiet Mode:        {}", if quiet { "ON" } else { "OFF" });
    println!("Screen Output:     {}", if screen_output { "ON" } else { "OFF" });
    println!("Binary Output:     {}", if binary_output { "ON" } else { "OFF" });
    println!("Matlab Output:     {}", if matlab_output { "ON" } else { "OFF" });
    println!("Sequence Output:   {}", if sequence_output { "ON" } else { "OFF" });
    println!("{}", "=".repeat(70));
    println!();
}
