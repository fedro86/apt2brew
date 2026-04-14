# Code Review Checklist — apt2brew

## Safety (Priorità massima)

- [ ] Nessun pacchetto `essential` o `required` viene toccato
- [ ] La rimozione APT avviene SOLO dopo verifica che il binario brew è nel PATH
- [ ] Dry-run è il default; `--execute` è richiesto esplicitamente
- [ ] Un rollback script viene generato PRIMA di qualsiasi modifica
- [ ] I comandi di sistema (`apt remove`, `brew install`) non usano `--force` o flag pericolosi
- [ ] Nessun `sudo` hardcoded — la rimozione APT richiede privilegi e l'utente ne è informato

## Correctness

- [ ] I tipi di dominio (`RiskLevel`, `PackageMigration`) sono usati correttamente
- [ ] I match APT → Brew sono verificati (non solo per nome ma anche per funzionalità)
- [ ] Gli errori di rete (API Homebrew) sono gestiti con retry o fallback graceful
- [ ] Il parsing del database dpkg gestisce edge case (pacchetti rimossi ma non purgati, virtual packages)

## Rust Quality

- [ ] `cargo clippy` passa senza warning
- [ ] `cargo test` verde
- [ ] Error handling con `thiserror` e `?` operator, no `.unwrap()` in produzione
- [ ] Nessun `unsafe`
- [ ] Tipi pubblici hanno documentazione (`///`)

## UX

- [ ] I messaggi di errore sono utili e suggeriscono un'azione
- [ ] La TUI è navigabile solo con tastiera
- [ ] Il riepilogo pre-esecuzione è chiaro e completo
- [ ] `--help` è informativo per ogni subcomando
- [ ] Output non-interattivo (piped) è parsabile (no colori, no TUI)

## Distribution

- [ ] `cargo-deb` genera un .deb valido
- [ ] Il binario si installa in `/usr/bin/apt2brew`
- [ ] Shell completions e man page sono inclusi nel .deb
- [ ] La versione in `Cargo.toml` è aggiornata
