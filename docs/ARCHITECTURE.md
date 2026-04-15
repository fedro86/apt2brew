# Architecture — apt2brew

## Problem Statement

On Linux, package management is traditionally delegated to APT with root privileges.
However, most modern CLI tools (git, neovim, bat, eza, fd, ripgrep...) don't require system privileges and can be managed in user-space via Homebrew.

Today there is no tool that automates this migration in a safe and intelligent way.

## Solution

**apt2brew** is a static Rust binary that:
1. Analyzes manually installed APT packages
2. Compares them against the Homebrew catalog
3. Classifies migration risk (user-space vs system-space)
4. Presents an interactive TUI for selection
5. Executes the migration with integrity verification

## Architectural Decisions

### ADR-001: Language — Rust
- **Context**: The tool manipulates the system package manager. Errors can compromise the environment.
- **Decision**: Rust for compile-time safety, static binary, zero runtime dependencies.
- **Consequences**: Simplified distribution (.deb with a single binary), steeper learning curve.

### ADR-002: Pipeline Architecture
- **Context**: The workflow is inherently sequential (scan → match → classify → select → execute).
- **Decision**: Pipeline with discrete stages, each with typed input/output.
- **Consequences**: Each stage is independently testable; future possibility to run only subsets (e.g., `apt2brew scan` without execution).

### ADR-003: TUI with ratatui
- **Context**: The user must be able to select/deselect packages before migration.
- **Decision**: TUI interface with ratatui for an interactive checklist.
- **Consequences**: Rich terminal user experience; `--yes` fallback for non-interactive environments.

### ADR-004: Primary Distribution via APT
- **Context**: It would be paradoxical to require brew to install a tool that migrates from apt to brew.
- **Decision**: Primary distribution as a `.deb` package, secondary via `cargo install`.
- **Consequences**: Build system must integrate `cargo-deb`; binary in `/usr/bin/`, man page and shell completions in standard paths.

### ADR-005: Reversible Operations
- **Context**: Removing system packages is a destructive operation.
- **Decision**: Dry-run by default; generation of a Brewfile and a rollback script before any modification.
- **Consequences**: The user can always return to the previous state; every run produces recovery artifacts.

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

| Crate       | Purpose                                    |
|-------------|--------------------------------------------|
| `clap`      | CLI argument parsing and subcommands       |
| `ratatui`   | Interactive TUI interface                  |
| `crossterm` | Terminal backend for ratatui               |
| `tokio`     | Async runtime for parallel API calls       |
| `reqwest`   | HTTP client for Homebrew API               |
| `serde`     | JSON serialization/deserialization         |
| `which`     | Binary location detection in PATH          |
| `thiserror` | Typed error handling                       |
| `cargo-deb` | .deb package generation (build tool)       |

## System Boundaries

### Input
- Local dpkg database (`/var/lib/dpkg/status`)
- Homebrew API (`https://formulae.brew.sh/api/formula.json`)
- User configuration (optional TOML file)

### Output
- List of migratable packages (stdout / TUI)
- Generated Brewfile (`~/Brewfile`)
- Rollback script (`~/.apt2brew/rollback-<timestamp>.sh`)
- Operation logs (`~/.apt2brew/logs/`)

### Constraints
- Target: Linux with APT (Debian/Ubuntu and derivatives)
- Requires Homebrew already installed for the migration phase
- Does not touch `essential` or `required` dpkg packages
- Does not remove APT packages without explicit confirmation
