#!/bin/bash
# Retire le lanceur, les styles, les thèmes, les icônes et les règles KWin. Tes conversations ne sont pas touchées.
conf="$HOME/.config/claude-window"
noms=$(grep -v '^#' "$conf/couleurs" 2>/dev/null | awk 'NF {print $1}')
order=$(kreadconfig6 --file kwinrulesrc --group General --key Order)
for n in $noms; do
  g="claude-code-$n"
  kwriteconfig6 --file kwinrulesrc --group "$g" --key Description --delete
  for key in decorationpolicy decorationpolicyrule wmclass wmclassmatch; do
    kwriteconfig6 --file kwinrulesrc --group "$g" --key "$key" --delete
  done
  order=$(echo ",$order," | sed "s/,$g,/,/; s/^,//; s/,$//")
  rm -f "$HOME/.claude/themes/claude-$n.json" "$HOME/.local/share/applications/claude-code-$n.desktop" \
        "$conf/style-$n.qss" "$HOME/.local/share/icons/claude-code-$n".{png,svg} \
        "$HOME/.local/share/icons/claude-code-$n-tuile".{png,svg}
done
[ -z "$(kreadconfig6 --file kwinrulesrc --group General --key Order)" ] || \
  kwriteconfig6 --file kwinrulesrc --group General --key Order "$order"
qdbus6 org.kde.KWin /KWin reconfigure 2>/dev/null || true
for b in claude-launcher claude-window claude-menu claude-hook claude-focus claude-couleurs; do
  rm -f "$HOME/.local/bin/$b"
done
rm -f "$HOME/.local/share/applications/claude-code.desktop" "$conf/couleurs" "$conf/defaults/konsolerc"
rm -rf "$HOME/.cache/claude-launcher"
rmdir "$conf/defaults" "$conf" 2>/dev/null
kbuildsycoca6 >/dev/null 2>&1
echo "Désinstallé. Restent tes épingles et la mémoire des couleurs dans ~/.local/state/claude-window/."
