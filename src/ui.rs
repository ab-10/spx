use colored::Colorize;

/// Print a step header (e.g. "[1/7] Pulling spawn base Docker image...")
pub fn step(current: usize, total: usize, message: &str) {
    eprintln!(
        "{} {}",
        format!("[{current}/{total}]").bold().cyan(),
        message
    );
}

/// Print a success message.
pub fn success(message: &str) {
    eprintln!("{} {}", "✓".bold().green(), message);
}

/// Print an error message.
pub fn error(message: &str) {
    eprintln!("{} {}", "✗".bold().red(), message);
}

/// Print a warning message.
pub fn warn(message: &str) {
    eprintln!("{} {}", "!".bold().yellow(), message);
}

/// Print an informational message.
pub fn info(message: &str) {
    eprintln!("{} {}", "→".bold().blue(), message);
}

/// Print a "next step" hint — shown after every command per design principles.
pub fn next_step(message: &str) {
    eprintln!();
    eprintln!("{} {}", "Next:".bold().green(), message);
}

/// Render a clickable URL using OSC 8 hyperlinks for modern terminals.
pub fn hyperlink(url: &str, label: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
}

/// Stream a command's output line-by-line to stderr (real-time visibility).
pub fn stream_header(command: &str) {
    eprintln!("{} {}", "$".dimmed(), command.dimmed());
}
