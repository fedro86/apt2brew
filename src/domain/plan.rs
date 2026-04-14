use super::package::PackageMigration;

/// A migration plan built from user selections.
#[derive(Debug)]
pub struct MigrationPlan {
    pub packages: Vec<PackageMigration>,
    pub dry_run: bool,
}

impl MigrationPlan {
    pub fn new(packages: Vec<PackageMigration>, dry_run: bool) -> Self {
        Self { packages, dry_run }
    }

    /// Packages the user has selected for migration.
    pub fn selected(&self) -> Vec<&PackageMigration> {
        self.packages.iter().filter(|p| p.is_selected).collect()
    }

    /// Packages with a brew match but not selected.
    pub fn skipped(&self) -> Vec<&PackageMigration> {
        self.packages
            .iter()
            .filter(|p| !p.is_selected && p.brew_name.is_some())
            .collect()
    }

    /// Packages with no brew equivalent found.
    pub fn unmatched(&self) -> Vec<&PackageMigration> {
        self.packages
            .iter()
            .filter(|p| p.brew_name.is_none())
            .collect()
    }
}
