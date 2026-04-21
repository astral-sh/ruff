//! Checks relating to invalid method overrides in subclasses,
//! including (but not limited to) violations of the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

use std::cell::OnceCell;

use bitflags::bitflags;
use ruff_db::diagnostic::Annotation;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use rustc_hash::FxHashSet;

use crate::{
    Db,
    lint::LintId,
    place::{DefinedPlace, Place, PlaceAndQualifiers},
    types::{
        CallableType, ClassBase, ClassLiteral, ClassType, KnownClass, Parameter, Parameters,
        Signature, StaticClassLiteral, Type, TypeContext, TypeQualifiers,
        call::CallArguments,
        class::{CodeGeneratorKind, FieldKind},
        constraints::ConstraintSetBuilder,
        context::InferContext,
        diagnostic::{
            INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_OVERRIDE, INVALID_DATACLASS,
            INVALID_EXPLICIT_OVERRIDE, INVALID_METHOD_OVERRIDE, INVALID_NAMED_TUPLE,
            INVALID_NAMED_TUPLE_OVERRIDE, OVERRIDE_OF_FINAL_METHOD, OVERRIDE_OF_FINAL_VARIABLE,
            report_invalid_method_override, report_overridden_final_method,
            report_overridden_final_variable,
        },
        enums::{EnumMetadata, enum_metadata},
        function::{FunctionDecorators, FunctionType, KnownFunction},
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
pub(super) fn check_class<'db>(context: &InferContext<'db, '_>, class: StaticClassLiteral<'db>) {
    let db = context.db();
    let configuration = OverrideRulesConfig::from(context);
    if configuration.no_rules_enabled() {
        return;
    }

    let class_specialized = class.identity_specialization(db);
    let scope = class.body_scope(db);
    let own_class_members: FxHashSet<_> = all_end_of_scope_members(db, scope).collect();
    let enum_info = enum_metadata(db, class.into());

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

        if CodeGeneratorKind::NamedTuple.matches(db, superclass_literal, superclass_specialization)
        {
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
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
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

    let Some((literal, specialization)) = class.static_class_literal(db) else {
        return;
    };
    let class_kind = CodeGeneratorKind::from_class(db, literal.into(), specialization);

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
                    &member.name
                ));
                diagnostic.info("This will cause the class creation to fail at runtime");
            }
        }
        Some(policy @ CodeGeneratorKind::DataclassLike(_)) => check_post_init_signature(
            context,
            configuration,
            class,
            member,
            *first_reachable_definition,
            policy,
        ),
        Some(CodeGeneratorKind::TypedDict) | None => {}
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
            let skip_type_check = (context.in_stub() && is_ellipsis) || is_auto;

            if !skip_type_check {
                if let Some(init_function) = enum_info.init_function {
                    check_enum_member_against_init(
                        context,
                        init_function,
                        instance_of_class,
                        member_value_type,
                        &member.name,
                        *first_reachable_definition,
                    );
                } else if let Some(expected_type) = enum_info.value_annotation {
                    if !member_value_type.is_assignable_to(db, expected_type) {
                        if let Some(builder) = context.report_lint(
                            &INVALID_ASSIGNMENT,
                            first_reachable_definition.focus_range(db, context.module()),
                        ) {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Enum member `{}` value is not assignable to expected type",
                                &member.name
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
    let mut overridden_final_method = None;
    let mut overridden_final_variable: Option<(ClassType<'db>, Option<Definition<'db>>)> = None;
    let is_private_member = is_mangled_private(member.name.as_str());
    let subclass_variable_kind: OnceCell<Option<VariableKind>> = OnceCell::new();

    // Track the first superclass that defines this method (the "immediate parent" for this method).
    // We need this to check if parent itself already has an LSP violation with an ancestor.
    // If so, we shouldn't report the same violation for the child class.
    let mut immediate_parent_method: Option<(ClassType<'db>, Type<'db>)> = None;
    let mut immediate_parent_variable_kind: Option<(ClassType<'db>, VariableKind)> = None;

    if !is_private_member {
        for class_base in class.iter_mro(db).skip(1) {
            let superclass = match class_base {
                ClassBase::Protocol | ClassBase::Generic => continue,
                ClassBase::Dynamic(_) => {
                    has_dynamic_superclass = true;
                    continue;
                }
                ClassBase::Divergent(_) => {
                    has_dynamic_superclass = true;
                    continue;
                }
                ClassBase::TypedDict => {
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
                method_kind = CodeGeneratorKind::from_class(
                    db,
                    superclass_literal.into(),
                    superclass_specialization,
                )
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
                        )?;

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
                let superclass_variable_kind = superclass_variable_kind(
                    db,
                    superclass.own_class_member(db, None, &member.name).inner,
                    superclass_instance_member,
                );

                if let Some(superclass_variable_kind) = superclass_variable_kind
                    && immediate_parent_variable_kind.is_none()
                {
                    immediate_parent_variable_kind = Some((superclass, superclass_variable_kind));
                }

                if let Some(superclass_variable_kind) = superclass_variable_kind {
                    let subclass_kind = *subclass_variable_kind.get_or_init(|| {
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
                        // declaration instead of introducing a conflicting instance variable.
                        if subclass_kind == VariableKind::Instance
                            && superclass_variable_kind == VariableKind::Class
                            && first_reachable_definition
                                .kind(db)
                                .is_unannotated_assignment()
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
            if matches!(
                &*member.name,
                "__init__" | "__new__" | "__post_init__" | "__init_subclass__"
            ) {
                continue;
            }

            // Synthesized `__replace__` methods on dataclasses are not checked
            if &member.name == "__replace__"
                && matches!(class_kind, Some(CodeGeneratorKind::DataclassLike(_)))
            {
                continue;
            }

            let Some(superclass_type_as_callable) = superclass_type.try_upcast_to_callable(db)
            else {
                continue;
            };

            let superclass_type_as_type = superclass_type_as_callable.into_type(db);

            if type_on_subclass_instance.is_assignable_to(db, superclass_type_as_type) {
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
                    if !immediate_parent_type.is_assignable_to(db, superclass_type_as_type) {
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
                || {
                    type_on_subclass_instance
                        .assignability_error_context(db, superclass_type_as_type)
                },
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

    if !subclass_overrides_superclass_declaration
        && !has_dynamic_superclass
        // accessing `.kind()` here is fine as `definition`
        // will always be a definition in the file currently being checked
        && first_reachable_definition.kind(db).is_function_def()
    {
        check_explicit_overrides(context, member, class);
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

/// Whether an attribute declaration is a class variable or an instance variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Returns the variable kind for a superclass member, excluding final attributes.
fn superclass_variable_kind<'db>(
    db: &'db dyn Db,
    class_member: PlaceAndQualifiers<'db>,
    instance_member: PlaceAndQualifiers<'db>,
) -> Option<VariableKind> {
    if class_member.qualifiers.contains(TypeQualifiers::FINAL) {
        return None;
    }

    variable_kind(db, class_member, instance_member)
}

/// Returns the variable kind for an attribute if it should participate in `ClassVar` override checks.
fn variable_kind<'db>(
    db: &'db dyn Db,
    class_member: PlaceAndQualifiers<'db>,
    instance_member: PlaceAndQualifiers<'db>,
) -> Option<VariableKind> {
    let class_member_ty = class_member.ignore_possibly_undefined()?;
    let instance_member_ty = instance_member.ignore_possibly_undefined()?;

    if !is_variable_like_type(db, class_member_ty) || !is_variable_like_type(db, instance_member_ty)
    {
        return None;
    }

    if class_member.is_class_var() || instance_member.is_class_var() {
        return Some(VariableKind::Class);
    }

    if class_member.qualifiers.contains(TypeQualifiers::FINAL)
        || class_member_ty
            .class_member(db, "__get__".into())
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
    use_def_map(db, scope)
        .end_of_scope_symbol_declarations(symbol)
        .find_map(|declaration| declaration.declaration.definition())
        .or_else(|| {
            use_def_map(db, scope)
                .end_of_scope_symbol_bindings(symbol)
                .find_map(|binding| binding.binding.definition())
        })
}

/// Returns `true` if a type represents a variable-like attribute.
fn is_variable_like_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    match ty {
        Type::FunctionLiteral(_)
        | Type::BoundMethod(_)
        | Type::KnownBoundMethod(_)
        | Type::WrapperDescriptor(_)
        | Type::PropertyInstance(_) => false,
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| is_variable_like_type(db, *element)),
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .all(|element| is_variable_like_type(db, *element)),
        _ => true,
    }
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
    class: ClassType<'db>,
) {
    let db = context.db();
    let underlying_functions = extract_underlying_functions(db, member.ty);
    let Some(functions) = underlying_functions else {
        return;
    };
    let Some(decorated_function) = functions
        .iter()
        .find(|function| function.has_known_decorator(db, FunctionDecorators::OVERRIDE))
    else {
        return;
    };
    let function_literal = if context.in_stub() {
        decorated_function.first_overload_or_implementation(db)
    } else {
        decorated_function.literal(db).last_definition
    };

    let Some(builder) = context.report_lint(
        &INVALID_EXPLICIT_OVERRIDE,
        function_literal.focus_range(db, context.module()),
    ) else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Method `{}` is decorated with `@override` but does not override anything",
        member.name
    ));
    if let Some(decorator_span) =
        function_literal.find_known_decorator_span(db, KnownFunction::Override)
    {
        diagnostic.annotate(Annotation::secondary(decorator_span));
    }
    diagnostic.info(format_args!(
        "No `{member}` definitions were found on any superclasses of `{class}`",
        member = &member.name,
        class = class.name(db)
    ));
}

fn extract_underlying_functions<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<smallvec::SmallVec<[FunctionType<'db>; 1]>> {
    match ty {
        Type::FunctionLiteral(function) => Some(smallvec::smallvec_inline![function]),
        Type::BoundMethod(method) => Some(smallvec::smallvec_inline![method.function(db)]),
        Type::PropertyInstance(property) => extract_underlying_functions(db, property.getter(db)?),
        Type::Union(union) => {
            let mut functions = smallvec::smallvec![];
            for member in union.elements(db) {
                if let Some(mut member_functions) = extract_underlying_functions(db, *member) {
                    functions.append(&mut member_functions);
                }
            }
            if functions.is_empty() {
                None
            } else {
                Some(functions)
            }
        }
        _ => None,
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

    let parameters = Parameters::new(
        db,
        std::iter::chain([first_parameter], following_parameters),
    );

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

/// Validates an enum member value against the enum's `__init__` signature.
///
/// The enum metaclass unpacks tuple values as positional arguments to `__init__`,
/// and passes non-tuple values as a single argument. This function synthesizes
/// a call to `__init__` with the appropriate arguments and reports a diagnostic
/// if the call would fail.
fn check_enum_member_against_init<'db>(
    context: &InferContext<'db, '_>,
    init_function: FunctionType<'db>,
    self_type: Type<'db>,
    member_value_type: Type<'db>,
    member_name: &Name,
    definition: Definition<'db>,
) {
    let db = context.db();

    // The enum metaclass unpacks tuple values as positional args:
    //   MEMBER = (a, b, c)  →  __init__(self, a, b, c)
    //   MEMBER = x          →  __init__(self, x)
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
    let call_args = call_args.with_self(Some(self_type));

    let constraints = ConstraintSetBuilder::new();
    let result = Type::FunctionLiteral(init_function)
        .bindings(db)
        .match_parameters(db, &call_args)
        .check_types(db, &constraints, &call_args, TypeContext::default(), &[]);

    if result.is_err() {
        if let Some(builder) = context.report_lint(
            &INVALID_ASSIGNMENT,
            definition.focus_range(db, context.module()),
        ) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Enum member `{member_name}` is incompatible with `__init__`",
            ));
            diagnostic.info(format_args!(
                "Expected compatible arguments for `{}`",
                Type::FunctionLiteral(init_function).display(db),
            ));
        }
    }
}
