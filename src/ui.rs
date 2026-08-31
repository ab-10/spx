use colored::Colorize;

/// Print a success message.
pub fn success(message: &str) {
    eprintln!("{} {}", "✓".bold().green(), message);
}

/// Print a warning message.
pub fn warn(message: &str) {
    eprintln!("{} {}", "!".bold().yellow(), message);
}

/// Print an informational message.
pub fn info(message: &str) {
    eprintln!("{} {}", "→".bold().blue(), message);
}

/// Print a verbose debug message (only shown with -v).
pub fn verbose(message: &str) {
    eprintln!("{} {}", "[verbose]".dimmed(), message.dimmed());
}
