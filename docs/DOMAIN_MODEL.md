# Domain Model — apt2brew

## Core Entities

### PackageMigration

Represents a single package candidate for migration.

```
PackageMigration
├── name: String              # APT package name (e.g., "git", "neovim")
├── apt_version: String       # Version installed via APT
├── brew_name: Option<String> # Corresponding Homebrew name (None if not found)
├── brew_version: Option<String> # Version available on Homebrew
├── risk: RiskLevel           # Risk classification
├── is_selected: bool         # Selected for migration (default based on RiskLevel)
└── source: PackageSource     # How it was installed (manual, dependency, auto)
```

### RiskLevel

Binary risk classification for migration.

```
RiskLevel
├── Low   — User-space: CLI tools, runtimes, development libraries
│           Examples: git, neovim, python3, htop, bat, eza, fd-find
│           → Pre-selected for migration
│
└── High  — System-space: daemons, drivers, networking, kernel-related
            Examples: docker-ce, nvidia-driver, postgresql, openssh-server, ufw
            → Deselected, requires explicit confirmation
```

### MigrationPlan

Collection of decisions made by the user before execution.

```
MigrationPlan
├── packages: Vec<PackageMigration>   # All analyzed packages
├── selected: Vec<&PackageMigration>  # Only those marked is_selected
├── timestamp: DateTime               # When the plan was generated
└── dry_run: bool                     # If true, no system modifications
```

### MigrationResult

Execution outcome for a single package.

```
MigrationResult
├── package: String
├── brew_installed: bool
├── apt_removed: bool
├── path_verified: bool        # The brew binary takes priority in $PATH
└── error: Option<String>
```

## Classification Rules (Risk Engine)

The Risk Engine applies heuristics in cascade:

```
1. Is PACKAGE in SYSTEM_CRITICAL_LIST?          → High
   (hardcoded list: systemd, grub, linux-*, network-manager, ufw, iptables...)

2. Does PACKAGE have files in /etc/init.d or systemd units? → High
   (indicates a system daemon)

3. Does PACKAGE have reverse dependencies from essential packages? → High

4. Is PACKAGE in the "libs" or "kernel" section? → High

5. Default                                       → Low
```

## Data Flow

```
dpkg status DB ──▶ AptScanner ──▶ Vec<AptPackage>
                                        │
                                        ▼
Homebrew API ────▶ BrewMatcher ──▶ Vec<PackageMigration> (with brew_name populated)
                                        │
                                        ▼
                   RiskEngine  ──▶ Vec<PackageMigration> (with risk classified)
                                        │
                                        ▼
                   TUI         ──▶ Vec<PackageMigration> (with is_selected updated)
                                        │
                                        ▼
                   MigrationPlan ──▶ Migrator ──▶ Vec<MigrationResult>
                                                       │
                                                       ▼
                                                  Brewfile + Rollback Script
```

## Invariants

1. **No `essential` package is ever proposed for migration**
2. **No APT removal without verified brew installation** — the binary must be reachable in PATH before removing the APT version
3. **Dry-run is the default behavior** — actual migration requires `--execute`
4. **Every migration produces a rollback script** — before any modification
5. **High-risk packages are never pre-selected** — the user must select them explicitly
