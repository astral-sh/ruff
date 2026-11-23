use ruff_python_stdlib::identifiers::is_mangled_private;

use crate::Db;
use crate::types::{DataclassFlags, FunctionType, KnownFunction, StaticClassLiteral};

/// Returns whether `class` is directly decorated with an `attrs` class decorator.
pub(in crate::types) fn is_attrs_class<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> bool {
    class
        .dataclass_params(db)
        .is_some_and(|params| params.flags(db).contains(DataclassFlags::ATTRS))
}

/// Returns whether `decorator` resolves to `attr.attrs`, `attrs.define`, or `attrs.frozen`.
pub(in crate::types) fn is_known_decorator<'db>(
    db: &'db dyn Db,
    decorator: FunctionType<'db>,
) -> bool {
    matches!(
        decorator.known(db),
        Some(KnownFunction::AttrAttrs | KnownFunction::AttrsDefine | KnownFunction::AttrsFrozen)
    )
}

/// Returns the alias used to name the constructor parameter for the field `name` in `class_name`.
/// If this returns `None`, the parameter takes the field's name.
pub(in crate::types) fn effective_field_alias(
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

    let class_name = class_name.trim_start_matches('_');
    if is_mangled_private(name) && !class_name.is_empty() {
        // For double-underscore field names, Python mangles `__id` to `_<class-name>__id` before attrs strips the leading underscore. So the
        // effective constructor parameter name for `__id` becomes `<class-name>__id` (https://github.com/python-attrs/attrs/issues/619):
        Some(format!("{class_name}{name}").into_boxed_str())
    } else {
        Some(name.trim_start_matches('_').into())
    }
}
