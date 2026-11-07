#!/bin/bash
# Installe (ou met à jour) le lanceur Claude Code pour KDE Plasma :
#   - « Claude Code » dans le menu des applications
#   - une fenêtre Konsole par conversation, chacune avec sa propre couleur
#   - un menu fzf pour reprendre une conversation ou en commencer une titrée
set -e
here=$(cd "$(dirname "$0")" && pwd)
conf="$HOME/.config/claude-window"

for cmd in konsole fzf python3 kwriteconfig6 kreadconfig6; do
  command -v "$cmd" >/dev/null || { echo "Il manque « $cmd » (Arch : sudo pacman -S --needed konsole fzf python kconfig)"; exit 1; }
done
command -v claude >/dev/null || [ -x "$HOME/.local/bin/claude" ] || echo "Attention : Claude Code n'est pas installé (https://claude.com/claude-code)."

mkdir -p "$HOME/.local/bin" "$conf/defaults" "$HOME/.local/share/applications"
install -m 755 "$here/bin/claude-launcher" "$here/bin/claude-window" "$HOME/.local/bin/"
[ -f "$conf/couleurs" ] || cp "$here/config/couleurs" "$conf/couleurs"   # garde tes couleurs si tu les as changées
cp "$here/config/defaults/konsolerc" "$conf/defaults/konsolerc"

icon="$HOME/.local/share/icons/claude-code.png"
[ -f "$icon" ] || icon="utilities-terminal"
noms=$(python3 -I "$here/tools/generer.py" "$icon")

cat > "$HOME/.local/share/applications/claude-code.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Claude Code
GenericName=Assistant IA (terminal)
Comment=Reprendre une conversation Claude Code ou en commencer une nouvelle
Exec=$HOME/.local/bin/claude-window
Icon=$icon
Terminal=false
Categories=Development;
Keywords=claude;ai;ia;code;assistant;
StartupNotify=false
DESK

# Règles KWin : pas de barre de titre KDE (Konsole dessine sa propre barre colorée).
k() { kwriteconfig6 --file kwinrulesrc "$@"; }
order=$(kreadconfig6 --file kwinrulesrc --group General --key Order)
old=$(kreadconfig6 --file kwinrulesrc --group General --key rules)
for n in $noms; do
  g="claude-code-$n"
  k --group "$g" --key Description "Claude Code ($n) : sans décoration"
  k --group "$g" --key decorationpolicy none
  k --group "$g" --key decorationpolicyrule 2
  k --group "$g" --key wmclass "$g"
  k --group "$g" --key wmclassmatch 1
  case ",$order," in *",$g,"*) ;; *) order="${order:+$order,}$g" ;; esac
  case ",$old," in *",$g,"*) ;; *) old="${old:+$old,}$g" ;; esac
done
# Plasma récent : liste « Order » ; plus ancien : « rules » + « count ». Fichier neuf : les deux.
had_order=$(kreadconfig6 --file kwinrulesrc --group General --key Order)
had_rules=$(kreadconfig6 --file kwinrulesrc --group General --key rules)
if [ -n "$had_order" ] || [ -z "$had_rules" ]; then
  k --group General --key Order "$order"
fi
if [ -n "$had_rules" ] || [ -z "$had_order" ]; then
  k --group General --key rules "$old"
  k --group General --key count "$(echo "$old" | tr ',' '\n' | grep -c .)"
fi
if [ -z "$NO_KWIN_RELOAD" ]; then
  qdbus6 org.kde.KWin /KWin reconfigure 2>/dev/null || true
fi
update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true

echo "Installé : cherche « Claude Code » dans le menu des applications."
echo "Couleurs : $(echo $noms) (à changer dans $conf/couleurs, puis relancer ./install.sh)"
