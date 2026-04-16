# apt2brew

> [!CAUTION]
> This tool is in **early development**. It modifies system packages and can break your setup if something goes wrong. Always review the selection before confirming, and keep the generated rollback scripts. Test on a non-critical machine first.

Migrate your manually installed APT and snap packages to [Homebrew](https://brew.sh/) on Linux. Keep your system package manager clean, manage user-space tools through brew.

> [!NOTE]
> **Help us expand package coverage!** The tool matches APT/snap names to Homebrew formulae using alias files. Many packages have different names across package managers (e.g. `fd-find` in APT is `fd` in brew). The more aliases we have, the more packages can be migrated automatically. See [Contributing aliases](#contributing-aliases) below -- it's just adding a line to a JSON file.

## Why

APT packages are system-wide and require `sudo`. Homebrew installs to user-space (`/home/linuxbrew/`), needs no root, and updates independently from the OS. `apt2brew` automates the migration: it scans what you have, finds the Homebrew equivalent, installs it, verifies it works, and only then removes the original.

## Install

### From source

```bash
cargo install --path .
```

### As .deb package (recommended — auto-updates via apt)

Add the apt2brew APT repository so `apt update` will pick up future releases:

```bash
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://fedro86.github.io/apt2brew/apt2brew.gpg.key \
  | sudo gpg --dearmor -o /etc/apt/keyrings/apt2brew.gpg
echo "deb [signed-by=/etc/apt/keyrings/apt2brew.gpg] https://fedro86.github.io/apt2brew stable main" \
  | sudo tee /etc/apt/sources.list.d/apt2brew.list
sudo apt update
sudo apt install apt2brew
```

To uninstall: `sudo apt remove apt2brew && sudo rm /etc/apt/sources.list.d/apt2brew.list /etc/apt/keyrings/apt2brew.gpg`.

### As .deb package (build locally)

```bash
cargo deb
sudo dpkg -i target/debian/apt2brew_*.deb
```

## Prerequisites

- Linux with APT (Debian/Ubuntu)
- [Homebrew for Linux](https://brew.sh/) installed and in your PATH

## Usage

### Scan (read-only)

Analyze installed packages and show what can be migrated:

```bash
apt2brew scan
```

Outputs a table with each package, its brew equivalent (if found), and a risk classification.

### Migrate

Interactive TUI to select and migrate packages:

```bash
apt2brew migrate
```

- Browse packages with `j/k`, toggle with `Space`, filter with `Tab`, search with `/`
- Press `Enter` to confirm selection and start migration
- The tool installs via brew, verifies the installation, then removes the APT/snap original
- A rollback script is generated before any removal

Non-interactive modes:

```bash
apt2brew migrate --dry-run    # Show what would happen
apt2brew migrate --yes        # Execute with pre-selected packages
```

### Rollback

Undo a migration -- reinstall packages via APT/snap and remove from brew:

```bash
apt2brew rollback
```

- Main screen shows all brew packages with their APT counterpart
- Press `s` to view rollback scripts in a modal
- Select packages and press `Enter` to rollback
- Each package is reinstalled individually so one failure doesn't block the rest

Non-interactive:

```bash
apt2brew rollback --yes                # Rollback latest script
apt2brew rollback --package <name>     # Rollback a single package
```

## How it works

### Risk engine

Packages are classified as **Low** (safe to migrate) or **High** (should stay in APT) using 8 heuristic rules:

1. Safety-net list (boot/init/coreutils -- cannot be detected by file analysis)
2. Essential/required priority (`Priority: required` in dpkg)
3. Systemd units or init.d scripts
4. Binaries in `/sbin` or `/usr/sbin`
5. Libraries (`lib*` prefix)
6. High reverse dependency count (5+)
7. Depended upon by essential packages
8. System-level dpkg section (kernel, base, libs, drivers)

Low-risk packages with a brew match are pre-selected. High-risk packages are deselected and shown with the reason.

### Package matching

APT/snap names are matched to brew formulae through:

1. **Explicit aliases** (`aliases/apt-to-brew.json`, `aliases/snap-to-brew.json`) -- curated mappings
2. **Exact name match** in the Homebrew formula index
3. **Brew aliases** (e.g. `nodejs` is an alias for `node` in Homebrew)
4. **Fuzzy matching** -- strips common APT suffixes (`-dev`, `-bin`, `-tools`), `lib` prefix, `python3-` prefix, version suffixes (`python3` -> `python@3`)

### Safety guarantees

- No APT/snap removal without a verified brew installation (`brew list <formula>`)
- Rollback script reserved before any system modification, then rewritten after every successful brew install -- a Ctrl-C or panic mid-migration leaves an on-disk rollback that matches actual brew state
- `apt remove` is simulated first (`apt-get -s remove`); the migration aborts and reports the extras if removal would cascade beyond the packages you selected (e.g. picking a library other packages depend on)
- Pre-existing brew copies are detected -- the install step is skipped and the result is annotated `(brew copy was pre-existing -- not refreshed)` so you know your brew copy wasn't rebuilt
- All package names are validated against a strict character set before reaching `brew`/`apt`/`sudo` argv, with `--` end-of-options markers at every destructive call site
- Snap packages are correctly separated from APT for both removal and rollback
- Sudo is requested only when needed (just before removal), not at startup

## Project structure

```
src/
  domain/         Pure business logic (risk engine, data types)
  application/    Use case orchestration (scan, migrate, rollback)
  infrastructure/ System integration (dpkg, brew API, subprocess)
  presentation/   CLI (clap) + TUI (ratatui)
aliases/          Curated package name mappings (JSON)
```

## Contributing aliases

The alias files are the key to matching packages across package managers. Without an alias, the tool relies on fuzzy matching (stripping `-dev`, `-bin`, `lib` prefixes, etc.), which doesn't always work.

**How to add an alias:**

1. Edit `aliases/apt-to-brew.json` (for APT packages) or `aliases/snap-to-brew.json` (for snap packages)
2. Add a line mapping the source name to the brew formula name:

```json
"fd-find": { "brew": "fd", "type": "formula" }
```

3. Open a PR

**How to find the brew name:** run `brew search <keyword>` or check [formulae.brew.sh](https://formulae.brew.sh/).

Every alias you add helps everyone who runs `apt2brew` on their machine.

## License

MIT
