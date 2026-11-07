#!/bin/bash
# Retire le lanceur, les styles, les thèmes et les règles KWin. Tes conversations ne sont pas touchées.
conf="$HOME/.config/claude-window"
noms=$(grep -v '^#' "$conf/couleurs" 2>/dev/null | awk 'NF {print $1}')
order=$(kreadconfig6 --file kwinrulesrc --group General --key Order)
old=$(kreadconfig6 --file kwinrulesrc --group General --key rules)
for n in $noms; do
  g="claude-code-$n"
  for key in Description decorationpolicy decorationpolicyrule wmclass wmclassmatch; do
    kwriteconfig6 --file kwinrulesrc --group "$g" --key "$key" --delete
  done
  order=$(echo ",$order," | sed "s/,$g,/,/; s/^,//; s/,$//")
  old=$(echo ",$old," | sed "s/,$g,/,/; s/^,//; s/,$//")
  rm -f "$HOME/.claude/themes/claude-$n.json" "$HOME/.local/share/applications/claude-code-$n.desktop" \
        "$conf/style-$n.qss"
done
[ -z "$(kreadconfig6 --file kwinrulesrc --group General --key Order)" ] || \
  kwriteconfig6 --file kwinrulesrc --group General --key Order "$order"
if [ -n "$(kreadconfig6 --file kwinrulesrc --group General --key rules)" ]; then
  kwriteconfig6 --file kwinrulesrc --group General --key rules "$old"
  kwriteconfig6 --file kwinrulesrc --group General --key count "$(echo "$old" | tr ',' '\n' | grep -c .)"
fi
[ -n "$NO_KWIN_RELOAD" ] || qdbus6 org.kde.KWin /KWin reconfigure 2>/dev/null || true
rm -f "$HOME/.local/bin/claude-launcher" "$HOME/.local/bin/claude-window" \
      "$HOME/.local/share/applications/claude-code.desktop" "$conf/couleurs" "$conf/defaults/konsolerc"
rmdir "$conf/defaults" "$conf" 2>/dev/null
echo "Désinstallé."
