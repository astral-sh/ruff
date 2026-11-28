use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{
    DeclarationsIterator, ScopedDefinitionId,
    ast_ids::HasScopedUseId,
    definition::{Definition, DefinitionKind},
    place::ScopedPlaceId,
    use_def_map,
};

use crate::place::known_module_symbol;
use crate::types::known_instance::FieldInstanceKind;
use crate::types::{
    BoundTypeVarInstance, DataclassFlags, FunctionType, GenericContext, KnownFunction, Parameter,
    Parameters, Signature, StaticClassLiteral, Type, TypeVarVariance, binding_type,
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

/// attrs [omits] `__attrs_attrs__: ClassVar[Any]` for type-checkers that are not Mypy.
/// Supply it on the public protocol (`attrs.AttrsInstance` re-exports `attr.AttrsInstance`)
/// unless the stubs already declare it.
///
/// TODO: Remove this fallback once the attrs stubs declare `__attrs_attrs__` for ty.
///
/// [omits]: https://github.com/python-attrs/attrs/blob/4a8098356659f37a5832be17aa1d8a40be7d7cec/src/attr/_typing_compat.pyi#L6-L15
pub(in crate::types) fn protocol_metadata_type<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<Type<'db>> {
    if class.name(db) != "AttrsInstance"
        || !file_to_module(db, class.program_file(db).resolver_file(db))?
            .is_known(db, KnownModule::Attr)
    {
        return None;
    }
    let protocol = class.into_protocol_class(db)?;
    (!protocol.has_member_declaration(db, "__attrs_attrs__")).then_some(Type::any())
}

/// The type of the synthesized `__attrs_attrs__` class variable.
pub(in crate::types) fn field_metadata_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
) -> Option<Type<'db>> {
    if is_attrs_class(db, class) {
        let attribute = known_module_symbol(db, env, KnownModule::Attr, "Attribute")
            .place
            .ignore_possibly_undefined()
            .and_then(|ty| ty.to_instance_approximation(db, env))
            .unwrap_or_else(Type::unknown);
        Some(Type::homogeneous_tuple(db, env, attribute))
    } else {
        // Allow reads of `__attrs_attrs__` on values typed as `AttrsInstance`.
        protocol_metadata_type(db, class)
    }
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
/// We use this for the `<field>.validator`, `<field>.default`, and `<field>.converter` decorators,
/// which simply return the original method unchanged.
/// <https://github.com/python-attrs/attrs/blob/4a8098356659f37a5832be17aa1d8a40be7d7cec/src/attr/_make.py#L2846-L2896>
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
/// Also records the first `@<field>.converter`-decorated method for each field.
///
/// For the following class, `collect` will be called for the `default_id` method. It will
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
#[derive(Default)]
pub(in crate::types) struct FieldDecorators<'db> {
    defaults: FxHashSet<ScopedPlaceId>,
    converters: FxHashMap<ScopedPlaceId, (ScopedDefinitionId, Definition<'db>)>,
}

impl<'db> FieldDecorators<'db> {
    /// Whether or not the attrs class contains a `@<field>.default`-decorated method for the given field.
    pub(in crate::types) fn has_default(&self, place: ScopedPlaceId) -> bool {
        self.defaults.contains(&place)
    }

    pub(in crate::types) fn converter_input_type(
        &self,
        db: &'db dyn Db,
        place: ScopedPlaceId,
    ) -> Option<Type<'db>> {
        let (_, definition) = self.converters.get(&place)?;
        let method = binding_type(db, *definition).as_function_literal()?;
        let params = method.last_definition_signature(db).parameters();
        // attrs calls the unbound method as `method(instance, field, value)`.
        params.get_positional(2).map(Parameter::annotated_type)
    }

    /// This function is called for declarations in the attrs class. If the declaration refers
    /// to a method with a `@<field>.default` decorator, it records the field so that we can
    /// later determine if it has a default value.
    /// For `@<field>.converter`, it records the method so we can look up its input type.
    pub(in crate::types) fn collect(
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
                && matches!(attr.attr.id.as_str(), "default" | "converter")
                && let Some(name) = attr.value.as_name_expr()
                && let Ok(binding) = use_def
                    .bindings_at_use(name.scoped_use_id(db, definition.program_file(db)))
                    .exactly_one()
                && let Some(field) = binding.binding.definition()
            {
                let place = field.place(db);
                if attr.attr.id == "default" {
                    self.defaults.insert(place);
                } else {
                    // attrs applies converters in source order, which can differ from symbol order.
                    let converter = (declaration.declaration_order, definition);
                    self.converters
                        .entry(place)
                        .and_modify(|first| *first = (*first).min(converter))
                        .or_insert(converter);
                }
            }
        }
    }
}
