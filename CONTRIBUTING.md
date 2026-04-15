# Contributing to apt2brew

## Development Setup

### Prerequisites

- Rust toolchain (stable) via [rustup](https://rustup.rs/)
- Homebrew for Linux installed
- Debian/Ubuntu system (or derivative) to test with real APT

### First-time setup

```bash
git clone https://github.com/fedro86/apt2brew.git
cd apt2brew
cargo build
cargo test
```

### Useful commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run test suite
cargo clippy             # Linting
cargo fmt                # Code formatting
cargo deb                # Generate .deb package (requires cargo-deb)
```

## Coding Standards

### Style

- **Formatting**: `cargo fmt` (rustfmt defaults)
- **Linting**: `cargo clippy` with zero warnings
- **No `unsafe`**
- **No `.unwrap()` in production** — use `?` operator with typed errors via `thiserror`

### Architecture

The project follows domain-driven design with 4 layers:

| Layer              | May depend on             | Must NOT depend on          |
|--------------------|---------------------------|-----------------------------|
| `domain/`          | Nothing (pure logic)      | infrastructure, presentation |
| `application/`     | domain                    | infrastructure (only via trait), presentation |
| `infrastructure/`  | domain, external crates   | presentation                |
| `presentation/`    | domain, application       | infrastructure directly     |

### Commits

- Messages in English, concise, present tense ("add scan command", not "added scan command")
- One commit per logical change

### Tests

- Every feature has at least one test
- Unit tests in `#[cfg(test)] mod tests` in the same file
- Integration tests in `tests/`
- Fixtures in `tests/fixtures/`

## Branching

- `main` — stable branch, always buildable
- `feature/<name>` — feature development
- `fix/<name>` — bug fixes

## Pull Requests

- Short, descriptive title
- Description with context and test plan
- `cargo test` and `cargo clippy` must pass
