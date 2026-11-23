use ruff_python_stdlib::identifiers::is_mangled_private;
use ty_module_resolver::{KnownModule, file_to_module};

use crate::Db;
use crate::types::{DataclassFlags, FunctionType, StaticClassLiteral};

pub(in crate::types) fn is_attrs_class<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> bool {
    class
        .dataclass_params(db)
        .is_some_and(|params| params.flags(db).contains(DataclassFlags::ATTRS))
}

/// Returns whether a dataclass-transform decorator is defined by attrs.
pub(in crate::types) fn is_decorator<'db>(db: &'db dyn Db, decorator: FunctionType<'db>) -> bool {
    matches!(
        file_to_module(db, decorator.program_file(db).resolver_file(db))
            .and_then(|module| module.known(db)),
        Some(KnownModule::Attr | KnownModule::Attrs)
    )
}

/// Returns the alias used by generated methods for a field declared in `class_name`.
pub(in crate::types) fn field_alias(
    class_name: &str,
    name: &str,
    explicit_alias: Option<Box<str>>,
) -> Option<Box<str>> {
    if let Some(alias) = explicit_alias.filter(|alias| !alias.is_empty()) {
        return Some(alias);
    }
    if !name.starts_with('_') {
        return None;
    }

    // Python mangles private names before attrs strips leading underscores.
    let class_name = class_name.trim_start_matches('_');
    if is_mangled_private(name) && !class_name.is_empty() {
        Some(format!("{class_name}{name}").into_boxed_str())
    } else {
        Some(name.trim_start_matches('_').into())
    }
}
