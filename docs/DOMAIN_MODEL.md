# Domain Model — apt2brew

## Core Entities

### PackageMigration

Rappresenta un singolo pacchetto candidato alla migrazione.

```
PackageMigration
├── name: String              # Nome pacchetto APT (es. "git", "neovim")
├── apt_version: String       # Versione installata via APT
├── brew_name: Option<String> # Nome corrispondente su Homebrew (None se non trovato)
├── brew_version: Option<String> # Versione disponibile su Homebrew
├── risk: RiskLevel           # Classificazione di rischio
├── is_selected: bool         # Selezionato per la migrazione (default da RiskLevel)
└── source: PackageSource     # Come è stato installato (manual, dependency, auto)
```

### RiskLevel

Classificazione binaria del rischio di migrazione.

```
RiskLevel
├── Low   — User-space: tool CLI, runtime, librerie di sviluppo
│           Esempi: git, neovim, python3, htop, bat, eza, fd-find
│           → Pre-selezionato per la migrazione
│
└── High  — System-space: daemon, driver, networking, kernel-related
            Esempi: docker-ce, nvidia-driver, postgresql, openssh-server, ufw
            → Deselezionato, richiede conferma esplicita
```

### MigrationPlan

Raccolta delle decisioni prese dall'utente prima dell'esecuzione.

```
MigrationPlan
├── packages: Vec<PackageMigration>   # Tutti i pacchetti analizzati
├── selected: Vec<&PackageMigration>  # Solo quelli marcati is_selected
├── timestamp: DateTime               # Quando è stato generato il piano
└── dry_run: bool                     # Se true, nessuna modifica al sistema
```

### MigrationResult

Esito dell'esecuzione per singolo pacchetto.

```
MigrationResult
├── package: String
├── brew_installed: bool
├── apt_removed: bool
├── path_verified: bool        # Il binario brew è prioritario nel $PATH
└── error: Option<String>
```

## Regole di classificazione (Risk Engine)

Il Risk Engine applica euristiche in cascata:

```
1. PACKAGE è in SYSTEM_CRITICAL_LIST?      → High
   (lista hardcoded: systemd, grub, linux-*, network-manager, ufw, iptables...)

2. PACKAGE ha file in /etc/init.d o unit systemd? → High
   (indica un daemon di sistema)

3. PACKAGE ha dipendenze inverse da pacchetti essential? → High

4. PACKAGE è nella sezione "libs" o "kernel"?  → High

5. Default                                      → Low
```

## Flusso dati

```
dpkg status DB ──▶ AptScanner ──▶ Vec<AptPackage>
                                        │
                                        ▼
Homebrew API ────▶ BrewMatcher ──▶ Vec<PackageMigration> (con brew_name popolato)
                                        │
                                        ▼
                   RiskEngine  ──▶ Vec<PackageMigration> (con risk classificato)
                                        │
                                        ▼
                   TUI         ──▶ Vec<PackageMigration> (con is_selected aggiornato)
                                        │
                                        ▼
                   MigrationPlan ──▶ Migrator ──▶ Vec<MigrationResult>
                                                       │
                                                       ▼
                                                  Brewfile + Rollback Script
```

## Invarianti

1. **Nessun pacchetto `essential` viene mai proposto per la migrazione**
2. **Nessuna rimozione APT senza installazione brew verificata** — il binario deve essere raggiungibile nel PATH prima di rimuovere la versione APT
3. **Dry-run è il comportamento default** — la migrazione effettiva richiede `--execute`
4. **Ogni migrazione produce un rollback script** — prima di qualsiasi modifica
5. **I pacchetti High-risk non vengono mai pre-selezionati** — l'utente deve selezionarli esplicitamente
