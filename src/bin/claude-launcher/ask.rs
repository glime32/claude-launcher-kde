//! Questions asked before a new conversation (title, folder), with line editing and
//! Tab completion of folder names.

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Asker {
    dirs: bool,
    home: PathBuf,
}

impl Completer for Asker {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        if !self.dirs {
            return Ok((0, Vec::new()));
        }
        let found = complete_dir(&self.home, &line[..pos])
            .into_iter()
            .map(|d| Pair { display: d.clone(), replacement: d })
            .collect();
        Ok((0, found))
    }
}

impl Hinter for Asker {
    type Hint = String;
}
impl Highlighter for Asker {}
impl Validator for Asker {}
impl Helper for Asker {}

pub struct Prompter {
    editor: Editor<Asker, DefaultHistory>,
    home: PathBuf,
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Prompter {
    pub fn new(home: &Path) -> Option<Prompter> {
        let config = Config::builder().completion_type(CompletionType::List).build();
        let editor = Editor::with_config(config).ok()?;
        Some(Prompter { editor, home: home.to_path_buf() })
    }

    /// `None` on Ctrl-C / Ctrl-D.
    pub fn ask(&mut self, styled: &str, dirs: bool) -> Option<String> {
        self.editor.set_helper(Some(Asker { dirs, home: self.home.clone() }));
        let raw = strip_ansi(styled);
        self.editor.readline(&(raw.as_str(), styled)).ok().map(|l| l.trim().to_string())
    }
}

/// `~`, `~/x`, absolute, or relative to the home folder.
pub fn expand(home: &Path, s: &str) -> PathBuf {
    let p = if s == "~" {
        home.to_path_buf()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(s)
    };
    if p.is_absolute() { p } else { home.join(p) }
}

/// Folders starting with `text`, written the way `text` is (~/…, absolute or relative to ~).
fn complete_dir(home: &Path, text: &str) -> Vec<String> {
    let full = if text.is_empty() {
        format!("{}/", home.display())
    } else {
        expand(home, text).to_string_lossy().into_owned()
    };
    let (dir, prefix) = match full.rfind('/') {
        Some(i) => (&full[..i.max(1)], &full[i + 1..]),
        None => (".", full.as_str()),
    };
    let mut found: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && (prefix.starts_with('.') || !n.starts_with('.')))
        .map(|n| if dir.ends_with('/') { format!("{dir}{n}") } else { format!("{dir}/{n}") })
        .filter(|p| Path::new(p).is_dir())
        .collect();
    found.sort();
    let home = home.to_string_lossy();
    found
        .into_iter()
        .map(|d| {
            let shown = if text.starts_with('~') {
                format!("~{}", d.get(home.len()..).unwrap_or(""))
            } else if !text.starts_with('/') {
                d.strip_prefix(&*home).map(|r| r.trim_start_matches('/')).unwrap_or(&d).to_string()
            } else {
                d
            };
            format!("{shown}/")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_like_typed() {
        let home = std::env::temp_dir().join(format!("ask-test-{}", std::process::id()));
        for d in ["Projets", "Prive", ".config", "claude"] {
            fs::create_dir_all(home.join(d)).unwrap();
        }
        fs::write(home.join("Prof.txt"), "").unwrap();
        assert_eq!(complete_dir(&home, "Pr"), ["Prive/", "Projets/"]);
        assert_eq!(complete_dir(&home, "~/Pro"), ["~/Projets/"]);
        assert_eq!(complete_dir(&home, "~/.c"), ["~/.config/"]);
        assert_eq!(complete_dir(&home, ""), ["Prive/", "Projets/", "claude/"]);
        let abs = format!("{}/cl", home.display());
        assert_eq!(complete_dir(&home, &abs), [format!("{}/claude/", home.display())]);
        fs::remove_dir_all(&home).unwrap();
        assert_eq!(strip_ansi("Titre \x1b[38;5;245m(x)\x1b[0m : "), "Titre (x) : ");
    }
}
