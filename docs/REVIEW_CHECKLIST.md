# Code Review Checklist — apt2brew

## Safety (Highest Priority)

- [ ] No `essential` or `required` package is touched
- [ ] APT removal happens ONLY after verifying the brew binary is in PATH
- [ ] Dry-run is the default; `--execute` is explicitly required
- [ ] A rollback script is generated BEFORE any modification
- [ ] System commands (`apt remove`, `brew install`) don't use `--force` or dangerous flags
- [ ] No hardcoded `sudo` — APT removal requires privileges and the user is informed

## Correctness

- [ ] Domain types (`RiskLevel`, `PackageMigration`) are used correctly
- [ ] APT → Brew matches are verified (not just by name but also by functionality)
- [ ] Network errors (Homebrew API) are handled with retry or graceful fallback
- [ ] dpkg database parsing handles edge cases (removed but not purged packages, virtual packages)

## Rust Quality

- [ ] `cargo clippy` passes with no warnings
- [ ] `cargo test` green
- [ ] Error handling with `thiserror` and `?` operator, no `.unwrap()` in production
- [ ] No `unsafe`
- [ ] Public types have documentation (`///`)

## UX

- [ ] Error messages are helpful and suggest an action
- [ ] The TUI is navigable using keyboard only
- [ ] The pre-execution summary is clear and complete
- [ ] `--help` is informative for each subcommand
- [ ] Non-interactive output (piped) is parsable (no colors, no TUI)

## Distribution

- [ ] `cargo-deb` generates a valid .deb
- [ ] The binary installs to `/usr/bin/apt2brew`
- [ ] Shell completions and man page are included in the .deb
- [ ] The version in `Cargo.toml` is up to date
