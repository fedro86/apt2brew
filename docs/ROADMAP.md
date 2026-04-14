# Roadmap — apt2brew

## Fase 1 — Foundation (MVP)

Obiettivo: scansione funzionante con output a terminale.

- [x] Setup progetto Rust con Cargo.toml (metadata cargo-deb inclusi)
- [x] Implementare AptScanner: parsing `/var/lib/dpkg/status`
- [x] Filtrare solo pacchetti installati manualmente (`apt-mark showmanual`)
- [x] Implementare BrewMatcher: fetch e cache locale di `formulae.brew.sh/api/formula.json`
- [x] Matching nome APT → nome Brew (exact match + alias noti)
- [x] Output tabellare su stdout con stato di corrispondenza
- [x] Test con fixture dpkg e mock API

## Fase 2 — Intelligence

Obiettivo: classificazione automatica del rischio.

- [x] Risk Engine euristico (8 regole in cascata: systemd, sbin, /etc/, lib*, reverse deps, essential deps, sezioni dpkg)
- [x] Safety-net minimale (~30 pacchetti non rilevabili da euristiche: coreutils, POSIX, crypto, toolchain)
- [x] Rilevamento daemon (unit systemd, init.d scripts via `dpkg -L`)
- [x] Analisi file installati (sbin, /etc/ config) via `dpkg -L`
- [x] Analisi dipendenze inverse da pacchetti essential/required
- [x] Conteggio reverse dependencies (threshold >= 5 → High)
- [x] Pre-selezione automatica basata su RiskLevel
- [x] Subcomando `apt2brew scan` con output formattato (migrabile / non-migrabile / non-trovato)
- [x] Colonna Brew Version per confronto versioni APT vs Brew
- [x] Reason leggibile per ogni classificazione di rischio

## Fase 3 — TUI

Obiettivo: interfaccia interattiva per la selezione.

- [x] Implementare TUI con ratatui + crossterm
- [x] Checklist navigabile con frecce/j/k e spazio per toggle
- [x] Filtro per rischio (Tab: All / Migratable / High Risk / No Match)
- [x] Ricerca per nome pacchetto (/ per attivare)
- [x] Select all (a) / deselect all (n)
- [x] Riepilogo pre-conferma con overlay ("N pacchetti selezionati, confermi?")
- [x] Fallback non-interattivo con `--yes` per CI/scripting

## Fase 4 — Migration Engine

Obiettivo: esecuzione sicura della migrazione.

- [x] Generazione MigrationPlan da selezione utente (TUI o --yes)
- [x] `--dry-run` per modalità non-interattiva di preview
- [x] Workflow interattivo: TUI selezione → conferma → esecuzione immediata
- [x] TUI con progress bar live durante brew install
- [x] Verifica installazione via `brew list` (non PATH — nomi binari diversi dai nomi formula)
- [x] Rimozione batch da APT con singolo `sudo apt remove -y` (una sola password)
- [x] Rimozione batch da Snap con `sudo snap remove`
- [x] Supporto `brew install --cask` per applicazioni GUI
- [x] Generazione `~/Brewfile` con i pacchetti migrati
- [x] Generazione rollback script (`~/.apt2brew/rollback-<timestamp>.sh`)
- [x] Logging operazioni in `~/.apt2brew/logs/`
- [x] Rollback script pre-generato PRIMA di qualsiasi modifica

## Fase 5 — Rollback & Safety

Obiettivo: possibilità di annullare la migrazione.

- [x] Subcomando `apt2brew rollback` con TUI: selezione script → selezione pacchetti → progress bar
- [x] Rollback selettivo: `--package <name>` per singolo pacchetto
- [x] Conferma interattiva prima dell'esecuzione (skip con `--yes`)
- [x] Batch `brew uninstall` + singolo `sudo apt install -y` (una sola password)
- [x] Parsing rollback script per estrarre le entry
- [x] Test per il parser dei rollback script
- [x] Check prerequisito Homebrew con istruzioni shell-specific (bash/zsh/fish) per PATH setup

## Fase 6 — Polish & Distribution

Obiettivo: pronto per la distribuzione pubblica.

- [ ] Shell completions generate a build-time (bash, zsh, fish) via `clap_complete`
- [ ] Man page generata via `clap_mangen`
- [ ] Pacchetto `.deb` via `cargo-deb`
- [ ] CI/CD: build + test + generazione .deb su GitHub Actions
- [ ] Release su GitHub con binario e .deb allegati
- [ ] Pubblicazione su crates.io (`cargo install apt2brew`)
- [ ] README completo con GIF demo, istruzioni installazione, esempi

## Fase 7 — Matching & Sources (completata)

- [x] Fuzzy matching nomi APT → Brew (strip suffissi -dev/-bin/-utils, prefisso lib, python3-, vendor prefix, version suffix)
- [x] Supporto cask (fetch parallelo formulae + cask API, `brew install --cask`)
- [x] Cask blocklist per false positive (es. `dash` APT ≠ Dash macOS)
- [x] Scansione pacchetti Snap (`snap list`, filtraggio system snap)
- [x] Alias esternalizzati in file JSON (`aliases/apt-to-brew.json`, `snap-to-brew.json`, `blocklist.json`) — PR-friendly
- [x] Alias snap-specifici (es. `code` → `visual-studio-code`, `astral-uv` → `uv`)

## Fase 8 — Future (Post-release)

- [ ] Configurazione utente via file TOML (`~/.config/apt2brew/config.toml`)
- [ ] Plugin system per regole di rischio custom
- [ ] PPA di distribuzione per apt (`sudo add-apt-repository ppa:...`)
- [ ] Supporto Flatpak come source aggiuntiva
- [ ] Cache locale dell'API Homebrew (evita re-fetch ad ogni scan)
