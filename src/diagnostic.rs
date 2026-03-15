use serde::Serialize;
use std::io::Write;

/// Diagnostic info appended to stderr when --diagnostic is used
#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub command: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub warnings: Vec<String>,
    pub syntax_health: SyntaxHealth,
}

#[derive(Debug, Serialize)]
pub struct SyntaxHealth {
    pub balanced: bool,
    pub unclosed: Vec<UnclosedSymbol>,
    pub output_parseable_as_json: bool,
}

#[derive(Debug, Serialize)]
pub struct UnclosedSymbol {
    pub symbol: String,
    pub context: String,
    pub line_in_header: Option<usize>,
}

impl Diagnostic {
    pub fn new(command: String) -> Self {
        Self {
            command,
            exit_code: 0,
            duration_ms: 0,
            stdout_bytes: 0,
            stderr_bytes: 0,
            warnings: Vec::new(),
            syntax_health: SyntaxHealth {
                balanced: true,
                unclosed: Vec::new(),
                output_parseable_as_json: false,
            },
        }
    }

    pub fn write_to_stderr(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = writeln!(std::io::stderr(), "{}", json);
        }
    }
}

/// Trace logger that writes to stderr when --trace is enabled
pub struct TraceLogger {
    enabled: bool,
}

impl TraceLogger {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn log(&self, msg: &str) {
        if self.enabled {
            let _ = writeln!(std::io::stderr(), "[trace] {}", msg);
        }
    }
}
