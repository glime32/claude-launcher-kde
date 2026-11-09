#!/bin/bash
# Installe (ou met à jour) le lanceur Claude Code pour KDE Plasma :
#   - « Claude Code » dans le menu des applications (Meta+C)
#   - une fenêtre Konsole par conversation, chacune avec sa propre couleur
#   - un menu fzf pour reprendre une conversation ou en commencer une titrée
set -e
here=$(cd "$(dirname "$0")" && pwd)
conf="$HOME/.config/claude-window"
bin="$HOME/.local/bin"

for cmd in cargo konsole fzf kwriteconfig6 qdbus6 notify-send; do
  command -v "$cmd" >/dev/null || { echo "Il manque « $cmd » (Arch : sudo pacman -S --needed rust konsole fzf kconfig qt6-tools libnotify)"; exit 1; }
done
command -v claude >/dev/null || [ -x "$bin/claude" ] || echo "Attention : Claude Code n'est pas installé (https://claude.com/claude-code)."

(cd "$here" && cargo build --release --locked)
mkdir -p "$bin" "$conf" "$HOME/.local/share/applications"
# copie puis renommage : les fenêtres déjà ouvertes gardent leur binaire
for b in claude-launcher claude-window claude-menu claude-hook claude-focus claude-couleurs; do
  install -m 755 "$here/target/release/$b" "$bin/.$b.new"
  mv -f "$bin/.$b.new" "$bin/$b"
done
[ -f "$conf/couleurs" ] || cp "$here/config/couleurs" "$conf/couleurs"   # garde tes couleurs si tu les as changées

# styles Konsole, thèmes Claude Code, icônes, règles KWin
"$bin/claude-couleurs"

first=$(awk '!/^#/ && NF {print $1; exit}' "$conf/couleurs")
icon="$HOME/.local/share/icons/claude-code.png"
[ -f "$icon" ] || icon="$HOME/.local/share/icons/claude-code-$first-tuile.png"
cat > "$HOME/.local/share/applications/claude-code.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Claude Code
GenericName=Assistant IA (terminal)
Comment=Reprendre une conversation Claude Code ou en commencer une nouvelle
Exec=$bin/claude-window
Icon=$icon
Terminal=false
Categories=Development;
Keywords=claude;ai;ia;code;assistant;
StartupNotify=false
X-KDE-Shortcuts=Meta+C
DESK
# à l'ouverture de session : rouvre les fenêtres qu'un plantage ou un arrêt a fermées
mkdir -p "$HOME/.config/autostart"
cat > "$HOME/.config/autostart/claude-reprise.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Claude Code : reprise après coupure
Comment=Rouvre les fenêtres Claude qu'un plantage ou un arrêt a fermées
Exec=$bin/claude-window --reprise
Icon=$icon
Terminal=false
DESK
update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true
kbuildsycoca6 >/dev/null 2>&1 || true

echo "Installé : cherche « Claude Code » dans le menu des applications, ou Meta+C."
echo "Couleurs : $conf/couleurs (relance ./install.sh après un changement)"
