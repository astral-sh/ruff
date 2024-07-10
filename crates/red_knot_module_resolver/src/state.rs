use ruff_db::system::System;
use ruff_db::vendored::VendoredFileSystem;

use crate::db::Db;
use crate::supported_py_version::TargetVersion;
use crate::typeshed::LazyTypeshedVersions;

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

    pub(crate) fn system(&self) -> &dyn System {
        self.db.system()
    }

    pub(crate) fn vendored(&self) -> &VendoredFileSystem {
        self.db.vendored()
    }
}
