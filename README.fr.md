<p align="center"><img src="docs/banner-claude-launcher-kde.png" alt="claude-launcher-kde" width="100%"></p>

<p align="center"><img src="https://img.shields.io/badge/platform-KDE%20Plasma%206-1d99f3?style=flat-square" alt="platform"> <img src="https://img.shields.io/badge/Rust%20·%20fzf-000000?style=flat-square" alt="Rust fzf"> <img src="https://img.shields.io/badge/license-MIT-610089?style=flat-square" alt="license"></p>

<p align="center"><a href="README.md">🇬🇧 English version</a></p>

**Tape « claude » dans le lanceur d'applications KDE (ou Meta+C) : une fenêtre s'ouvre avec toutes tes conversations Claude Code, les plus récentes en haut. Choisis-en une pour la reprendre, ou tape un titre pour en commencer une nouvelle. Chaque fenêtre a sa propre couleur : impossible de les confondre.**

<p align="center"><img src="docs/fenetres.png" width="100%" alt="le menu et une fenêtre Claude"></p>

## Le menu

- **Toutes les conversations** de tous les dossiers, avec titre, date, dossier, et un aperçu du premier et du dernier message. Les conversations de plus de 7 jours sont en gris.
- **Entrée** reprend la conversation dans le bon dossier. Si elle est déjà ouverte dans une autre fenêtre (● de sa couleur), cette fenêtre passe au premier plan au lieu d'en ouvrir une deuxième.
- **Ctrl-N**, ou une recherche qui ne correspond à rien, en commence une nouvelle : tu donnes un titre, puis un dossier (Tab complète, les 6 derniers dossiers sont proposés par numéro, un dossier qui n'existe pas peut être créé).
- **Ctrl-F** cherche dans le texte des messages (sans tenir compte des accents ni des majuscules), **Ctrl-T** revient aux titres, **Ctrl-E** épingle une conversation en haut de la liste (📌).
- Le menu s'ouvre en 2 ms : un index par conversation est tenu à jour dans `~/.cache/claude-launcher/` et seule la fin des fichiers modifiés est relue.

## Les fenêtres

- **Une couleur par fenêtre** (8 couleurs : bleu, violet, orange, vert, rose, cyan, jaune, rouge). La barre d'onglet, le cadre, le curseur et le thème de Claude Code prennent cette couleur, et une conversation garde la sienne d'une fois sur l'autre. Les fenêtres inactives sont atténuées. Une 9e fenêtre est refusée avec une notification plutôt que de réutiliser une couleur.
- **La barre affiche le titre de la conversation** et le suit en direct (titre automatique de Claude, ou `/rename`), précédé de l'état : ⏳ Claude travaille, ✓ à toi, ❗ Claude attend ton accord.
- **Notification quand Claude a fini** ou attend une permission, seulement si tu ne regardes pas déjà cette fenêtre. Son bouton « Afficher » la ramène. Écran verrouillé : la notification part aussi sur ton téléphone avec [ntfy](https://ntfy.sh), si un topic est réglé dans `~/.config/claude-usage/config.json` (le même que [claude-usage-plasma](https://github.com/glime32/claude-usage-plasma), qui montre aussi une pastille par fenêtre ouverte).
- **Une entrée par fenêtre dans la barre des tâches**, avec une icône à sa couleur, au lieu de tout ranger sous Konsole.
- **Ctrl+Z marche** : le lanceur reste derrière Claude et le relance, car il n'y a pas de shell dans la fenêtre pour taper `fg`.

Les fenêtres n'ont pas de barre de titre KDE : déplace-les avec **Meta + glisser**, redimensionne avec **Meta + clic droit glissé**.

## Installation

Il faut KDE Plasma 6, Konsole, fzf, Rust (`cargo`) et [Claude Code](https://claude.com/claude-code).

```bash
git clone https://github.com/glime32/claude-launcher-kde && cd claude-launcher-kde && ./install.sh
```

Ensuite, cherche **Claude Code** dans le lanceur d'applications, ou Meta+C. Pour changer les couleurs : `~/.config/claude-window/couleurs` (`nom r,g,b`), puis relance `./install.sh`.

## Comment ça marche

Tout est en Rust (la version 0.1 était en bash et Python) : une petite bibliothèque commune et six binaires installés dans `~/.local/bin`.

| Binaire | Rôle |
|---|---|
| `claude-window` | Prend une couleur libre (verrou + fichier pid par couleur dans `$XDG_RUNTIME_DIR/claude-couleurs/`), ouvre Konsole avec la feuille de style Qt et les réglages de cette couleur. |
| `claude-launcher` | Affiche le menu fzf, puis lance Claude dans son propre groupe de processus, au premier plan du terminal, et le surveille. |
| `claude-menu` | Tient l'index des conversations et fournit les lignes, l'aperçu et la recherche au menu. |
| `claude-hook` | Crochet de Claude Code (passé par `--settings`, seulement dans ces fenêtres) : écrit l'état et envoie les notifications. Il n'affiche jamais rien, sinon Claude l'ajouterait à la conversation. |
| `claude-focus` | Met une fenêtre au premier plan par son pid (petit script KWin). |
| `claude-couleurs` | Génère pour chaque couleur la feuille de style Konsole, le thème Claude Code (`~/.claude/themes/`), les icônes, un `.desktop` caché et une règle KWin sans décoration. |

Quelques bizarreries de KDE gérées en route :

- fzf tourne dans son propre pseudo-terminal, sans les séquences de « mise à jour synchronisée » : sinon Konsole figeait parfois la fenêtre toute blanche ;
- c'est le lanceur qui écrit le titre de la fenêtre, pas Claude ;
- la barre des tâches de Plasma 6.8 range toute fenêtre dont l'exécutable s'appelle `konsole` sous Konsole, quel que soit son identifiant. `claude-window` lance donc une copie de Konsole nommée `claude-konsole` (dans `~/.cache/claude-launcher/`, recopiée quand Konsole est mis à jour), et chaque fenêtre retrouve son `.desktop` et son icône.

## Désinstaller

```bash
./uninstall.sh
```

Tes conversations ne sont pas touchées.
