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

- [x] Implementare RiskEngine con euristiche di classificazione (5 regole in cascata)
- [x] Lista hardcoded di pacchetti system-critical (~45 pattern)
- [x] Rilevamento daemon (file in `/etc/init.d/`, unit systemd via `dpkg -L`)
- [x] Analisi dipendenze inverse da pacchetti essential/required
- [x] Pre-selezione automatica basata su RiskLevel
- [x] Subcomando `apt2brew scan` con output formattato (migrabile / non-migrabile / non-trovato)
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
- [x] Dry-run di default (stampa cosa farebbe senza eseguire)
- [x] Flag `--execute` per esecuzione reale
- [x] Installazione via `brew install` con progress per pacchetto
- [x] Verifica PATH: il binario brew è raggiungibile prima di rimuovere da APT
- [x] Rimozione da APT solo dopo verifica riuscita (sudo apt remove)
- [x] Generazione `~/Brewfile` con i pacchetti migrati
- [x] Generazione rollback script (`~/.apt2brew/rollback-<timestamp>.sh`)
- [x] Logging operazioni in `~/.apt2brew/logs/`
- [x] Rollback script pre-generato PRIMA di qualsiasi modifica

## Fase 5 — Rollback & Safety

Obiettivo: possibilità di annullare la migrazione.

- [x] Subcomando `apt2brew rollback` che trova e esegue lo script più recente
- [x] Rollback selettivo: `--package <name>` per singolo pacchetto
- [x] Conferma interattiva prima dell'esecuzione (skip con `--yes`)
- [x] Parsing rollback script per estrarre le entry (apt install + brew uninstall)
- [x] Test per il parser dei rollback script

## Fase 6 — Polish & Distribution

Obiettivo: pronto per la distribuzione pubblica.

- [ ] Shell completions generate a build-time (bash, zsh, fish) via `clap_complete`
- [ ] Man page generata via `clap_mangen`
- [ ] Pacchetto `.deb` via `cargo-deb`
- [ ] CI/CD: build + test + generazione .deb su GitHub Actions
- [ ] Release su GitHub con binario e .deb allegati
- [ ] Pubblicazione su crates.io (`cargo install apt2brew`)
- [ ] README completo con GIF demo, istruzioni installazione, esempi

## Fase 7 — Future (Post-release)

- [ ] Fuzzy matching nomi APT → Brew (es. `fd-find` → `fd`)
- [ ] Supporto cask (applicazioni GUI)
- [ ] Configurazione utente via file TOML (`~/.config/apt2brew/config.toml`)
- [ ] Plugin system per regole di rischio custom
- [ ] PPA di distribuzione per apt (`sudo add-apt-repository ppa:...`)
