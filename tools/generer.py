#!/usr/bin/env python3
"""Génère, pour chaque couleur de ~/.config/claude-window/couleurs :
  - ~/.config/claude-window/style-<nom>.qss       (barre noire, titre et cadre en couleur)
  - ~/.claude/themes/claude-<nom>.json            (thème Claude Code assorti)
  - ~/.local/share/applications/claude-code-<nom>.desktop (icône de la barre des tâches)
et affiche les noms des couleurs (pour les règles KWin posées par install.sh).
"""
import json
import sys
from pathlib import Path

HOME = Path.home()
CONF = HOME / ".config/claude-window"
icon = sys.argv[1] if len(sys.argv) > 1 else "utilities-terminal"


def mix(c, other, t):
    return tuple(round(a * (1 - t) + b * t) for a, b in zip(c, other))


def hx(c):
    return "#%02x%02x%02x" % c


def rgb(c):
    return "rgb(%d,%d,%d)" % c


QSS = """/* Fenêtre Claude {nom} : barre noire + titre en couleur + cadre fin (généré) */
QMainWindow, QTabWidget, QStackedWidget, QTabBar {{ background: #000000; }}
QTabBar::tab {{ background: #000000; color: {c}; font-weight: bold; padding: 0px 10px;
               border: 2px solid {c}; border-bottom: none; margin: 0px; }}
QTabBar::tab:!active {{ color: {d}; border-color: {d}; }}
QTabBar::close-button {{ image: none; width: 0px; height: 0px; }}
QTabWidget::right-corner, QTabWidget::left-corner {{ width: 0px; height: 0px; }}
QTabWidget QToolButton {{ width: 0px; max-width: 0px; min-width: 0px; padding: 0px; margin: 0px; border: none; }}
QSplitter {{ border: 2px solid {c}; border-top: none; }}
QSplitter:!active {{ border-color: {d}; }}
"""

DESKTOP = """[Desktop Entry]
Type=Application
Name=Claude Code
Comment=Fenêtre Claude Code ({nom})
Exec={home}/.local/bin/claude-window
Icon={icon}
NoDisplay=true
Terminal=false
"""

(HOME / ".claude/themes").mkdir(parents=True, exist_ok=True)
(HOME / ".local/share/applications").mkdir(parents=True, exist_ok=True)
for line in (CONF / "couleurs").read_text().splitlines():
    if not line.strip() or line.startswith("#"):
        continue
    nom, val, *reste = line.split()
    c = tuple(int(x) for x in val.split(","))
    inactif = int(reste[0]) / 100 if reste else 0.75   # luminosité du cadre d'une fenêtre inactive
    dim = mix(c, (0, 0, 0), 1 - inactif)
    (CONF / f"style-{nom}.qss").write_text(QSS.format(nom=nom, c=hx(c), d=hx(dim)))
    light, bg = mix(c, (255, 255, 255), 0.45), mix(c, (0, 0, 0), 0.75)
    hover = tuple(min(255, x + 12) for x in bg)
    theme = {"name": f"Claude {nom}", "base": "dark", "overrides": {
        "claude": rgb(c), "claudeShimmer": rgb(light),
        "promptBorder": rgb(c), "promptBorderShimmer": rgb(light),
        "clawd_body": rgb(c), "briefLabelClaude": rgb(c),
        "userMessageBackground": rgb(bg), "userMessageBackgroundHover": rgb(hover)}}
    (HOME / f".claude/themes/claude-{nom}.json").write_text(json.dumps(theme, indent=2, ensure_ascii=False) + "\n")
    (HOME / f".local/share/applications/claude-code-{nom}.desktop").write_text(
        DESKTOP.format(nom=nom, home=HOME, icon=icon))
    print(nom)
