use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use rustc_hash::FxHashSet;
use ty_python_core::{
    DeclarationsIterator, ast_ids::HasScopedUseId, definition::DefinitionKind,
    place::ScopedPlaceId, use_def_map,
};

use crate::types::known_instance::{FieldInstance, FieldInstanceKind};
use crate::types::{
    BoundTypeVarInstance, DataclassFlags, FunctionType, GenericContext, KnownFunction, Parameter,
    Parameters, Signature, StaticClassLiteral, Type, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

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

/// Recognizes attrs field specifiers like `attr.ib` and `attrs.field`.
pub(in crate::types) fn field_instance_kind<'db>(
    db: &'db dyn Db,
    specifier: FunctionType<'db>,
) -> FieldInstanceKind {
    match specifier.known(db) {
        Some(KnownFunction::AttrAttrib | KnownFunction::AttrsField) => FieldInstanceKind::Attrs,
        _ => FieldInstanceKind::Other,
    }
}

/// Returns the type of a generic identity function `[F](F, /) -> F`.
///
/// We use this for the `<field>.validator` and `<field>.default` decorators, which simply return
/// the original method unchanged.
/// <https://github.com/python-attrs/attrs/blob/7bfc49e9b22d5ba25b6e429524c3d49fee27cb36/src/attr/_make.py#L2811-L2841>
pub(in crate::types) fn generic_identity_function<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> Type<'db> {
    let f =
        BoundTypeVarInstance::synthetic(db, env, Name::new_static("F"), TypeVarVariance::Invariant);
    let signature = Signature::new_generic(
        Some(GenericContext::from_typevar_instances(db, env, [f])),
        Parameters::standard([
            Parameter::positional_only(None).with_annotated_type(Type::TypeVar(f))
        ]),
        Type::TypeVar(f),
    );
    Type::function_like_callable(db, signature)
}

/// Keeps track of fields that have a default value provided through a `@<field>.default`-decorated method.
///
/// For the following class, `collect_default_generator` will be called for the `default_id` method. It will
/// then record the field so that it can later be recognized as having a default value.
///
/// ```python
/// from attrs import define, field
///
/// @define
/// class User:
///     id: int = field()
///
///     @id.default
///     def default_id(self) -> int:
///         return 0
/// ```
pub(in crate::types) struct DecoratedDefaults {
    fields: FxHashSet<ScopedPlaceId>,
}

impl DecoratedDefaults {
    pub(in crate::types) fn new() -> Self {
        Self {
            fields: FxHashSet::default(),
        }
    }

    /// Whether or not the attrs class contains a `@<field>.default`-decorated method for the given field.
    pub(in crate::types) fn has_default<'db>(
        &self,
        db: &'db dyn Db,
        place: ScopedPlaceId,
        field: FieldInstance<'db>,
    ) -> bool {
        self.fields.contains(&place) && field.kind(db).is_attrs()
    }

    /// This function is called for declarations in the attrs class. If the declaration refers
    /// to a method with a `@<field>.default` decorator, it records the field so that we can
    /// later determine if it has a default value.
    pub(in crate::types) fn collect_default_generator<'db>(
        &mut self,
        db: &'db dyn Db,
        declarations: DeclarationsIterator<'_, 'db>,
    ) {
        let Ok(declaration) = declarations.exactly_one() else {
            return;
        };
        let Some(definition) = declaration.declaration.definition() else {
            return;
        };
        let DefinitionKind::Function(function) = definition.kind(db) else {
            return;
        };
        if !function.has_decorators() {
            return;
        }

        let module = parsed_module(db, definition.python_file(db)).load(db);
        let use_def = use_def_map(db, definition.scope(db));
        for decorator in &function.node(&module).decorator_list {
            if let Some(attr) = decorator.expression.as_attribute_expr()
                && attr.attr.id == "default"
                && let Some(name) = attr.value.as_name_expr()
                && let Ok(binding) = use_def
                    .bindings_at_use(name.scoped_use_id(db, definition.program_file(db)))
                    .exactly_one()
                && let Some(field) = binding.binding.definition()
            {
                self.fields.insert(field.place(db));
            }
        }
    }
}
