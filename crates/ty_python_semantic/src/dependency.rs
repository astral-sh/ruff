//! Direct dependencies and module ownership supplied by a package manager.

use std::collections::{BTreeMap, BTreeSet};

use compact_str::CompactString;
use ruff_db::system::SystemPathBuf;
use ty_module_resolver::ModuleName;

/// The dependency information needed to check imports, without source ranges or lockfile details.
///
/// Distribution keys are opaque package-manager IDs. Keeping those IDs avoids conflating packages
/// with the same name but different sources, and separates package identity from its display name.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyMetadata {
    pub projects: Box<[DependencyProject]>,
    pub distributions: BTreeMap<CompactString, DependencyDistribution>,
    pub module_owners: BTreeMap<ModuleName, Box<[CompactString]>>,
}

/// The direct dependency declarations of a workspace member or a virtual workspace root.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyProject {
    pub path: SystemPathBuf,
    pub distribution: Option<CompactString>,
    pub dependencies: BTreeSet<CompactString>,
    pub group_dependencies: BTreeSet<CompactString>,
}

/// A distribution's display name and, for editable installs, its source directory.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyDistribution {
    pub name: CompactString,
    pub editable_path: Option<SystemPathBuf>,
}
