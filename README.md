<p align="center"><img src="docs/banner-claude-launcher-kde.png" alt="claude-launcher-kde" width="100%"></p>

<p align="center"><img src="https://img.shields.io/badge/platform-KDE%20Plasma%206-1d99f3?style=flat-square" alt="platform"> <img src="https://img.shields.io/badge/Python%20·%20Bash%20·%20fzf-000000?style=flat-square" alt="Python Bash fzf"> <img src="https://img.shields.io/badge/license-MIT-610089?style=flat-square" alt="license"></p>

<p align="center"><a href="README.fr.md">🇫🇷 Version française</a></p>

**Type "claude" in the KDE app launcher: a window opens with all your Claude Code conversations, newest first. Pick one to resume it, or type a title to start a new one. Every window gets its own color, so you never mix them up.**

<p align="center"><img src="docs/fenetres.png" width="100%" alt="picker and two Claude windows"></p>

## What you get

- **A picker** (fzf) listing every Claude Code conversation from every folder, with its title, date and folder, and a preview with the first and last message.
- **Enter** resumes it in the right folder. **Ctrl-N**, or a search that matches nothing, starts a new conversation: you give a title, then a folder (Tab completes, a missing folder can be created).
- **One color per window** (8 colors: blue, purple, orange, green, pink, cyan, yellow, red). The tab bar, the 2 px frame, the cursor and the Claude Code theme all use it. The unfocused windows are dimmed. A 9th window is refused with a notification instead of reusing a color.
- **The title bar shows the conversation title** and follows it live (Claude's automatic title, or `/rename`).
- **Ctrl+Z works**: the launcher stays behind Claude and resumes it, since there is no shell in the window to type `fg`.

## Install

Needs KDE Plasma 6, Konsole, fzf, Python 3 and [Claude Code](https://claude.com/claude-code).

```bash
git clone https://github.com/glime32/claude-launcher-kde && cd claude-launcher-kde && ./install.sh
```

Then search **Claude Code** in the app launcher. The windows have no KDE title bar: move them with **Meta + drag**, resize with **Meta + right-drag**.

Change the colors in `~/.config/claude-window/couleurs` (`name r,g,b`), then run `./install.sh` again.

## How it works

| File | Role |
|---|---|
| `bin/claude-window` | Picks the first free color (lock + pid file per color in `$XDG_RUNTIME_DIR`), opens Konsole with that color's Qt stylesheet and settings. |
| `bin/claude-launcher` | Reads `~/.claude/projects/*/*.jsonl`, shows the fzf picker, then runs Claude in its own process group in the foreground of the terminal. |
| `tools/generer.py` | Generates, for each color, the Konsole stylesheet, the Claude Code theme (`~/.claude/themes/`) and a `.desktop` file for the taskbar icon. |
| `install.sh` | Installs everything and adds one KWin rule per color (no decoration). |

Two Konsole quirks are handled in `claude-launcher`: fzf runs in its own pseudo-terminal with the "synchronized update" sequences stripped (otherwise Konsole sometimes froze the window blank), and the window title is written by the launcher, not by Claude.

## Uninstall

```bash
./uninstall.sh
```

Your conversations are not touched.
