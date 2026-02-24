use console::{style, Term};
use serde_json::json;
use std::io::Write;

/// All user-facing output goes through this struct.
/// In `--json` mode it emits NDJSON events; otherwise styled terminal output.
pub struct Output {
    json_mode: bool,
    term: Term,
}

impl Output {
    pub fn new(json_mode: bool) -> Self {
        Self {
            json_mode,
            term: Term::stderr(),
        }
    }

    /// Print a numbered step: [n/total] message
    pub fn step(&self, n: u32, total: u32, msg: &str) {
        if self.json_mode {
            self.emit_json("step", &json!({ "n": n, "total": total, "message": msg }));
        } else {
            let _ = self.term.write_line(&format!(
                "{} {}",
                style(format!("[{n}/{total}]")).bold().cyan(),
                msg
            ));
        }
    }

    /// Stream a single line of output (for build logs, agent output, etc.)
    pub fn stream_line(&self, line: &str) {
        if self.json_mode {
            self.emit_json("stream", &json!({ "line": line }));
        } else {
            let _ = self.term.write_line(line);
        }
    }

    /// Print a success message
    pub fn success(&self, msg: &str) {
        if self.json_mode {
            self.emit_json("success", &json!({ "message": msg }));
        } else {
            let _ = self.term.write_line(&format!("{} {}", style("✔").green().bold(), msg));
        }
    }

    /// Print a warning message
    pub fn warn(&self, msg: &str) {
        if self.json_mode {
            self.emit_json("warn", &json!({ "message": msg }));
        } else {
            let _ = self
                .term
                .write_line(&format!("{} {}", style("⚠").yellow().bold(), msg));
        }
    }

    /// Print an error message
    pub fn error(&self, msg: &str) {
        if self.json_mode {
            self.emit_json("error", &json!({ "message": msg }));
        } else {
            let _ = self
                .term
                .write_line(&format!("{} {}", style("✖").red().bold(), msg));
        }
    }

    /// Print a clickable OSC 8 hyperlink (terminals that support it)
    pub fn link(&self, text: &str, url: &str) {
        if self.json_mode {
            self.emit_json("link", &json!({ "text": text, "url": url }));
        } else {
            // OSC 8 hyperlink: \x1b]8;;URL\x1b\\TEXT\x1b]8;;\x1b\\
            let _ = self
                .term
                .write_line(&format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\"));
        }
    }

    /// Print a "next step" hint after a command completes
    pub fn next_step(&self, hint: &str) {
        if self.json_mode {
            self.emit_json("next_step", &json!({ "hint": hint }));
        } else {
            let _ = self.term.write_line("");
            let _ = self.term.write_line(&format!(
                "{} {}",
                style("Next step:").bold().magenta(),
                hint
            ));
        }
    }

    /// Whether JSON mode is enabled
    pub fn is_json(&self) -> bool {
        self.json_mode
    }

    fn emit_json(&self, event: &str, data: &serde_json::Value) {
        let mut out = std::io::stdout().lock();
        let _ = serde_json::to_writer(&mut out, &json!({ "event": event, "data": data }));
        let _ = writeln!(out);
    }
}
