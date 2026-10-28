# MemGuard

**🇫🇷 Français** · [🇬🇧 English](#english)

Moniteur et optimiseur de mémoire RAM pour **Windows 11**, écrit en Rust. Conçu pour les PC qui ont peu de RAM : l'outil lui-même est ultra-léger (interface `egui` en OpenGL, sans `wgpu` ni `tokio`) et il agit de façon **ciblée** plutôt que de « vider » la mémoire à l'aveugle.

## Fonctionnalités

- **Moniteur en temps réel** : charge RAM, historique, standby, pages modifiées, commit / limite (valeurs réelles lues via `GlobalMemoryStatusEx` et `NtQuerySystemInformation`, aucune estimation).
- **Optimisation ciblée** des processus d'arrière-plan lourds :
  - priorité mémoire « très basse » (`SetProcessInformation`) : Windows les rogne en premier sous pression, sans pénaliser le programme actif ;
  - trim du working set ;
  - purge de la Standby List **basse priorité** uniquement (version douce, le cache utile est préservé).
- **Auto-optimisation** déclenchée seulement quand la charge dépasse un seuil (80 % par défaut), avec 60 s entre deux passages.
- **Protections** : le processus au premier plan, MemGuard lui-même, les processus système critiques et une **whitelist** (noms exacts) ne sont jamais touchés.
- **Très peu de mémoire consommée** : polling adaptatif (2 s actif / 5 s en arrière-plan / 10 s réduit), liste des processus libérée quand la fenêtre est cachée, trim de son propre working set.
- **Zone de notification** : la croix réduit dans la zone de notification ; menu *Afficher / Optimiser maintenant / Quitter* ; démarrage discret avec `--tray`.
- **Bilingue FR / EN** : langue de Windows détectée au démarrage, bascule à chaud avec les boutons FR | EN.

## Installation

Prérequis : Windows 10/11 x64, [Rust](https://rustup.rs) (édition 2021 ou 2024).

```powershell
git clone https://github.com/<votre-compte>/memguard.git
cd memguard
cargo build --release
```

L'exécutable se trouve dans `target\release\memguard.exe`.

> Le manifeste intégré demande les **droits administrateur** (nécessaires pour purger la Standby List via `SeProfileSingleProcessPrivilege`). Pour `cargo run`, utilise un terminal lancé en administrateur. Sans ce privilège, MemGuard fonctionne mais la purge Standby est désactivée.

## Utilisation

```text
memguard.exe                 # lance l'interface
memguard.exe --tray          # démarre directement dans la zone de notification
memguard.exe --lang=en       # force l'anglais (ou --lang=fr)
```

| Réglage | Rôle |
|---|---|
| **Optimiser maintenant** | Lance une passe immédiate |
| **Auto** + **seuil** | Optimise automatiquement au-dessus du seuil de charge |
| **min / processus** | Ignore les processus dont le working set est inférieur à cette valeur |
| **Purge Standby** | Purge la Standby List basse priorité (admin requis) |
| **Whitelist** | Noms exacts séparés par des virgules, ex. `code.exe, obs64.exe` |
| **Alléger** | Allège un processus précis de la liste |

Pour un démarrage automatique avec Windows, utilise le **Planificateur de tâches** avec « Exécuter avec les privilèges les plus élevés » (le dossier Démarrage ne convient pas à une application qui exige l'élévation).

## Comment ça marche (et ce que MemGuard ne fait pas)

Windows 11 gère déjà très bien la mémoire (compression mémoire, standby list). Les « nettoyeurs de RAM » classiques qui vident le working set de tous les processus sont souvent contre-productifs : les pages reviennent par *page faults* et provoquent des accès disque. MemGuard préfère :

1. orienter Windows (priorité mémoire basse sur l'arrière-plan) plutôt que forcer ;
2. n'agir que **sous pression mémoire** ;
3. mesurer le résultat (Δ de RAM libre avant/après) pour que tu voies si l'action sert vraiment.

Si un PC swappe en permanence, aucun réglage ne remplace de la RAM supplémentaire. Ne désactive jamais le fichier d'échange (pagefile).

## Structure du projet

```text
memguard/
├── Cargo.toml
├── build.rs            # embarque le manifeste « requireAdministrator »
└── src/
    ├── main.rs         # interface egui, icône de notification, thread de fond
    ├── win_api.rs      # tout le `unsafe` / API Windows (windows-sys)
    ├── optimizer.rs    # politique d'optimisation (protections, whitelist)
    └── i18n.rs         # traductions FR / EN
```

## Limites connues

- Windows uniquement.
- Quand la fenêtre est cachée, l'infobulle de l'icône n'est rafraîchie que si l'auto-optimisation est active.
- Le choix de la langue n'est pas mémorisé entre deux lancements (utiliser `--lang=`).

## Licence

À définir.

---

<a name="english"></a>

# MemGuard (English)

[🇫🇷 Français](#memguard) · **🇬🇧 English**

RAM monitor and optimizer for **Windows 11**, written in Rust. Built for PCs with little memory: the tool itself is extremely light (`egui` on OpenGL, no `wgpu`, no `tokio`) and it acts in a **targeted** way instead of blindly "flushing" memory.

## Features

- **Real-time monitor**: RAM load, history, standby, modified pages, commit / limit (real values from `GlobalMemoryStatusEx` and `NtQuerySystemInformation`, no estimates).
- **Targeted optimization** of heavy background processes:
  - "very low" memory priority (`SetProcessInformation`): Windows trims them first under pressure without hurting the active program;
  - working-set trim;
  - **low-priority** Standby List purge only (the gentle variant, useful cache is kept).
- **Auto-optimization** triggered only above a load threshold (80% by default), with a 60 s cooldown.
- **Safeguards**: the foreground process, MemGuard itself, critical system processes and a **whitelist** (exact names) are never touched.
- **Tiny footprint**: adaptive polling (2 s active / 5 s background / 10 s minimized), process list freed when the window is hidden, self working-set trim.
- **System tray**: the close button hides to the tray; *Show / Optimize now / Quit* menu; quiet startup with `--tray`.
- **Bilingual FR / EN**: Windows UI language detected at startup, live switch with the FR | EN buttons.

## Installation

Requirements: Windows 10/11 x64, [Rust](https://rustup.rs) (edition 2021 or 2024).

```powershell
git clone https://github.com/<your-account>/memguard.git
cd memguard
cargo build --release
```

The executable is at `target\release\memguard.exe`.

> The embedded manifest requests **administrator rights** (required to purge the Standby List via `SeProfileSingleProcessPrivilege`). For `cargo run`, use an elevated terminal. Without the privilege, MemGuard still works but the Standby purge is disabled.

## Usage

```text
memguard.exe                 # start the UI
memguard.exe --tray          # start directly in the system tray
memguard.exe --lang=en       # force English (or --lang=fr)
```

| Setting | Purpose |
|---|---|
| **Optimize now** | Runs one pass immediately |
| **Auto** + **threshold** | Optimizes automatically above the load threshold |
| **min / process** | Skips processes whose working set is below this value |
| **Purge Standby** | Purges the low-priority Standby List (admin required) |
| **Whitelist** | Exact names separated by commas, e.g. `code.exe, obs64.exe` |
| **Lighten** | Lightens one specific process from the list |

To start with Windows, use **Task Scheduler** with "Run with highest privileges" (the Startup folder doesn't work well for an app that requires elevation).

## How it works (and what MemGuard does not do)

Windows 11 already manages memory well (memory compression, standby list). Classic "RAM cleaners" that empty every process's working set are often counter-productive: pages come back through page faults and cause disk I/O. MemGuard prefers to:

1. steer Windows (low memory priority on background apps) rather than force it;
2. act only **under memory pressure**;
3. measure the result (free-RAM Δ before/after) so you can see whether it really helps.

If a PC swaps constantly, no setting replaces more RAM. Never disable the pagefile.

## Project layout

```text
memguard/
├── Cargo.toml
├── build.rs            # embeds the "requireAdministrator" manifest
└── src/
    ├── main.rs         # egui UI, tray icon, background thread
    ├── win_api.rs      # all `unsafe` / Windows API code (windows-sys)
    ├── optimizer.rs    # optimization policy (safeguards, whitelist)
    └── i18n.rs         # FR / EN translations
```

## Known limitations

- Windows only.
- While the window is hidden, the tray tooltip is refreshed only when auto-optimization is on.
- The language choice isn't remembered between runs (use `--lang=`).
