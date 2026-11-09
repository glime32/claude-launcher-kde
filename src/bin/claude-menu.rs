//! Called by fzf: list | search WORDS | preview ID [WORDS] | pin ID | info ID | dirs

use claude_launcher::{Paths, index, menu};
use std::env;
use std::io::{self, Write};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let paths = Paths::new();
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    let rest = |i: usize| args.get(i..).map(|a| a.join(" ")).unwrap_or_default();
    let out = match arg(0) {
        "list" => menu::list(&paths),
        "search" => menu::search(&paths, &rest(1)),
        "preview" => menu::preview(&paths, arg(1), &rest(2)),
        "pin" => {
            menu::toggle_pin(&paths, arg(1));
            String::new()
        }
        "info" => match index::find_session(&paths, arg(1)) {
            Some(s) => {
                let json = serde_json::json!({
                    "id": s.id,
                    "title": s.title,
                    "cwd": s.meta.cwd,
                    "custom": s.meta.custom.is_some(),
                    "file": s.file,
                });
                format!("{json}\n")
            }
            None => String::new(),
        },
        "dirs" => format!("{}\n", serde_json::to_string(&menu::recent_dirs(&paths)).unwrap_or_default()),
        _ => {
            eprintln!("usage: claude-menu list | search MOTS | preview ID [MOTS] | pin ID | info ID | dirs");
            std::process::exit(2);
        }
    };
    // fzf may stop reading when a newer reload replaces this one
    let _ = io::stdout().lock().write_all(out.as_bytes());
}
