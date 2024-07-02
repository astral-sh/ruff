#![allow(clippy::used_underscore_binding)] // necessary for Salsa inputs
#![allow(unreachable_pub)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::clone_on_copy)]

use crate::Db;

// TODO: unify with the PythonVersion enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SupportedPyVersion {
    Py37,
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

#[salsa::input(singleton)]
pub(crate) struct TargetPyVersion {
    pub(crate) target_py_version: SupportedPyVersion,
}

pub(crate) fn set_target_py_version(db: &mut dyn Db, target_version: SupportedPyVersion) {
    if let Some(existing) = TargetPyVersion::try_get(db) {
        existing.set_target_py_version(db).to(target_version);
    } else {
        TargetPyVersion::new(db, target_version);
    }
}

pub(crate) fn get_target_py_version(db: &dyn Db) -> SupportedPyVersion {
    TargetPyVersion::get(db).target_py_version(db)
}
