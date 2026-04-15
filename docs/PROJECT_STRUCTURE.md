# Project Structure — apt2brew

```
apt2brew/
├── Cargo.toml                 # Manifest with cargo-deb metadata
├── Cargo.lock
├── README.md
├── LICENSE
├── CONTRIBUTING.md
│
├── src/
│   ├── main.rs                # Entry point, CLI setup with clap
│   │
│   ├── domain/                # Core business logic, zero external dependencies
│   │   ├── mod.rs
│   │   ├── package.rs         # PackageMigration struct, RiskLevel enum
│   │   ├── risk.rs            # Risk classification rules
│   │   └── plan.rs            # MigrationPlan: collection of pre-execution decisions
│   │
│   ├── application/           # Use case orchestration
│   │   ├── mod.rs
│   │   ├── scan.rs            # Use case: package scanning
│   │   ├── migrate.rs         # Use case: migration execution
│   │   └── rollback.rs        # Use case: restoring previous state
│   │
│   ├── infrastructure/        # External system integrations
│   │   ├── mod.rs
│   │   ├── apt.rs             # dpkg/apt database reading
│   │   ├── brew.rs            # Homebrew API client + brew command execution
│   │   ├── config.rs          # TOML configuration reading
│   │   └── filesystem.rs      # Brewfile, rollback script, log writing
│   │
│   └── presentation/          # Presentation layer
│       ├── mod.rs
│       ├── cli.rs             # clap command definitions
│       └── tui/               # ratatui interface
│           ├── mod.rs
│           ├── app.rs         # TUI application state
│           ├── render.rs      # Checklist and summary rendering
│           └── input.rs       # Keyboard input handling
│
├── tests/                     # Integration tests
│   ├── scan_test.rs
│   ├── matcher_test.rs
│   └── fixtures/              # Test data (mock dpkg status, mock API response)
│       ├── dpkg_status_sample
│       └── brew_api_sample.json
│
├── debian/                    # .deb package metadata (cargo-deb)
│   └── postinst               # Post-installation script (optional)
│
├── completions/               # Shell completions (generated at build-time)
│   ├── apt2brew.bash
│   ├── apt2brew.zsh
│   └── apt2brew.fish
│
├── man/                       # Man pages
│   └── apt2brew.1
│
└── docs/                      # Project documentation
    ├── ARCHITECTURE.md
    ├── PROJECT_STRUCTURE.md
    ├── DOMAIN_MODEL.md
    ├── ROADMAP.md
    ├── REVIEW_CHECKLIST.md
    └── temp/                  # Scratch pad (gitignored)
```

## Layer Responsibilities

### `domain/`
Contains **only** pure business logic. No external dependencies (no I/O crates, no network).
Defines core types (`PackageMigration`, `RiskLevel`, `MigrationPlan`) and classification rules.
Everything in this layer is testable with pure unit tests, no mocks needed.

### `application/`
Use case orchestrators. They receive trait objects from infrastructure and coordinate the flow:
scan → match → classify → plan. They contain no business logic nor I/O details.

### `infrastructure/`
Concrete implementations of interfaces defined in domain.
This is where dpkg database parsing, Homebrew API HTTP client, and file read/write live.

### `presentation/`
Everything related to user interaction: CLI argument parsing, TUI rendering,
input handling. No business logic.

### `debian/`
Metadata needed for `.deb` package generation via `cargo-deb`.
Includes optional post-installation scripts.

### `completions/` and `man/`
Generated at build-time (via `clap_complete` and `clap_mangen`).
Installed to standard paths by the .deb package.
