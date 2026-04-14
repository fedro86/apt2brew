# Contributing to apt2brew

## Setup ambiente di sviluppo

### Prerequisiti

- Rust toolchain (stable) via [rustup](https://rustup.rs/)
- Homebrew for Linux installato
- Sistema Debian/Ubuntu (o derivata) per testare con APT reale

### Primo setup

```bash
git clone https://github.com/fedro86/apt2brew.git
cd apt2brew
cargo build
cargo test
```

### Comandi utili

```bash
cargo build              # Build debug
cargo build --release    # Build release
cargo test               # Esegui test suite
cargo clippy             # Linting
cargo fmt                # Formattazione codice
cargo deb                # Genera pacchetto .deb (richiede cargo-deb)
```

## Coding Standards

### Stile

- **Formattazione**: `cargo fmt` (rustfmt defaults)
- **Linting**: `cargo clippy` con zero warning
- **Nessun `unsafe`**
- **Nessun `.unwrap()` in produzione** — usa `?` operator con errori tipizzati via `thiserror`

### Architettura

Il progetto segue domain-driven design con 4 layer:

| Layer              | Può dipendere da          | NON può dipendere da      |
|--------------------|---------------------------|---------------------------|
| `domain/`          | Nulla (pura logica)       | infrastructure, presentation |
| `application/`     | domain                    | infrastructure (solo via trait), presentation |
| `infrastructure/`  | domain, crate esterni     | presentation              |
| `presentation/`    | domain, application       | infrastructure direttamente |

### Commit

- Messaggi in inglese, concisi, al presente ("add scan command", non "added scan command")
- Un commit per modifica logica

### Test

- Ogni feature ha almeno un test
- Unit test in `#[cfg(test)] mod tests` nello stesso file
- Integration test in `tests/`
- Fixture in `tests/fixtures/`

## Branching

- `main` — branch stabile, sempre buildabile
- `feature/<nome>` — feature in sviluppo
- `fix/<nome>` — bug fix

## Pull Request

- Titolo breve e descrittivo
- Descrizione con contesto e test plan
- `cargo test` e `cargo clippy` devono passare
