# Architecture — apt2brew

## Problem Statement

Su Linux, la gestione dei pacchetti è tradizionalmente delegata ad APT con privilegi root.
Tuttavia, la maggior parte dei tool CLI moderni (git, neovim, bat, eza, fd, ripgrep...) non richiede privilegi di sistema e può essere gestita in user-space tramite Homebrew.

Oggi non esiste uno strumento che automatizzi questa migrazione in modo sicuro e intelligente.

## Solution

**apt2brew** è un binario statico Rust che:
1. Analizza i pacchetti APT installati manualmente
2. Li confronta con il catalogo Homebrew
3. Classifica il rischio di migrazione (user-space vs system-space)
4. Presenta una TUI interattiva per la selezione
5. Esegue la migrazione con verifica di integrità

## Architectural Decisions

### ADR-001: Linguaggio — Rust
- **Contesto**: Il tool manipola il package manager di sistema. Errori possono compromettere l'ambiente.
- **Decisione**: Rust per safety a compile-time, binario statico, zero dipendenze runtime.
- **Conseguenze**: Distribuzione semplificata (.deb con singolo binario), curva di apprendimento più ripida.

### ADR-002: Architettura a Pipeline
- **Contesto**: Il workflow è intrinsecamente sequenziale (scan → match → classify → select → execute).
- **Decisione**: Pipeline con fasi discrete, ciascuna con input/output tipizzati.
- **Conseguenze**: Ogni fase è testabile isolatamente; possibilità futura di eseguire solo sottoinsiemi (es. `apt2brew scan` senza esecuzione).

### ADR-003: TUI con ratatui
- **Contesto**: L'utente deve poter selezionare/deselezionare pacchetti prima della migrazione.
- **Decisione**: Interfaccia TUI con ratatui per checklist interattiva.
- **Conseguenze**: Esperienza utente ricca nel terminale; fallback `--yes` per ambienti non-interattivi.

### ADR-004: Distribuzione primaria via APT
- **Contesto**: Sarebbe paradossale dover installare brew per installare un tool che migra da apt a brew.
- **Decisione**: Distribuzione primaria come pacchetto `.deb`, secondaria via `cargo install`.
- **Conseguenze**: Build system deve integrare `cargo-deb`; binario in `/usr/bin/`, man page e shell completions nei path standard.

### ADR-005: Operazioni reversibili
- **Contesto**: Rimuovere pacchetti di sistema è un'operazione distruttiva.
- **Decisione**: Dry-run di default; generazione di un Brewfile e di uno script di rollback prima di qualsiasi modifica.
- **Conseguenze**: L'utente può sempre tornare allo stato precedente; ogni run produce artefatti di recovery.

## Component Architecture

```
┌─────────────────────────────────────────────────┐
│                     CLI (clap)                   │
│              apt2brew scan|migrate|rollback      │
├─────────────────────────────────────────────────┤
│                                                  │
│  ┌──────────┐  ┌─────────────┐  ┌────────────┐ │
│  │   Apt     │  │    Brew     │  │   Risk     │ │
│  │  Scanner  │──▶   Matcher   │──▶  Engine    │ │
│  └──────────┘  └─────────────┘  └─────┬──────┘ │
│                                        │        │
│                                  ┌─────▼──────┐ │
│                                  │    TUI     │ │
│                                  │  Renderer  │ │
│                                  └─────┬──────┘ │
│                                        │        │
│                                  ┌─────▼──────┐ │
│                                  │  Migrator  │ │
│                                  │  (executor)│ │
│                                  └────────────┘ │
│                                                  │
├─────────────────────────────────────────────────┤
│              Cross-cutting concerns              │
│         Logging · Config · Error Handling        │
└─────────────────────────────────────────────────┘
```

## Technology Stack

| Crate       | Scopo                                      |
|-------------|--------------------------------------------|
| `clap`      | Parsing argomenti CLI e subcomandi         |
| `ratatui`   | Interfaccia TUI interattiva                |
| `crossterm` | Backend terminale per ratatui              |
| `tokio`     | Runtime async per chiamate API parallele   |
| `reqwest`   | Client HTTP per API Homebrew               |
| `serde`     | Serializzazione/deserializzazione JSON     |
| `which`     | Rilevamento posizione binari nel PATH      |
| `thiserror` | Gestione errori tipizzata                  |
| `cargo-deb` | Generazione pacchetto .deb (build tool)    |

## System Boundaries

### Input
- Database dpkg locale (`/var/lib/dpkg/status`)
- API Homebrew (`https://formulae.brew.sh/api/formula.json`)
- Configurazione utente (file TOML opzionale)

### Output
- Lista pacchetti migrabili (stdout / TUI)
- Brewfile generato (`~/Brewfile`)
- Script di rollback (`~/.apt2brew/rollback-<timestamp>.sh`)
- Log delle operazioni (`~/.apt2brew/logs/`)

### Constraints
- Target: Linux con APT (Debian/Ubuntu e derivate)
- Richiede Homebrew già installato per la fase di migrazione
- Non tocca pacchetti `essential` o `required` di dpkg
- Non rimuove pacchetti APT senza conferma esplicita
