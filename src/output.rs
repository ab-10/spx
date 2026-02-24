use colored::Colorize;
use serde::Serialize;

/// Format a clickable hyperlink using OSC 8 escape sequences for modern terminals.
pub fn hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Print a step indicator (e.g., "[1/5] Pulling Docker image...")
pub fn step(current: usize, total: usize, msg: &str) {
    println!(
        "{} {}",
        format!("[{current}/{total}]").bold().cyan(),
        msg
    );
}

/// Print a success message.
pub fn success(msg: &str) {
    println!("{} {}", "✓".bold().green(), msg);
}

/// Print an error message to stderr.
pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".bold().red(), msg);
}

/// Print a warning message.
pub fn warn(msg: &str) {
    println!("{} {}", "!".bold().yellow(), msg);
}

/// Print a "next step" hint — shown after every command per design principles.
pub fn next_step(msg: &str) {
    println!("\n{} {}", "→".bold().white(), msg.bold());
}

/// Print a section header.
pub fn header(msg: &str) {
    println!("\n{}", msg.bold().underline());
}

/// If --json is set, serialize the value and print it. Returns true if printed.
pub fn json_output<T: Serialize>(json: bool, value: &T) -> bool {
    if json {
        match serde_json::to_string_pretty(value) {
            Ok(s) => println!("{s}"),
            Err(e) => error(&format!("Failed to serialize JSON: {e}")),
        }
        true
    } else {
        false
    }
}

/// Stream output from a child process line by line, prefixed with a label.
pub fn stream_line(label: &str, line: &str) {
    println!("  {} {}", format!("[{label}]").dimmed(), line);
}
