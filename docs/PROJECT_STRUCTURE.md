# Project Structure — apt2brew

```
apt2brew/
├── Cargo.toml                 # Manifest con metadata per cargo-deb
├── Cargo.lock
├── README.md
├── LICENSE
├── CONTRIBUTING.md
│
├── src/
│   ├── main.rs                # Entry point, setup CLI con clap
│   │
│   ├── domain/                # Core business logic, zero dipendenze esterne
│   │   ├── mod.rs
│   │   ├── package.rs         # Struct PackageMigration, RiskLevel enum
│   │   ├── risk.rs            # Regole di classificazione rischio
│   │   └── plan.rs            # MigrationPlan: raccolta di decisioni pre-esecuzione
│   │
│   ├── application/           # Orchestrazione dei casi d'uso
│   │   ├── mod.rs
│   │   ├── scan.rs            # Caso d'uso: scansione pacchetti
│   │   ├── migrate.rs         # Caso d'uso: esecuzione migrazione
│   │   └── rollback.rs        # Caso d'uso: ripristino stato precedente
│   │
│   ├── infrastructure/        # Integrazioni con sistemi esterni
│   │   ├── mod.rs
│   │   ├── apt.rs             # Lettura database dpkg/apt
│   │   ├── brew.rs            # Client API Homebrew + esecuzione comandi brew
│   │   ├── config.rs          # Lettura configurazione TOML
│   │   └── filesystem.rs      # Scrittura Brewfile, rollback script, log
│   │
│   └── presentation/          # Layer di presentazione
│       ├── mod.rs
│       ├── cli.rs             # Definizione comandi clap
│       └── tui/               # Interfaccia ratatui
│           ├── mod.rs
│           ├── app.rs         # Stato applicazione TUI
│           ├── render.rs      # Rendering checklist e riepilogo
│           └── input.rs       # Gestione input tastiera
│
├── tests/                     # Integration tests
│   ├── scan_test.rs
│   ├── matcher_test.rs
│   └── fixtures/              # Dati di test (mock dpkg status, mock API response)
│       ├── dpkg_status_sample
│       └── brew_api_sample.json
│
├── debian/                    # Metadata per pacchetto .deb (cargo-deb)
│   └── postinst               # Script post-installazione (opzionale)
│
├── completions/               # Shell completions (generate a build-time)
│   ├── apt2brew.bash
│   ├── apt2brew.zsh
│   └── apt2brew.fish
│
├── man/                       # Man pages
│   └── apt2brew.1
│
└── docs/                      # Documentazione progetto
    ├── ARCHITECTURE.md
    ├── PROJECT_STRUCTURE.md
    ├── DOMAIN_MODEL.md
    ├── ROADMAP.md
    ├── REVIEW_CHECKLIST.md
    └── temp/                  # Scratch pad (gitignored)
```

## Responsabilità dei layer

### `domain/`
Contiene **solo** la logica di business pura. Nessuna dipendenza esterna (no crate di I/O, no network).
Definisce i tipi core (`PackageMigration`, `RiskLevel`, `MigrationPlan`) e le regole di classificazione.
Tutto in questo layer è testabile con unit test puri, senza mock.

### `application/`
Orchestratori dei casi d'uso. Ricevono trait objects dalle infrastrutture e coordinano il flusso:
scan → match → classify → plan. Non contengono logica di business né dettagli di I/O.

### `infrastructure/`
Implementazioni concrete delle interfacce definite in domain.
Qui vivono: parsing del database dpkg, client HTTP per le API Homebrew, lettura/scrittura file.

### `presentation/`
Tutto ciò che riguarda l'interazione con l'utente: parsing argomenti CLI, rendering TUI,
gestione input. Nessuna logica di business.

### `debian/`
Metadata necessario per la generazione del pacchetto `.deb` tramite `cargo-deb`.
Include eventuali script di post-installazione.

### `completions/` e `man/`
Generati a build-time (via `clap_complete` e `clap_mangen`).
Installati nei path standard dal pacchetto .deb.
