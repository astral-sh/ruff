use ty_module_resolver::file_to_module;

use crate::Db;
use crate::types::ClassSpecialization;

/// Whether an alias specializes a scalar base used by older NumPy stubs.
///
/// Those stubs describe runtime classes such as `int64` as `signedinteger[_64Bit]`.
/// Allow these aliases where a class is required, without changing their representation.
/// Explicit subscriptions of the same bases are indistinguishable from the scalar aliases.
///
/// TODO: Remove this helper and the associated compatibility exceptions once all supported NumPy
/// versions include <https://github.com/numpy/numpy/pull/33052>.
pub(in crate::types) fn is_scalar_alias(db: &dyn Db, alias: ClassSpecialization<'_>) -> bool {
    let origin = alias.origin(db);
    if !matches!(
        origin.name(db).as_str(),
        "signedinteger" | "unsignedinteger" | "floating" | "complexfloating"
    ) || !origin.file(db).is_stub(db)
        || !origin.definition(db).file_scope(db).is_global()
    {
        return false;
    }

    file_to_module(db, origin.program_file(db).resolver_file(db))
        .is_some_and(|module| module.name(db).as_str() == "numpy")
}
