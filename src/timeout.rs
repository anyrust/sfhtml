use anyhow::{bail, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A timeout guard that can be checked periodically
pub struct TimeoutGuard {
    deadline: Option<Instant>,
    cancelled: Arc<AtomicBool>,
}

impl TimeoutGuard {
    /// Create a new timeout guard. If timeout_ms is 0, no timeout is enforced.
    pub fn new(timeout_ms: u64) -> Self {
        let deadline = if timeout_ms == 0 {
            None
        } else {
            Some(Instant::now() + Duration::from_millis(timeout_ms))
        };
        Self {
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if the timeout has been reached. Returns Err if timed out.
    pub fn check(&self, command: &str, phase: &str) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            bail!("Command cancelled.");
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                let elapsed = self.elapsed_ms();
                bail!(
                    "Error: Command timed out after {}ms.\nCommand: {}\nPhase:   {}\nHint:    Increase timeout with --timeout {}, or check if the file is unusually large.",
                    elapsed,
                    command,
                    phase,
                    elapsed * 3
                );
            }
        }
        Ok(())
    }

    pub fn elapsed_ms(&self) -> u64 {
        if let Some(deadline) = self.deadline {
            let total = if let Some(d) = deadline.checked_duration_since(Instant::now()) {
                // Still have time left — calculate how much has passed
                let timeout_dur = deadline.duration_since(
                    deadline - self.deadline.map(|_| Duration::from_millis(0)).unwrap_or_default(),
                );
                timeout_dur.saturating_sub(d).as_millis() as u64
            } else {
                // Past deadline
                Instant::now().duration_since(deadline).as_millis() as u64
            };
            total
        } else {
            0
        }
    }

    /// Get the cancellation flag for sharing across threads
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
}

/// Default timeout values per command (in milliseconds)
pub fn default_timeout(command: &str) -> u64 {
    match command {
        "scan" | "search" => 30_000,
        "header" | "locate" | "read" | "init" | "check-output" => 5_000,
        "apply" | "validate" | "header-rebuild" => 10_000,
        _ => 5_000,
    }
}
