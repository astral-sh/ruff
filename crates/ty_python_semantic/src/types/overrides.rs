//! Checks relating to invalid method overrides in subclasses,
//! including (but not limited to) violations of the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

use bitflags::bitflags;
use ruff_db::{
    diagnostic::{Annotation, Span},
    files::FileRange,
    parsed::ParsedModuleRef,
};
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_python_stdlib::identifiers::is_mangled_private;
use rustc_hash::FxHashSet;

use crate::{
    Db, Program,
    lint::LintId,
    place::{DefinedPlace, Place, PlaceAndQualifiers, TypeOrigin},
    reachability::ReachabilityConstraintsExtension,
    types::{
        CallableType, ClassBase, ClassLiteral, ClassType, IntersectionType, KnownClass, Parameter,
        Parameters, Signature, StaticClassLiteral, Type, TypeContext, TypeQualifiers,
        call::CallArguments,
        class::{CodeGeneratorKind, FieldKind, MethodDecorator},
        constraints::ConstraintSetBuilder,
        context::InferContext,
        diagnostic::{
            INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_OVERRIDE, INVALID_DATACLASS,
            INVALID_EXPLICIT_OVERRIDE, INVALID_METHOD_OVERRIDE, INVALID_NAMED_TUPLE,
            INVALID_NAMED_TUPLE_OVERRIDE, MISSING_OVERRIDE_DECORATOR, OVERRIDE_OF_FINAL_METHOD,
            OVERRIDE_OF_FINAL_VARIABLE, report_incompatible_base_method,
            report_invalid_method_override, report_overridden_final_method,
            report_overridden_final_variable,
        },
        enums::{EnumMetadata, enum_metadata, is_enum_class_by_inheritance},
        function::{FunctionDecorators, FunctionType, KnownFunction, OverloadLiteral},
        infer::infer_definition_types,
        list_members::{Member, MemberWithDefinition, all_end_of_scope_members},
        tuple::Tuple,
    },
};
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    place::ScopedPlaceId,
    place_table,
    scope::ScopeId,
    symbol::ScopedSymbolId,
    use_def_map,
};

/// Prohibited `NamedTuple` attributes that cannot be overwritten.
/// See <https://github.com/python/cpython/blob/main/Lib/typing.py> for the list.
const PROHIBITED_NAMEDTUPLE_ATTRS: &[&str] = &[
    "__new__",
    "__init__",
    "__slots__",
    "__getnewargs__",
    "_fields",
    "_field_defaults",
    "_field_types",
    "_make",
    "_replace",
    "_asdict",
    "_source",
];

// TODO: Support dynamic class literals. If we allow dynamic classes to define attributes in their
// namespace dictionary, we should also check whether those attributes are valid overrides of
// attributes in their superclasses.
pub(super) fn check_class<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    inconsistent_generic_bases: bool,
) {
    let db = context.db();
    let configuration = OverrideRulesConfig::from(context);
    if configuration.no_rules_enabled() {
        return;
    }

    let scope = class.body_scope(db);
    let own_class_members: FxHashSet<_> = all_end_of_scope_members(db, scope).collect();
    let class_specialized = class.identity_specialization(db);
    if configuration.check_method_liskov_violations() && !inconsistent_generic_bases {
        check_inherited_method_conflicts(context, class, class_specialized, &own_class_members);
    }

    let enum_info = enum_metadata(db, class.into());

    #[expect(
        clippy::iter_over_hash_type,
        reason = "each class member is checked independently"
    )]
    for member in own_class_members {
        check_class_declaration(
            context,
            configuration,
            enum_info,
            class_specialized,
            scope,
            &member,
        );
    }
}

/// Rechecks methods defined on parents against the remainder of the resolved MRO.
///
/// A multiple-inheritance join can place two otherwise-unrelated classes in the same MRO. The
/// effective source-defined method in that ordering must be compatible with each later definition
/// of the same method:
///
/// ```python
/// class ReturnsStr:
///     def method(self) -> str: ...
///
/// class ReturnsInt:
///     def method(self) -> int: ...
///
/// class Combined(ReturnsStr, ReturnsInt): ...  # Error
/// ```
///
/// The caller skips classes with inconsistent generic bases, since their specialized MRO is not a
/// valid contract to check.
fn check_inherited_method_conflicts<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_specialized: ClassType<'db>,
    own_class_members: &FxHashSet<MemberWithDefinition<'db>>,
) {
    let db = context.db();

    let mut direct_bases = Vec::new();
    for base in class.explicit_bases(db) {
        match ClassBase::try_from_explicit_base(db, *base, Some(class.into())) {
            Some(ClassBase::Class(base)) if base.static_class_literal(db).is_some() => {
                direct_bases.push(base);
            }
            Some(
                ClassBase::Generic
                | ClassBase::Protocol
                | ClassBase::Any
                | ClassBase::Dynamic(_)
                | ClassBase::Divergent(_),
            ) => {}
            _ => return,
        }
    }
    if direct_bases.len() < 2 || class.try_mro(db, None).is_err() {
        return;
    }

    let constraints = ConstraintSetBuilder::new();
    if direct_bases.iter().enumerate().any(|(index, left)| {
        direct_bases[index + 1..]
            .iter()
            .any(|right| !left.could_coexist_in_mro_with(db, *right, &constraints))
    }) {
        return;
    }

    let mut mro = Vec::new();
    let mut first_dynamic_base = None;
    for base in class_specialized.iter_mro(db).skip(1) {
        match base {
            ClassBase::Class(base) if base.is_object(db) => break,
            ClassBase::Class(base) if base.static_class_literal(db).is_some() => mro.push(base),
            ClassBase::Protocol | ClassBase::Generic => {}
            ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                first_dynamic_base.get_or_insert(mro.len());
            }
            ClassBase::TypedDict(_) | ClassBase::Class(_) => return,
        }
    }
    let receiver = Type::instance(db, class_specialized);
    let mut seen_names: FxHashSet<_> = own_class_members
        .iter()
        .map(|member| member.member.name.clone())
        .collect();

    for (index, owner) in mro.iter().copied().enumerate() {
        if first_dynamic_base.is_some_and(|dynamic_index| index >= dynamic_index) {
            break;
        }
        let Some((owner_literal, _)) = owner.static_class_literal(db) else {
            continue;
        };
        let scope = owner_literal.body_scope(db);
        let members: FxHashSet<_> = all_end_of_scope_members(db, scope).collect();

        #[expect(
            clippy::iter_over_hash_type,
            reason = "each class member is checked independently"
        )]
        'members: for member in members {
            let name = &member.member.name;
            if is_mangled_private(name.as_str())
                || is_constructor_like_method(name.as_str())
                || !seen_names.insert(name.clone())
            {
                continue;
            }
            let Some((selected_decorator, selected_ty)) =
                source_method_contract(db, owner, receiver, name)
            else {
                continue;
            };

            for contract_owner in mro[index + 1..].iter().copied() {
                let Some((contract_decorator, contract_ty)) =
                    source_method_contract(db, contract_owner, receiver, name)
                else {
                    continue;
                };
                let Some((selected_ty, contract_ty)) =
                    method_override_types(db, selected_ty, contract_ty)
                else {
                    continue;
                };
                if selected_decorator == contract_decorator
                    && selected_ty.is_assignable_to(db, contract_ty)
                {
                    continue;
                }

                // `EnumType` can replace mixin dunders while constructing the enum, so the
                // inherited definitions do not necessarily describe the resulting method. For
                // example, the inherited check would otherwise compare the incompatible
                // `int.__format__` and `Enum.__format__` definitions here:
                //
                // ```python
                // from enum import Enum
                //
                // # int.__format__(self, format_spec: str, /) -> str
                // # Enum.__format__(self, format_spec: str) -> str
                // class Status(int, Enum):
                //     READY = 1
                // ```
                //
                // Keep this check specific to the enum definition so conflicts between two
                // ordinary mixins are still reported.
                if enum_class_creation_manages_conflict(db, class, name, owner, contract_owner) {
                    continue;
                }

                // Do not re-emit an incompatibility that already exists in the parent's own MRO.
                // This matters for intentionally suppressed typeshed overrides such as
                // `str.__contains__` versus `Sequence.__contains__`, while still allowing a
                // receiver-sensitive incompatibility that appears only when rebound to `class`.
                // Resolve the ancestor in the parent's own MRO so that its generic specialization
                // matches the one used by the normal Liskov check on the parent.
                if let Some(parent_contract_owner) = owner
                    .iter_mro(db)
                    .skip(1)
                    .filter_map(ClassBase::into_class)
                    .find(|ancestor| ancestor.class_literal(db) == contract_owner.class_literal(db))
                {
                    let parent_receiver = Type::instance(db, owner);
                    let Some((parent_decorator, parent_ty)) =
                        source_method_contract(db, owner, parent_receiver, name)
                    else {
                        continue;
                    };
                    let Some((ancestor_decorator, ancestor_ty)) =
                        source_method_contract(db, parent_contract_owner, parent_receiver, name)
                    else {
                        continue;
                    };
                    if parent_decorator != ancestor_decorator
                        || !is_assignable_method_override(db, parent_ty, ancestor_ty)
                    {
                        continue;
                    }
                }

                let Some((contract_literal, _)) = contract_owner.static_class_literal(db) else {
                    continue;
                };
                let contract_scope = contract_literal.body_scope(db);
                let Some(contract_symbol) = place_table(db, contract_scope).symbol_id(name) else {
                    continue;
                };
                let Some(contract_definition) =
                    symbol_definition(db, contract_scope, contract_symbol)
                else {
                    continue;
                };
                report_incompatible_base_method(
                    context,
                    class,
                    name,
                    (owner, member.first_reachable_definition, selected_decorator),
                    (contract_owner, contract_definition, contract_decorator),
                    || selected_ty.assignability_error_context(db, contract_ty),
                );
                continue 'members;
            }
        }
    }
}

/// Returns a source-defined method bound to the class whose MRO is being checked.
fn source_method_contract<'db>(
    db: &'db dyn Db,
    owner: ClassType<'db>,
    receiver: Type<'db>,
    name: &Name,
) -> Option<(MethodDecorator, Type<'db>)> {
    // TODO: Check inherited conflicts involving properties and other attributes. For example:
    //
    // ```python
    // class ReturnsStr:
    //     @property
    //     def value(self) -> str: ...
    //
    // class ReturnsInt:
    //     @property
    //     def value(self) -> int: ...
    //
    // class Conflict(ReturnsStr, ReturnsInt): ...
    // ```
    let Type::FunctionLiteral(function) = owner
        .own_class_member(db, None, name)
        .inner
        .place
        .raw_type()?
    else {
        return None;
    };
    let ty = Type::FunctionLiteral(function)
        .try_call_dunder_get(db, Some(receiver), receiver.to_meta_type(db))?
        .0;
    Some((MethodDecorator::try_from_fn_type(db, function)?, ty))
}

/// Returns `true` when this source-level conflict involves a method replaced by `EnumType` during
/// class creation.
///
/// Restricting this to a known enum implementation still allows two ordinary mixins to contribute
/// conflicting contracts for the same method name.
fn enum_class_creation_manages_conflict<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    name: &Name,
    selected_owner: ClassType<'db>,
    contract_owner: ClassType<'db>,
) -> bool {
    if !is_enum_class_by_inheritance(db, class) {
        return false;
    }

    if matches!(
        name.as_str(),
        "__repr__" | "__str__" | "__format__" | "__reduce_ex__"
    ) {
        return selected_owner.is_known(db, KnownClass::Enum)
            || contract_owner.is_known(db, KnownClass::Enum);
    }

    Program::get(db).python_version(db) >= PythonVersion::PY311
        && Type::ClassLiteral(class.into()).is_subtype_of(db, KnownClass::Flag.to_subclass_of(db))
        && matches!(
            name.as_str(),
            "__or__" | "__and__" | "__xor__" | "__ror__" | "__rand__" | "__rxor__" | "__invert__"
        )
        && (selected_owner.is_known(db, KnownClass::Flag)
            || contract_owner.is_known(db, KnownClass::Flag))
}

/// Returns the first inherited `NamedTuple` field in the MRO for `field_name`.
fn conflicting_named_tuple_field_in_mro<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    field_name: &Name,
) -> Option<(ClassType<'db>, Option<Definition<'db>>)> {
    for class_base in class.iter_mro(db, None).skip(1) {
        let Some(superclass) = class_base.into_class() else {
            continue;
        };

        let (superclass_literal, superclass_specialization) =
            superclass.class_literal_and_specialization(db);

        if CodeGeneratorKind::NamedTuple.matches(db, superclass_literal) {
            match superclass_literal {
                ClassLiteral::Static(superclass_literal) => {
                    if let Some(field) = superclass_literal
                        .own_fields(db, superclass_specialization, CodeGeneratorKind::NamedTuple)
                        .get(field_name)
                    {
                        return Some((superclass, field.first_declaration));
                    }
                }
                ClassLiteral::DynamicNamedTuple(namedtuple) => {
                    if namedtuple.field(db, field_name).is_some() {
                        return Some((superclass, namedtuple.definition(db)));
                    }
                }
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_) => {}
            }
        }
    }

    None
}

fn check_class_declaration<'db>(
    context: &InferContext<'db, '_>,
    configuration: OverrideRulesConfig,
    enum_info: Option<&EnumMetadata<'db>>,
    class: ClassType<'db>,
    class_scope: ScopeId<'db>,
    member: &MemberWithDefinition<'db>,
) {
    let db = context.db();

    let MemberWithDefinition {
        member,
        first_reachable_definition,
    } = member;

    let instance_of_class = Type::instance(db, class);

    let subclass_instance_member = instance_of_class.member(db, &member.name);
    let Place::Defined(DefinedPlace {
        ty: type_on_subclass_instance,
        ..
    }) = subclass_instance_member.place
    else {
        return;
    };

    let Some((literal, _)) = class.static_class_literal(db) else {
        return;
    };
    let class_kind = CodeGeneratorKind::from_class(db, literal.into());

    // Check for prohibited `NamedTuple` attribute overrides.
    //
    // `NamedTuple` classes have certain synthesized attributes (like `_asdict`, `_make`, etc.)
    // that cannot be overwritten. Attempting to assign to these attributes (without type
    // annotations) or define methods with these names will raise an `AttributeError` at runtime.
    match class_kind {
        Some(CodeGeneratorKind::NamedTuple) => {
            if configuration.check_invalid_named_tuple_definitions()
                && PROHIBITED_NAMEDTUPLE_ATTRS.contains(&member.name.as_str())
                && let Some(symbol_id) = place_table(db, class_scope).symbol_id(&member.name)
                && let Some(bad_definition) = use_def_map(db, class_scope)
                    .reachable_bindings(ScopedPlaceId::Symbol(symbol_id))
                    .filter_map(|binding| binding.binding.definition())
                    .find(|def| !matches!(def.kind(db), DefinitionKind::AnnotatedAssignment(_)))
                && let Some(builder) = context.report_lint(
                    &INVALID_NAMED_TUPLE,
                    bad_definition.focus_range(db, context.module()),
                )
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot overwrite NamedTuple attribute `{}`",
                    member.name
                ));
                diagnostic.info("This will cause the class creation to fail at runtime");
            }
        }
        Some(policy @ CodeGeneratorKind::DataclassLike(_)) => {
            check_post_init_signature(
                context,
                configuration,
                class,
                member,
                *first_reachable_definition,
                policy,
            );
        }
        Some(CodeGeneratorKind::Pydantic(_) | CodeGeneratorKind::TypedDict) | None => {}
    }

    if configuration.check_invalid_named_tuple_field_overrides()
        && let Some((superclass, overridden_field_declaration)) =
            conflicting_named_tuple_field_in_mro(db, literal, &member.name)
        && let Some(builder) = context.report_lint(
            &INVALID_NAMED_TUPLE_OVERRIDE,
            first_reachable_definition.focus_range(db, context.module()),
        )
    {
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot override NamedTuple field `{}` inherited from `{}`",
            member.name,
            superclass.name(db)
        ));
        diagnostic
            .info("Subclass members are not allowed to reuse inherited NamedTuple field names");
        if let Some(first_declaration) = overridden_field_declaration
            && first_declaration.file(db) == context.file()
        {
            diagnostic.annotate(
                Annotation::secondary(
                    context.span(first_declaration.kind(db).full_range(context.module())),
                )
                .message(format_args!(
                    "Inherited NamedTuple field `{}` declared here",
                    member.name
                )),
            );
        }
    }

    // Check for invalid Enum member values.
    if let Some(enum_info) = enum_info {
        if member.name != "_value_"
            && matches!(
                first_reachable_definition.kind(db),
                DefinitionKind::Assignment(_) | DefinitionKind::AnnotatedAssignment(_)
            )
        {
            // Use the value type from `EnumMetadata` rather than `member.ty`, because
            // for annotated assignments like `X: Final = "value"`, the member may come
            // from the declaration chain (where `ty` is the declared type, e.g. `Unknown`)
            // rather than the binding chain (where `ty` is the actual value type).
            let Some(&member_value_type) = enum_info.members.get(&member.name) else {
                return;
            };

            // TODO ideally this would be a syntactic check that only matches on literal `...`
            // in the source, rather than matching on the type. But this would require storing
            // additional information in `EnumMetadata`.
            let is_ellipsis = matches!(
                member_value_type,
                Type::NominalInstance(nominal_instance)
                    if nominal_instance.has_known_class(db, KnownClass::EllipsisType)
            );
            // `auto()` values are computed at runtime by the enum metaclass,
            // so we can't validate them against _value_ or __init__ at the type level.
            let is_auto = enum_info.auto_members.contains(&member.name);
            let skip_type_check = (context.in_stub() && is_ellipsis)
                || is_auto
                || enum_info.value_construction.metaclass_may_transform_values;

            if !skip_type_check {
                if let Some(new_function) = enum_info.value_construction.new.function() {
                    check_enum_member_against_constructor_method(
                        context,
                        new_function,
                        Type::from(class),
                        member_value_type,
                        &member.name,
                        *first_reachable_definition,
                        EnumConstructorMethod::New,
                    );
                }

                if let Some(init_function) = enum_info.value_construction.init.function() {
                    check_enum_member_against_constructor_method(
                        context,
                        init_function,
                        instance_of_class,
                        member_value_type,
                        &member.name,
                        *first_reachable_definition,
                        EnumConstructorMethod::Init,
                    );
                } else if enum_info
                    .value_construction
                    .can_validate_with_value_annotation()
                    && let Some(expected_type) = enum_info.value_annotation_type()
                {
                    if !member_value_type.is_assignable_to(db, expected_type) {
                        if let Some(builder) = context.report_lint(
                            &INVALID_ASSIGNMENT,
                            first_reachable_definition.focus_range(db, context.module()),
                        ) {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Enum member `{}` value is not assignable to expected type",
                                member.name
                            ));
                            diagnostic.info(format_args!(
                                "Expected `{}`, got `{}`",
                                expected_type.display(db),
                                member_value_type.display(db)
                            ));
                        }
                    }
                }
            }
        }
    }

    let mut subclass_overrides_superclass_declaration = false;
    let mut has_dynamic_superclass = false;
    let mut has_typeddict_in_mro = false;
    let mut liskov_diagnostic_emitted = false;
    let mut missing_override_target: Option<MissingOverrideTarget<'db>> = None;
    let mut overridden_final_method = None;
    let mut overridden_final_variable: Option<(ClassType<'db>, Option<Definition<'db>>)> = None;
    let is_private_member = is_mangled_private(member.name.as_str());
    let mut subclass_variable_kind: Option<Option<VariableKind>> = None;

    // Track the first superclass that defines this method (the "immediate parent" for this method).
    // We need this to check if parent itself already has an LSP violation with an ancestor.
    // If so, we shouldn't report the same violation for the child class.
    let mut immediate_parent_method: Option<(ClassType<'db>, Type<'db>)> = None;
    let mut immediate_parent_variable_kind: Option<(ClassType<'db>, VariableKind)> = None;

    if !is_private_member {
        for class_base in class.iter_mro(db).skip(1) {
            let superclass = match class_base {
                ClassBase::Protocol | ClassBase::Generic => continue,
                ClassBase::Any | ClassBase::Dynamic(_) => {
                    has_dynamic_superclass = true;
                    continue;
                }
                ClassBase::Divergent(_) => {
                    has_dynamic_superclass = true;
                    continue;
                }
                ClassBase::TypedDict(_) => {
                    has_typeddict_in_mro = true;
                    continue;
                }
                ClassBase::Class(class) => class,
            };

            let Some((superclass_literal, superclass_specialization)) =
                superclass.static_class_literal(db)
            else {
                continue;
            };
            let superclass_scope = superclass_literal.body_scope(db);
            let superclass_symbol_table = place_table(db, superclass_scope);
            let superclass_symbol_id = superclass_symbol_table.symbol_id(&member.name);

            let mut method_kind = MethodKind::default();

            // If the member is not defined on the class itself, skip it
            if let Some(id) = superclass_symbol_id {
                let superclass_symbol = superclass_symbol_table.symbol(id);
                if !(superclass_symbol.is_bound() || superclass_symbol.is_declared()) {
                    continue;
                }
            } else {
                if superclass_literal
                    .own_synthesized_member(db, superclass_specialization, None, &member.name)
                    .is_none()
                {
                    continue;
                }
                method_kind = CodeGeneratorKind::from_class(db, superclass_literal.into())
                    .map(MethodKind::Synthesized)
                    .unwrap_or_default();
            }

            let superclass_instance_member =
                Type::instance(db, superclass).member(db, &member.name);
            let Place::Defined(DefinedPlace {
                ty: superclass_type,
                ..
            }) = superclass_instance_member.place
            else {
                // If not defined on any superclass, no point in continuing to walk up the MRO
                break;
            };

            subclass_overrides_superclass_declaration = true;

            // Record the first overridden superclass member that is subject to the missing override
            // decorator check so that we can later confirm that the overriding definition is indeed
            // marked with the decorator.
            if configuration.check_missing_overrides()
                && missing_override_target.is_none()
                && !is_constructor_like_method(&member.name)
            {
                missing_override_target = Some(MissingOverrideTarget::for_superclass(
                    db,
                    superclass,
                    superclass_scope,
                    superclass_symbol_id,
                ));
            }

            // Record the first superclass that defines this method as the "immediate parent method"
            if immediate_parent_method.is_none() {
                immediate_parent_method = Some((superclass, superclass_type));
            }

            if (configuration.check_final_method_overridden() && overridden_final_method.is_none())
                || (configuration.check_final_variable_overridden()
                    && overridden_final_variable.is_none())
            {
                let own_class_member = superclass.own_class_member(db, None, &member.name);

                if configuration.check_final_method_overridden() {
                    overridden_final_method = overridden_final_method.or_else(|| {
                        let superclass_symbol_id = superclass_symbol_id?;

                        // TODO: `@final` should be more like a type qualifier:
                        // we should also recognise `@final`-decorated methods that don't end up
                        // as being function- or property-types (because they're wrapped by other
                        // decorators that transform the type into something else).
                        let underlying_functions = extract_underlying_functions(
                            db,
                            own_class_member.ignore_possibly_undefined()?,
                        );

                        if underlying_functions.iter().any(|function| {
                            function.has_known_decorator(db, FunctionDecorators::FINAL)
                        }) && is_function_definition(db, superclass_scope, superclass_symbol_id)
                        {
                            Some((superclass, underlying_functions))
                        } else {
                            None
                        }
                    });
                }

                if configuration.check_final_variable_overridden() {
                    overridden_final_variable = overridden_final_variable.or_else(|| {
                        if !own_class_member
                            .qualifiers()
                            .contains(TypeQualifiers::FINAL)
                        {
                            return None;
                        }

                        // Find the declaration definition in the superclass for the secondary
                        // annotation.
                        let superclass_definition = superclass_symbol_id.and_then(|id| {
                            use_def_map(db, superclass_scope)
                                .end_of_scope_symbol_declarations(id)
                                .find_map(|decl| decl.declaration.definition())
                        });

                        Some((superclass, superclass_definition))
                    });
                }
            }

            // **********************************************************
            // Everything below this point in the loop
            // is about Liskov Substitution Principle checks
            // **********************************************************

            // Only one Liskov diagnostic should be emitted per each invalid override,
            // even if it overrides multiple superclasses incorrectly!
            if liskov_diagnostic_emitted {
                continue;
            }

            if !configuration.check_liskov_violations() {
                continue;
            }

            if configuration.check_attribute_liskov_violations() {
                if let Some(superclass_variable_kind) =
                    effective_superclass_variable_kind(db, superclass, member.name.clone())
                {
                    if immediate_parent_variable_kind.is_none() {
                        immediate_parent_variable_kind =
                            Some((superclass, superclass_variable_kind));
                    }

                    let subclass_kind = *subclass_variable_kind.get_or_insert_with(|| {
                        variable_kind(
                            db,
                            class.own_class_member(db, None, &member.name).inner,
                            subclass_instance_member,
                        )
                    });

                    if let Some(subclass_kind) = subclass_kind
                        && subclass_kind != superclass_variable_kind
                    {
                        // An unannotated class-body assignment can inherit an overridden `ClassVar`
                        // declaration instead of introducing a conflicting instance variable. This
                        // also applies to augmented assignments after the initial class-body
                        // assignment, e.g. `epilog = "..."; epilog += "..."`.
                        if subclass_kind == VariableKind::Instance
                            && superclass_variable_kind == VariableKind::Class
                            && matches!(
                                first_reachable_definition.kind(db),
                                DefinitionKind::Assignment(_)
                                    | DefinitionKind::AugmentedAssignment(_)
                            )
                        {
                            continue;
                        }

                        if let Some((immediate_parent, immediate_parent_kind)) =
                            immediate_parent_variable_kind
                            && immediate_parent != superclass
                            && immediate_parent.is_subclass_of(db, superclass)
                            && immediate_parent_kind != superclass_variable_kind
                        {
                            continue;
                        }

                        let superclass_definition = superclass_symbol_id
                            .and_then(|id| symbol_definition(db, superclass_scope, id));
                        report_invalid_attribute_override(
                            context,
                            &member.name,
                            *first_reachable_definition,
                            superclass,
                            superclass_definition,
                            subclass_kind,
                            superclass_variable_kind,
                        );
                        liskov_diagnostic_emitted = true;
                        continue;
                    }
                }
            }

            if !configuration.check_method_liskov_violations() {
                continue;
            }

            let Type::FunctionLiteral(subclass_function) = member.ty else {
                continue;
            };

            // Constructor methods are not checked for Liskov compliance
            if is_constructor_like_method(&member.name) {
                continue;
            }

            // Synthesized `__replace__` methods on dataclasses are not checked
            if &member.name == "__replace__"
                && class_kind.is_some_and(CodeGeneratorKind::is_dataclass_like)
            {
                continue;
            }

            let Some((subclass_override_type, superclass_override_type)) =
                method_override_types(db, type_on_subclass_instance, superclass_type)
            else {
                continue;
            };

            if subclass_override_type.is_assignable_to(db, superclass_override_type) {
                continue;
            }

            // If this superclass is not the immediate parent for this method,
            // check if the immediate parent itself already has an LSP violation with this ancestor.
            // If so, don't report the same violation for the child class -- it would be a false positive
            // since the child cannot fix the violation without contradicting its immediate parent's contract.
            // See: https://github.com/astral-sh/ty/issues/2000
            if let Some((immediate_parent, immediate_parent_type)) = immediate_parent_method {
                if immediate_parent != superclass {
                    // The immediate parent already defines this method and is different from the
                    // current ancestor we're checking. Check if the immediate parent's method
                    // is also incompatible with this ancestor.
                    if !is_assignable_method_override(db, immediate_parent_type, superclass_type) {
                        // The immediate parent already has an LSP violation with this ancestor.
                        // Don't report the same violation for the child.
                        continue;
                    }
                }
            }

            report_invalid_method_override(
                context,
                &member.name,
                class,
                *first_reachable_definition,
                subclass_function,
                superclass,
                superclass_type,
                method_kind,
                || subclass_override_type.assignability_error_context(db, superclass_override_type),
            );

            liskov_diagnostic_emitted = true;
        }
    }

    if !subclass_overrides_superclass_declaration && !has_dynamic_superclass {
        if has_typeddict_in_mro {
            if !KnownClass::TypedDictFallback
                .to_instance(db)
                .member(db, &member.name)
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = true;
            }
        } else if class_kind == Some(CodeGeneratorKind::NamedTuple) {
            if !KnownClass::NamedTupleFallback
                .to_instance(db)
                .member(db, &member.name)
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = true;
            }
        }
    }

    if let Some(target) = missing_override_target
        && first_reachable_definition.kind(db).is_function_def()
    {
        check_missing_overrides(context, member, class_scope, target);
    }

    if !subclass_overrides_superclass_declaration
        && !has_dynamic_superclass
        // accessing `.kind()` here is fine as `definition`
        // will always be a definition in the file currently being checked
        && first_reachable_definition.kind(db).is_function_def()
    {
        check_explicit_overrides(context, member, class_scope, class);
    }

    if let Some((superclass, superclass_method)) = overridden_final_method {
        report_overridden_final_method(
            context,
            &member.name,
            *first_reachable_definition,
            member.ty,
            superclass,
            class,
            &superclass_method,
        );
    }

    if let Some((superclass, superclass_definition)) = overridden_final_variable {
        report_overridden_final_variable(
            context,
            &member.name,
            *first_reachable_definition,
            superclass,
            class,
            superclass_definition,
        );
    }
}

/// Checks whether a method override preserves its superclass method's callable domain.
///
/// An explicitly annotated superclass receiver can restrict a method to a subset of subclass
/// receivers. Bind both methods to that common receiver domain before comparing their signatures.
///
/// ```python
/// from typing import Protocol
///
/// class HasValue(Protocol):
///     value: int
///
/// class Mixin:
///     def method(self: HasValue) -> None: ...
///
/// class Sub(Mixin):
///     def method(self: HasValue) -> None: ...
/// ```
fn is_assignable_method_override<'db>(
    db: &'db dyn Db,
    subclass_type: Type<'db>,
    superclass_type: Type<'db>,
) -> bool {
    method_override_types(db, subclass_type, superclass_type).is_some_and(
        |(subclass_type, superclass_type)| subclass_type.is_assignable_to(db, superclass_type),
    )
}

fn method_override_types<'db>(
    db: &'db dyn Db,
    subclass_type: Type<'db>,
    superclass_type: Type<'db>,
) -> Option<(Type<'db>, Type<'db>)> {
    let (subclass_type, superclass_type) = match (subclass_type, superclass_type) {
        (Type::BoundMethod(subclass_method), Type::BoundMethod(superclass_method)) => {
            let superclass_signature = superclass_method.function(db).signature(db);
            let receiver = match superclass_signature.overloads.as_slice() {
                [signature] => signature
                    .parameters()
                    .get(0)
                    .filter(|parameter| parameter.is_positional() && !parameter.inferred_annotation)
                    .map(Parameter::annotated_type),
                // TODO: Compare overloaded mixin methods within each overload's explicit receiver
                // domain. Binding them directly to the concrete subclass can filter out applicable
                // overloads when the subclass does not itself satisfy the receiver protocol.
                _ => None,
            };

            receiver.map_or((subclass_type, superclass_type), |receiver| {
                let typing_self_type = subclass_method.typing_self_type(db);
                let receiver = receiver.bind_self_typevars(db, typing_self_type);
                let receiver = IntersectionType::from_elements(
                    db,
                    [subclass_method.self_instance(db), receiver],
                );
                (
                    Type::Callable(subclass_method.into_callable_type_with_receiver(
                        db,
                        receiver,
                        typing_self_type,
                    )),
                    Type::Callable(superclass_method.into_callable_type_with_receiver(
                        db,
                        receiver,
                        typing_self_type,
                    )),
                )
            })
        }
        _ => (subclass_type, superclass_type),
    };
    let superclass_callable = superclass_type.try_upcast_to_callable(db)?;

    Some((subclass_type, superclass_callable.into_type(db)))
}

/// Whether an attribute declaration is a class variable or an instance variable.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize)]
enum VariableKind {
    /// A variable annotated with `ClassVar`.
    Class,
    /// An instance variable, including an unannotated class-body assignment.
    Instance,
}

impl VariableKind {
    /// Returns the wording used for this variable kind in diagnostics.
    const fn description(self) -> &'static str {
        match self {
            VariableKind::Class => "class variable",
            VariableKind::Instance => "instance variable",
        }
    }
}

/// Returns the variable kind for a superclass member.
fn superclass_variable_kind<'db>(
    db: &'db dyn Db,
    superclass_scope: ScopeId<'db>,
    superclass_symbol_id: Option<ScopedSymbolId>,
    class_member: PlaceAndQualifiers<'db>,
    instance_member: PlaceAndQualifiers<'db>,
) -> Option<VariableKind> {
    // Method definitions and properties are not instance-variable declarations. Check the symbol
    // definition before class/instance member lookup can erase that distinction. For example,
    // resolving an abstract `@property def f(self) -> int` through instance-member lookup would
    // make it look like an instance variable of type `int`, causing this rule to report
    // `f: ClassVar[int]` as an invalid attribute override even though the superclass member is not
    // an instance-attribute declaration.
    if superclass_symbol_id.is_some_and(|id| is_function_definition(db, superclass_scope, id)) {
        return None;
    }

    // Final attributes have their own override rule and diagnostic. Treating them as class
    // variables here would report both diagnostics for the same override.
    if class_member.qualifiers.contains(TypeQualifiers::FINAL) {
        return None;
    }

    variable_kind(db, class_member, instance_member)
}

/// Returns the variable kind for a superclass member, preserving inherited `ClassVar` declarations
/// through unannotated class-body assignments.
///
/// For example, `Intermediate.x = 1` inherits the pure-class-variable declaration from `Base`, so
/// `Sub.x: ClassVar[int]` should not be reported as overriding an instance variable:
///
/// ```python
/// from typing import ClassVar
///
/// class Base:
///     x: ClassVar[int]
///
/// class Intermediate(Base):
///     x = 1
///
/// class Sub(Intermediate):
///     x: ClassVar[int] = 2
/// ```
#[allow(clippy::needless_pass_by_value)]
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn effective_superclass_variable_kind<'db>(
    db: &'db dyn Db,
    superclass: ClassType<'db>,
    name: Name,
) -> Option<VariableKind> {
    let inherited_variable_kind = || {
        superclass
            .iter_mro(db)
            .skip(1)
            .filter_map(ClassBase::into_class)
            .find_map(|base| effective_superclass_variable_kind(db, base, name.clone()))
    };

    let (superclass_literal, superclass_specialization) = superclass.static_class_literal(db)?;
    let superclass_scope = superclass_literal.body_scope(db);
    let superclass_symbol_table = place_table(db, superclass_scope);
    let superclass_symbol_id = superclass_symbol_table.symbol_id(&name);

    let has_own_member = if let Some(id) = superclass_symbol_id {
        let superclass_symbol = superclass_symbol_table.symbol(id);
        superclass_symbol.is_bound() || superclass_symbol.is_declared()
    } else {
        superclass_literal
            .own_synthesized_member(db, superclass_specialization, None, &name)
            .is_some()
    };

    if has_own_member {
        let superclass_variable_kind = superclass_variable_kind(
            db,
            superclass_scope,
            superclass_symbol_id,
            superclass.own_class_member(db, None, &name).inner,
            Type::instance(db, superclass).member(db, &name),
        );

        if superclass_variable_kind == Some(VariableKind::Instance)
            && superclass_symbol_id.is_some_and(|id| {
                symbol_definition(db, superclass_scope, id).is_some_and(|definition| {
                    matches!(
                        definition.kind(db),
                        DefinitionKind::Assignment(_) | DefinitionKind::AugmentedAssignment(_)
                    )
                })
            })
            && inherited_variable_kind() == Some(VariableKind::Class)
        {
            return Some(VariableKind::Class);
        }

        if superclass_variable_kind.is_some() {
            return superclass_variable_kind;
        }
    }

    inherited_variable_kind()
}

/// Salsa-tracked query to check whether any of the definitions of a symbol
/// in a superclass scope are function definitions.
///
/// We need to know this for compatibility with pyright and mypy, neither of which emit an error
/// on `C.f` here:
///
/// ```python
/// from typing import final
///
/// class A:
///     @final
///     def f(self) -> None: ...
///
/// class B:
///     f = A.f
///
/// class C(B):
///     def f(self) -> None: ...  # no error here
/// ```
///
/// This is a Salsa-tracked query because it has to look at the AST node for the definition,
/// which might be in a different Python module. If this weren't a tracked query, we could
/// introduce cross-module dependencies and over-invalidation.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn is_function_definition<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> bool {
    use_def_map(db, scope)
        .end_of_scope_symbol_bindings(symbol)
        .filter_map(|binding| binding.binding.definition())
        .any(|definition| definition.kind(db).is_function_def())
}

/// Returns the variable kind for an attribute if it should participate in `ClassVar` override checks.
fn variable_kind<'db>(
    db: &'db dyn Db,
    class_member: PlaceAndQualifiers<'db>,
    instance_member: PlaceAndQualifiers<'db>,
) -> Option<VariableKind> {
    if class_member.is_class_var() || instance_member.is_class_var() {
        return Some(VariableKind::Class);
    }

    // A `Final` attribute behaves like a class variable, but final overrides are diagnosed by
    // `override-of-final-variable` instead of this rule.
    if class_member.qualifiers.contains(TypeQualifiers::FINAL) {
        return None;
    }

    // A method definition is a descriptor in the class body, not an instance variable declaration,
    // even though instance lookup binds it as a method. It should therefore not participate in the
    // class-variable vs. instance-variable declaration check. For example, `Sub.f` here is a
    // descriptor stored on the class, not an instance attribute:
    //
    // ```python
    // class Base:
    //     f: ClassVar[int]
    //
    // class Sub(Base):
    //     def f(self) -> int: ...
    // ```
    if matches!(
        class_member.place,
        Place::Defined(DefinedPlace {
            ty: Type::FunctionLiteral(_),
            ..
        })
    ) {
        return None;
    }

    // Descriptor values are not normal instance variables: lookup calls `__get__`, so the value
    // exposed through an instance can differ from the value stored on the class. For example,
    // `attr = property(lambda self: 1)` installs a descriptor value, so `C().attr` exposes the
    // getter return type instead of the `property` object. By contrast, `attr: Descriptor` only
    // annotates an instance attribute; the annotated type having `__get__` does not make `C.attr`
    // a descriptor value.
    if let Place::Defined(DefinedPlace {
        ty: class_member_ty,
        origin: TypeOrigin::Inferred,
        ..
    }) = class_member.place
        && class_member_ty
            .class_member(db, "__get__")
            .place
            .ignore_possibly_undefined()
            .is_some()
    {
        return None;
    }

    Some(VariableKind::Instance)
}

/// Returns the definition to use as the secondary annotation for an overridden symbol.
fn symbol_definition<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Option<Definition<'db>> {
    let use_def_map = use_def_map(db, scope);
    use_def_map
        .end_of_scope_symbol_declarations(symbol)
        .find_map(|declaration| declaration.declaration.definition())
        .or_else(|| {
            use_def_map
                .end_of_scope_symbol_bindings(symbol)
                .find_map(|binding| binding.binding.definition())
        })
}

/// Reports an invalid override between a class variable and an instance variable.
fn report_invalid_attribute_override<'db>(
    context: &InferContext<'db, '_>,
    member: &Name,
    subclass_definition: Definition<'db>,
    superclass: ClassType<'db>,
    superclass_definition: Option<Definition<'db>>,
    subclass_kind: VariableKind,
    superclass_kind: VariableKind,
) {
    let db = context.db();

    let Some(builder) = context.report_lint(
        &INVALID_ATTRIBUTE_OVERRIDE,
        subclass_definition.focus_range(db, context.module()),
    ) else {
        return;
    };

    let superclass_name = superclass.name(db);
    let superclass_member = format!("{superclass_name}.{member}");
    let subclass_kind = subclass_kind.description();
    let superclass_kind = superclass_kind.description();

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Invalid override of attribute `{member}`"));
    diagnostic.set_primary_message(format_args!(
        "{subclass_kind} cannot override {superclass_kind} `{superclass_member}`"
    ));
    diagnostic.info("This violates the Liskov Substitution Principle");

    if let Some(superclass_definition) = superclass_definition
        && superclass_definition.file(db) == context.file()
    {
        diagnostic.annotate(
            Annotation::secondary(
                context.span(superclass_definition.focus_range(db, context.module())),
            )
            .message(format_args!(
                "{superclass_kind} `{superclass_member}` declared here"
            )),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum MethodKind<'db> {
    Synthesized(CodeGeneratorKind<'db>),
    #[default]
    NotSynthesized,
}

fn is_constructor_like_method(name: &str) -> bool {
    matches!(
        name,
        "__init__" | "__new__" | "__post_init__" | "__init_subclass__"
    )
}

bitflags! {
    /// Bitflags representing which override-related rules have been enabled.
    #[derive(Default, Debug, Copy, Clone)]
    struct OverrideRulesConfig: u16 {
        const LISKOV_METHODS = 1 << 0;
        const LISKOV_ATTRIBUTES = 1 << 1;
        const EXPLICIT_OVERRIDE = 1 << 2;
        const FINAL_METHOD_OVERRIDDEN = 1 << 3;
        const INVALID_NAMED_TUPLE = 1 << 4;
        const NAMED_TUPLE_FIELD_OVERRIDE = 1 << 5;
        const INVALID_DATACLASS = 1 << 6;
        const FINAL_VARIABLE_OVERRIDDEN = 1 << 7;
        const INVALID_ENUM_VALUE = 1 << 8;
        const MISSING_OVERRIDE_DECORATOR = 1 << 9;
    }
}

impl From<&InferContext<'_, '_>> for OverrideRulesConfig {
    fn from(value: &InferContext<'_, '_>) -> Self {
        let db = value.db();
        let rule_selection = db.rule_selection(value.file());

        let mut config = OverrideRulesConfig::empty();

        if rule_selection.is_enabled(LintId::of(&INVALID_METHOD_OVERRIDE)) {
            config |= OverrideRulesConfig::LISKOV_METHODS;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_ATTRIBUTE_OVERRIDE)) {
            config |= OverrideRulesConfig::LISKOV_ATTRIBUTES;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_EXPLICIT_OVERRIDE)) {
            config |= OverrideRulesConfig::EXPLICIT_OVERRIDE;
        }
        if rule_selection.is_enabled(LintId::of(&MISSING_OVERRIDE_DECORATOR)) {
            config |= OverrideRulesConfig::MISSING_OVERRIDE_DECORATOR;
        }
        if rule_selection.is_enabled(LintId::of(&OVERRIDE_OF_FINAL_METHOD)) {
            config |= OverrideRulesConfig::FINAL_METHOD_OVERRIDDEN;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_NAMED_TUPLE)) {
            config |= OverrideRulesConfig::INVALID_NAMED_TUPLE;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_NAMED_TUPLE_OVERRIDE)) {
            config |= OverrideRulesConfig::NAMED_TUPLE_FIELD_OVERRIDE;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_DATACLASS)) {
            config |= OverrideRulesConfig::INVALID_DATACLASS;
        }
        if rule_selection.is_enabled(LintId::of(&OVERRIDE_OF_FINAL_VARIABLE)) {
            config |= OverrideRulesConfig::FINAL_VARIABLE_OVERRIDDEN;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_ASSIGNMENT)) {
            config |= OverrideRulesConfig::INVALID_ENUM_VALUE;
        }

        config
    }
}

impl OverrideRulesConfig {
    const fn no_rules_enabled(self) -> bool {
        self.is_empty()
    }

    const fn check_method_liskov_violations(self) -> bool {
        self.contains(OverrideRulesConfig::LISKOV_METHODS)
    }

    const fn check_attribute_liskov_violations(self) -> bool {
        self.contains(OverrideRulesConfig::LISKOV_ATTRIBUTES)
    }

    const fn check_liskov_violations(self) -> bool {
        self.contains(OverrideRulesConfig::LISKOV_METHODS)
            || self.contains(OverrideRulesConfig::LISKOV_ATTRIBUTES)
    }

    const fn check_final_method_overridden(self) -> bool {
        self.contains(OverrideRulesConfig::FINAL_METHOD_OVERRIDDEN)
    }

    const fn check_missing_overrides(self) -> bool {
        self.contains(OverrideRulesConfig::MISSING_OVERRIDE_DECORATOR)
    }

    const fn check_invalid_named_tuple_definitions(self) -> bool {
        self.contains(OverrideRulesConfig::INVALID_NAMED_TUPLE)
    }

    const fn check_invalid_named_tuple_field_overrides(self) -> bool {
        self.contains(OverrideRulesConfig::NAMED_TUPLE_FIELD_OVERRIDE)
    }

    const fn check_invalid_dataclasses(self) -> bool {
        self.contains(OverrideRulesConfig::INVALID_DATACLASS)
    }

    const fn check_final_variable_overridden(self) -> bool {
        self.contains(OverrideRulesConfig::FINAL_VARIABLE_OVERRIDDEN)
    }
}

fn check_explicit_overrides<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    subclass_scope: ScopeId<'db>,
    class: ClassType<'db>,
) {
    let db = context.db();
    let Some(definition) = invalid_explicit_override_definition(context, member, subclass_scope)
    else {
        return;
    };

    let Some(builder) = context.report_lint(&INVALID_EXPLICIT_OVERRIDE, definition.focus_range)
    else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Method `{}` is decorated with `@override` but does not override anything",
        member.name
    ));
    if let Some(decorator_span) = definition.focus_override_decorator_span {
        diagnostic.annotate(Annotation::secondary(decorator_span));
    }
    diagnostic.info(format_args!(
        "No `{member}` definitions were found on any superclasses of `{class}`",
        member = member.name,
        class = class.name(db)
    ));
}

/// Facts extracted for one local definition of the member under analysis.
#[derive(Debug)]
struct LocalOverrideDefinition {
    /// Range to use as the primary diagnostic location.
    ///
    /// This is usually the function name. For an overloaded function, it points to the
    /// implementation in a source file, or the first overload in a stub.
    focus_range: FileRange,
    /// Whether any overload or implementation in this local function has `@override`.
    ///
    /// `invalid-explicit-override` treats `@override` as explicit even when it appears on a
    /// non-focused overload, for example on the first overload in a source file where the
    /// diagnostic itself points at the implementation.
    any_definition_has_override_decorator: bool,
    /// Whether the definition selected as the diagnostic target has `@override`.
    ///
    /// `missing-override-decorator` uses this instead of `any_definition_has_override_decorator`
    /// so that a misplaced `@override` on another overload still reports an error on the
    /// implementation.
    focus_definition_has_override_decorator: bool,
    /// Span of the `@override` decorator on the focused definition, if present.
    ///
    /// This lets `invalid-explicit-override` underline the decorator separately from the function
    /// name. It is absent when only a non-focused overload has `@override`.
    focus_override_decorator_span: Option<Span>,
}

impl LocalOverrideDefinition {
    fn from_function<'db>(
        db: &'db dyn Db,
        function: FunctionType<'db>,
        in_stub: bool,
        module: &ParsedModuleRef,
    ) -> Self {
        let focus_definition = overriding_definition(db, function, in_stub);

        Self {
            focus_range: focus_definition.focus_range(db, module),
            any_definition_has_override_decorator: function
                .has_known_decorator(db, FunctionDecorators::OVERRIDE),
            focus_definition_has_override_decorator: focus_definition
                .has_known_decorator(db, FunctionDecorators::OVERRIDE),
            focus_override_decorator_span: focus_definition
                .find_known_decorator_span(db, KnownFunction::Override),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MissingOverrideTarget<'db> {
    superclass: ClassType<'db>,
    /// The source definition for the overridden superclass member, if one is available.
    definition: Option<Definition<'db>>,
}

impl<'db> MissingOverrideTarget<'db> {
    fn for_superclass(
        db: &'db dyn Db,
        superclass: ClassType<'db>,
        superclass_scope: ScopeId<'db>,
        superclass_symbol_id: Option<ScopedSymbolId>,
    ) -> Self {
        let definition =
            superclass_symbol_id.and_then(|id| symbol_definition(db, superclass_scope, id));

        Self {
            superclass,
            definition,
        }
    }
}

fn check_missing_overrides<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    subclass_scope: ScopeId<'db>,
    target: MissingOverrideTarget<'db>,
) {
    let db = context.db();

    let Some(definition) = missing_override_definition(context, member, subclass_scope) else {
        return;
    };

    let Some(builder) = context.report_lint(&MISSING_OVERRIDE_DECORATOR, definition.focus_range)
    else {
        return;
    };

    let MissingOverrideTarget {
        superclass,
        definition: superclass_definition,
    } = target;
    let superclass_name = superclass.name(db);
    let superclass_member = format!("{superclass_name}.{}", member.name);
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Method `{}` overrides `{superclass_member}` but is not decorated with `@override`",
        member.name
    ));
    diagnostic.info("Decorate the method with `@typing.override` to make the override explicit");

    if let Some(superclass_definition) = superclass_definition
        && superclass_definition.file(db) == context.file()
    {
        diagnostic.annotate(
            Annotation::secondary(
                context.span(superclass_definition.focus_range(db, context.module())),
            )
            .message(format_args!("`{superclass_member}` defined here")),
        );
    }
}

fn invalid_explicit_override_definition<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    subclass_scope: ScopeId<'db>,
) -> Option<LocalOverrideDefinition> {
    extract_local_override_definitions(context, member, subclass_scope)
        .into_iter()
        .find(|definition| definition.any_definition_has_override_decorator)
}

fn missing_override_definition<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    subclass_scope: ScopeId<'db>,
) -> Option<LocalOverrideDefinition> {
    extract_local_override_definitions(context, member, subclass_scope)
        .into_iter()
        .find(|definition| !definition.focus_definition_has_override_decorator)
}

/// Extract function definitions that can carry an `@override` decorator for the class member
/// currently being checked.
///
/// Use functions recovered from the member type when possible, because this preserves overload and
/// property accessor handling. If decorators replaced some subclass definitions with functions from
/// another class or file, recover the local function type from the binding definition so overload
/// metadata is still preserved.
fn extract_local_override_definitions<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    subclass_scope: ScopeId<'db>,
) -> smallvec::SmallVec<[LocalOverrideDefinition; 1]> {
    let db = context.db();
    let in_stub = context.in_stub();
    let module = context.module();
    let member_functions =
        extract_member_functions_from_type(db, member.ty, &member.name, subclass_scope);
    let mut candidates = smallvec::smallvec![];
    let mut seen_function_types = smallvec::SmallVec::<[FunctionType<'db>; 1]>::new();

    for definition in end_of_scope_function_definitions(db, subclass_scope, &member.name) {
        let function = member_functions
            .iter()
            .copied()
            .find(|function| function.contains_definition(db, definition))
            .or_else(|| infer_definition_types(db, definition).function_type(definition));

        let Some(function) = function else {
            continue;
        };

        if seen_function_types.contains(&function) {
            continue;
        }
        candidates.push(LocalOverrideDefinition::from_function(
            db, function, in_stub, module,
        ));
        seen_function_types.push(function);
    }

    // A property with a setter can keep the getter in the member type even though the setter is the
    // end-of-scope binding. Preserve any type-derived functions that the syntactic pass did not see.
    for function in member_functions {
        if !seen_function_types.contains(&function) {
            candidates.push(LocalOverrideDefinition::from_function(
                db, function, in_stub, module,
            ));
        }
    }

    candidates
}

/// Return reachable function definitions that bind `member_name` at the end of `subclass_scope`.
fn end_of_scope_function_definitions<'db>(
    db: &'db dyn Db,
    subclass_scope: ScopeId<'db>,
    member_name: &Name,
) -> smallvec::SmallVec<[Definition<'db>; 1]> {
    let table = place_table(db, subclass_scope);
    let Some(symbol_id) = table.symbol_id(member_name) else {
        return smallvec::smallvec![];
    };

    let use_def = use_def_map(db, subclass_scope);
    let predicates = use_def.predicates();
    let reachability_constraints = use_def.reachability_constraints();

    use_def
        .end_of_scope_symbol_bindings(symbol_id)
        .filter_map(|binding| {
            let definition = binding.binding.definition()?;
            let reachability =
                reachability_constraints.evaluate(db, predicates, binding.reachability_constraint);
            if reachability.is_always_false() || !definition.kind(db).is_function_def() {
                return None;
            }

            Some(definition)
        })
        .collect()
}

/// Extract functions represented by a member type that belong to the member currently being
/// checked. Decorators can replace a function with a function from another class or file, so
/// callers must not use unfiltered functions as diagnostic anchors.
///
/// The same is true for property accessors: a setter or deleter can reuse a getter defined by
/// another class, but override diagnostics should only point at accessors defined by the subclass
/// member under analysis.
fn extract_member_functions_from_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    member_name: &Name,
    member_scope: ScopeId<'db>,
) -> smallvec::SmallVec<[FunctionType<'db>; 1]> {
    let mut functions = smallvec::SmallVec::<[FunctionType<'db>; 1]>::new();
    let mut types: smallvec::SmallVec<[Type<'db>; 1]> = smallvec::smallvec![ty];
    let mut index = 0;

    while let Some(ty) = types.get(index).copied() {
        index += 1;
        match ty {
            Type::PropertyInstance(property) => {
                for accessor in [
                    property.getter(db),
                    property.setter(db),
                    property.deleter(db),
                ]
                .into_iter()
                .flatten()
                {
                    functions.extend(extract_underlying_functions(db, accessor));
                }
            }
            Type::Union(union) => {
                types.extend(union.elements(db).iter().copied());
            }
            _ => functions.extend(extract_underlying_functions(db, ty)),
        }
    }

    functions
        .into_iter()
        .filter(|function| is_local_member_function(db, *function, member_name, member_scope))
        .collect()
}

fn is_local_member_function<'db>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    member_name: &Name,
    member_scope: ScopeId<'db>,
) -> bool {
    function.file(db) == member_scope.file(db)
        && function.definition(db).scope(db) == member_scope
        && function.name(db) == member_name
}

fn overriding_definition<'db>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    in_stub: bool,
) -> OverloadLiteral<'db> {
    let (_, implementation) = function.overloads_and_implementation(db);
    if !in_stub && let Some(implementation) = implementation {
        implementation
    } else {
        function.first_overload_or_implementation(db)
    }
}

/// Extract callable functions represented by a type.
/// These may be defined in files other than the one being checked.
fn extract_underlying_functions<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> smallvec::SmallVec<[FunctionType<'db>; 1]> {
    match ty {
        Type::FunctionLiteral(function) => smallvec::smallvec_inline![function],
        Type::BoundMethod(method) => smallvec::smallvec_inline![method.function(db)],
        Type::PropertyInstance(property) => property.getter(db).map_or_else(
            || smallvec::smallvec![],
            |getter| extract_underlying_functions(db, getter),
        ),
        Type::Union(union) => {
            let mut functions = smallvec::smallvec![];
            for member in union.elements(db) {
                functions.extend(extract_underlying_functions(db, *member));
            }
            functions
        }
        _ => smallvec::smallvec![],
    }
}

fn check_post_init_signature<'db>(
    context: &InferContext<'db, '_>,
    configuration: OverrideRulesConfig,
    class: ClassType<'db>,
    member: &Member<'db>,
    definition: Definition<'db>,
    policy: CodeGeneratorKind<'db>,
) {
    let db = context.db();

    if !configuration.check_invalid_dataclasses() {
        return;
    }
    if member.name != "__post_init__" {
        return;
    }
    let Some((static_class, spec)) = class.static_class_literal(db) else {
        return;
    };

    let init_var_fields = static_class
        .fields(db, spec, policy)
        .iter()
        .filter(|(_, field)| {
            matches!(
                field.kind,
                FieldKind::Dataclass {
                    init_only: true,
                    ..
                }
            )
        });

    let first_parameter = Parameter::positional_only(Some(Name::new_static("self")))
        .with_annotated_type(Type::instance(db, class));

    let following_parameters = init_var_fields.map(|(name, field)| {
        Parameter::positional_only(Some(name.clone())).with_annotated_type(field.declared_ty)
    });

    let parameters =
        Parameters::standard(std::iter::chain([first_parameter], following_parameters));

    let expected_signature = CallableType::single(db, Signature::new(parameters, Type::object()));

    if member
        .ty
        .is_assignable_to(db, Type::Callable(expected_signature))
    {
        return;
    }

    let Some(builder) = context.report_lint(
        &INVALID_DATACLASS,
        definition.focus_range(db, context.module()),
    ) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Invalid `__post_init__` signature for dataclass `{}`",
        class.name(db)
    ));
    diagnostic.info(
        "`__post_init__` methods must accept all `InitVar` fields \
            as positional-only parameters",
    );
}

#[derive(Clone, Copy, Debug)]
enum EnumConstructorMethod {
    New,
    Init,
}

impl EnumConstructorMethod {
    fn name(self) -> &'static str {
        match self {
            Self::New => "__new__",
            Self::Init => "__init__",
        }
    }
}

/// Validates an enum member value against an enum constructor method signature.
///
/// The enum metaclass unpacks tuple values as positional arguments to `__new__` and `__init__`,
/// and passes non-tuple values as a single argument. This function synthesizes
/// a call with the appropriate arguments and reports a diagnostic
/// if the call would fail.
fn check_enum_member_against_constructor_method<'db>(
    context: &InferContext<'db, '_>,
    function: FunctionType<'db>,
    bound_self_type: Type<'db>,
    member_value_type: Type<'db>,
    member_name: &Name,
    definition: Definition<'db>,
    method: EnumConstructorMethod,
) {
    let db = context.db();

    // The enum metaclass unpacks tuple values as positional args:
    //   MEMBER = (a, b, c)  →  __new__(cls, a, b, c) / __init__(self, a, b, c)
    //   MEMBER = x          →  __new__(cls, x) / __init__(self, x)
    let args: Vec<Type<'db>> = if let Type::NominalInstance(instance) = member_value_type {
        if let Some(spec) = instance.tuple_spec(db) {
            if let Tuple::Fixed(fixed) = &*spec {
                fixed.all_elements().to_vec()
            } else {
                // Variable-length tuples: can't determine exact args, skip validation.
                return;
            }
        } else {
            vec![member_value_type]
        }
    } else {
        vec![member_value_type]
    };

    let call_args = CallArguments::positional(args);
    let call_args = call_args.with_self(Some(bound_self_type));

    let constraints = ConstraintSetBuilder::new();
    let result = Type::FunctionLiteral(function)
        .bindings(db)
        .match_parameters(db, &call_args)
        .check_types(db, &constraints, &call_args, TypeContext::default(), &[]);

    if result.is_err() {
        if let Some(builder) = context.report_lint(
            &INVALID_ASSIGNMENT,
            definition.focus_range(db, context.module()),
        ) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Enum member `{member_name}` is incompatible with `{}`",
                method.name(),
            ));
            diagnostic.info(format_args!(
                "Expected compatible arguments for `{}`",
                Type::FunctionLiteral(function).display(db),
            ));
        }
    }
}
