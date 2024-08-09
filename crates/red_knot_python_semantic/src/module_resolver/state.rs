use ruff_db::vendored::VendoredFileSystem;

use crate::db::Db;
use crate::TargetVersion;

pub(crate) struct ResolverContext<'db> {
    pub(crate) db: &'db dyn Db,
    pub(crate) target_version: TargetVersion,
}

impl<'db> ResolverContext<'db> {
    pub(crate) fn new(db: &'db dyn Db, target_version: TargetVersion) -> Self {
        Self { db, target_version }
    }

    pub(crate) fn vendored(&self) -> &VendoredFileSystem {
        self.db.vendored()
    }
}
