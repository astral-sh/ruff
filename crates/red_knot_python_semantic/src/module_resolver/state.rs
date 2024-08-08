use ruff_db::program::TargetVersion;
use ruff_db::vendored::VendoredFileSystem;

use super::typeshed::LazyTypeshedVersions;
use crate::db::Db;

pub(crate) struct ResolverState<'db> {
    pub(crate) db: &'db dyn Db,
    pub(crate) typeshed_versions: LazyTypeshedVersions<'db>,
    pub(crate) target_version: TargetVersion,
}

impl<'db> ResolverState<'db> {
    pub(crate) fn new(db: &'db dyn Db, target_version: TargetVersion) -> Self {
        Self {
            db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version,
        }
    }

    pub(crate) fn vendored(&self) -> &VendoredFileSystem {
        self.db.vendored()
    }
}
