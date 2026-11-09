//! Raises the window of a process: claude-focus PID

use claude_launcher::{Paths, focus};

fn main() {
    let pid = std::env::args().nth(1).unwrap_or_default();
    std::process::exit(if focus(&Paths::new(), &pid) { 0 } else { 1 });
}
