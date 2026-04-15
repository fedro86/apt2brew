# Roadmap — apt2brew

## Phase 1 — Foundation (MVP)

Goal: working scan with terminal output.

- [x] Rust project setup with Cargo.toml (cargo-deb metadata included)
- [x] Implement AptScanner: parsing `/var/lib/dpkg/status`
- [x] Filter only manually installed packages (`apt-mark showmanual`)
- [x] Implement BrewMatcher: fetch and local cache of `formulae.brew.sh/api/formula.json`
- [x] APT name → Brew name matching (exact match + known aliases)
- [x] Tabular stdout output with match status
- [x] Tests with dpkg fixtures and mock API

## Phase 2 — Intelligence

Goal: automatic risk classification.

- [x] Heuristic Risk Engine (8 cascade rules: systemd, sbin, /etc/, lib*, reverse deps, essential deps, dpkg sections)
- [x] Minimal safety-net (~30 packages undetectable by heuristics: coreutils, POSIX, crypto, toolchain)
- [x] Daemon detection (systemd units, init.d scripts via `dpkg -L`)
- [x] Installed file analysis (sbin, /etc/ config) via `dpkg -L`
- [x] Reverse dependency analysis from essential/required packages
- [x] Reverse dependency count (threshold >= 5 → High)
- [x] Automatic pre-selection based on RiskLevel
- [x] `apt2brew scan` subcommand with formatted output (migratable / non-migratable / no match)
- [x] Brew Version column for APT vs Brew version comparison
- [x] Human-readable reason for each risk classification

## Phase 3 — TUI

Goal: interactive interface for selection.

- [x] Implement TUI with ratatui + crossterm
- [x] Navigable checklist with arrows/j/k and space to toggle
- [x] Risk filter (Tab: All / Migratable / High Risk / No Match)
- [x] Package name search (/ to activate)
- [x] Select all (a) / deselect all (n)
- [x] Pre-confirmation summary with overlay ("N packages selected, confirm?")
- [x] Non-interactive fallback with `--yes` for CI/scripting

## Phase 4 — Migration Engine

Goal: safe migration execution.

- [x] MigrationPlan generation from user selection (TUI or --yes)
- [x] `--dry-run` for non-interactive preview mode
- [x] Interactive workflow: TUI selection → confirmation → immediate execution
- [x] TUI with live progress bar during brew install
- [x] Installation verification via `brew list` (not PATH — binary names differ from formula names)
- [x] Batch APT removal with single `sudo apt remove -y` (one password prompt)
- [x] Batch Snap removal with `sudo snap remove`
- [x] Support for `brew install --cask` for GUI applications
- [x] Generated `~/Brewfile` with migrated packages
- [x] Rollback script generation (`~/.apt2brew/rollback-<timestamp>.sh`)
- [x] Operation logging in `~/.apt2brew/logs/`
- [x] Rollback script pre-generated BEFORE any modification

## Phase 5 — Rollback & Safety

Goal: ability to undo the migration.

- [x] `apt2brew rollback` subcommand with TUI: script selection → package selection → progress bar
- [x] Selective rollback: `--package <name>` for a single package
- [x] Interactive confirmation before execution (skip with `--yes`)
- [x] Batch `brew uninstall` + single `sudo apt install -y` (one password prompt)
- [x] Rollback script parsing to extract entries
- [x] Tests for the rollback script parser
- [x] Homebrew prerequisite check with shell-specific instructions (bash/zsh/fish) for PATH setup

## Phase 6 — Polish & Distribution

Goal: ready for public distribution.

- [ ] Shell completions generated at build-time (bash, zsh, fish) via `clap_complete`
- [ ] Man page generated via `clap_mangen`
- [ ] `.deb` package via `cargo-deb`
- [ ] CI/CD: build + test + .deb generation on GitHub Actions
- [ ] GitHub release with binary and .deb attached
- [ ] Self-hosted APT repository on GitHub Pages (GPG-signed, auto-published via CI)
- [ ] Publish on crates.io as secondary channel (`cargo install apt2brew`)
- [ ] Complete README with demo GIF, installation instructions, examples

## Phase 7 — Matching & Sources (completed)

- [x] Fuzzy matching APT → Brew names (strip -dev/-bin/-utils suffixes, lib prefix, python3-, vendor prefix, version suffix)
- [x] Cask support (parallel fetch of formulae + cask API, `brew install --cask`)
- [x] Cask blocklist for false positives (e.g., `dash` APT ≠ Dash macOS)
- [x] Snap package scanning (`snap list`, system snap filtering)
- [x] Externalized aliases in JSON files (`aliases/apt-to-brew.json`, `snap-to-brew.json`, `blocklist.json`) — PR-friendly
- [x] Snap-specific aliases (e.g., `code` → `visual-studio-code`, `astral-uv` → `uv`)

## Phase 8 — Future (Post-release)

- [ ] User configuration via TOML file (`~/.config/apt2brew/config.toml`)
- [ ] Plugin system for custom risk rules
- [ ] Distribution PPA for apt (`sudo add-apt-repository ppa:...`)
- [ ] Flatpak support as an additional source
- [ ] Local Homebrew API cache (avoid re-fetch on every scan)
