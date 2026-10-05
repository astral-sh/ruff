//! Continues per-member override validation after descriptor lookup. `check_remaining_with` retains
//! the selected superclass and diagnostic state while `RemainingOverrideEffects` performs semantic
//! children and reports.

use std::convert::Infallible;

use ruff_db::diagnostic::Annotation;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{place_table, use_def_map};

use super::member_entry::OverrideMemberRequest;
use super::{
    EnumConstructorMethod, MethodKind, MissingOverrideTarget, VariableKind, bind_new_for_override,
    check_enum_member_against_constructor_method, check_explicit_overrides,
    check_missing_overrides, effective_superclass_variable_kind, is_constructor_like_method,
    is_function_definition, is_inherited_method_violation, lookup_override_member,
    method_override_types, report_invalid_attribute_override, symbol_definition, variable_kind,
};
use crate::place::{DefinedPlace, Place, PlaceAndQualifiers};
use crate::types::class::CodeGeneratorKind;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    INVALID_ASSIGNMENT, INVALID_NAMED_TUPLE_OVERRIDE, report_invalid_method_override,
    report_overridden_final_method, report_overridden_final_variable,
};
use crate::types::enums::EnumMetadata;
use crate::types::function::{FunctionDecorators, FunctionType};
use crate::types::list_members::{MemberWithDefinition, extract_underlying_functions};
use crate::types::member::Member;
use crate::types::{
    ClassBase, ClassLiteral, ClassType, KnownClass, Specialization, StaticClassLiteral, Type,
    TypeQualifiers,
};

pub(in crate::types) enum OverrideReport<'a, 'db> {
    NamedTuple(ClassType<'db>, Option<Definition<'db>>),
    EnumValue {
        expected: Type<'db>,
        actual: Type<'db>,
    },
    Attribute {
        superclass: ClassType<'db>,
        definition: Option<Definition<'db>>,
        subclass_kind: VariableKind,
        superclass_kind: VariableKind,
    },
    Method {
        function: FunctionType<'db>,
        superclass: ClassType<'db>,
        superclass_type: Type<'db>,
        kind: MethodKind<'db>,
        subclass_override: Type<'db>,
        superclass_override: Type<'db>,
    },
    FinalMethod(ClassType<'db>, &'a [FunctionType<'db>]),
    FinalVariable(ClassType<'db>, Option<Definition<'db>>),
}

pub(in crate::types) trait RemainingOverrideEffects<'db> {
    type Error;

    /// Runs local work using quoted work units and requested storage bytes.
    /// Controlled execution admits both costs before calling `action`; `None` means an
    /// overflowed quotation and refuses the action. Ordinary execution calls it directly.
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Runs a scalar operation with one work unit and `size_of::<T>()` result bytes.
    /// Variable work and transitive storage require separate admission; this fixed charge
    /// does not cover traversal, allocation, or cleanup of an owned collection.
    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(Some(1), Some(size_of::<T>()), action).await
    }

    async fn named_tuple_conflict(
        &self,
        literal: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<Option<(ClassType<'db>, Option<Definition<'db>>)>, Self::Error>;
    async fn report(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        report: OverrideReport<'_, 'db>,
    ) -> Result<(), Self::Error>;
    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;
    async fn enum_value(
        &self,
        info: &EnumMetadata<'db>,
        name: &Name,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn enum_auto(&self, info: &EnumMetadata<'db>, name: &Name) -> Result<bool, Self::Error>;
    async fn is_ellipsis(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    async fn in_stub(&self) -> Result<bool, Self::Error>;
    async fn enum_constructor(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        function: FunctionType<'db>,
        receiver: Type<'db>,
        value: Type<'db>,
        method: EnumConstructorMethod,
    ) -> Result<(), Self::Error>;
    async fn static_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
    async fn scope_symbol(
        &self,
        literal: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>), Self::Error>;
    async fn synthesized_member(
        &self,
        literal: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &Name,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn code_generator(
        &self,
        literal: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
    async fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Member<'db>, Self::Error>;
    async fn lookup_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn symbol_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<Option<Definition<'db>>, Self::Error>;
    async fn first_declaration(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<Option<Definition<'db>>, Self::Error>;
    async fn functions(
        &self,
        ty: Type<'db>,
    ) -> Result<SmallVec<[FunctionType<'db>; 1]>, Self::Error>;
    async fn has_decorator(
        &self,
        function: FunctionType<'db>,
        decorator: FunctionDecorators,
    ) -> Result<bool, Self::Error>;
    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Self::Error>;
    async fn effective_variable_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Option<VariableKind>, Self::Error>;
    async fn variable_kind(
        &self,
        own: PlaceAndQualifiers<'db>,
        instance: PlaceAndQualifiers<'db>,
    ) -> Result<Option<VariableKind>, Self::Error>;
    async fn is_subclass(
        &self,
        child: ClassType<'db>,
        parent: ClassType<'db>,
    ) -> Result<bool, Self::Error>;
    async fn override_types(
        &self,
        class: ClassType<'db>,
        name: &Name,
        subclass: Type<'db>,
        superclass: Type<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
    async fn assignable(&self, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
    async fn inherited_violation(
        &self,
        bases: &[ClassBase<'db>],
        owner: ClassType<'db>,
        superclass: ClassType<'db>,
        superclass_type: Type<'db>,
        name: &Name,
    ) -> Result<bool, Self::Error>;
    async fn fallback_member(
        &self,
        class: KnownClass,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn missing_override(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        target: MissingOverrideTarget<'db>,
    ) -> Result<(), Self::Error>;
    async fn explicit_override(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
    ) -> Result<(), Self::Error>;
}

/// Checks inherited-field conflicts, Enum values, final members, override compatibility, and
/// override decorators for `request.member` after the generator-specific declaration checks in
/// [`super::member_entry::check_resolved_class_declaration_with`].
///
/// The resolved inputs belong to the same request: `instance_of_class` is the instance type of
/// `request.class`, `subclass_instance_member` is the override lookup result for `request.member`
/// on that class, and `type_on_subclass_instance` is that result's defined type. `literal` and
/// `class_kind` describe that same class.
pub(in crate::types) async fn check_remaining_with<'db, E: RemainingOverrideEffects<'db>>(
    request: OverrideMemberRequest<'_, 'db>,
    instance_of_class: Type<'db>,
    subclass_instance_member: PlaceAndQualifiers<'db>,
    type_on_subclass_instance: Type<'db>,
    literal: StaticClassLiteral<'db>,
    class_kind: Option<CodeGeneratorKind<'db>>,
    effects: &E,
) -> Result<(), E::Error> {
    let OverrideMemberRequest {
        configuration,
        enum_info,
        class,
        bases,
        member,
        ..
    } = effects.step(|| request).await?;
    let MemberWithDefinition {
        member,
        first_reachable_definition,
    } = member;

    if effects
        .step(|| configuration.check_invalid_named_tuple_field_overrides())
        .await?
        && let Some((superclass, declaration)) =
            effects.named_tuple_conflict(literal, &member.name).await?
    {
        effects
            .report(
                request,
                effects
                    .step(|| OverrideReport::NamedTuple(superclass, declaration))
                    .await?,
            )
            .await?;
    }

    // Check for invalid Enum member values.
    if let Some(enum_info) = enum_info {
        if effects
            .local(
                member.name.len().checked_add(1),
                Some(size_of::<bool>()),
                || member.name != "_value_",
            )
            .await?
            && matches!(
                effects.definition_kind(*first_reachable_definition).await?,
                DefinitionKind::Assignment(_) | DefinitionKind::AnnotatedAssignment(_)
            )
        {
            // Use the value type from `EnumMetadata` rather than `member.ty`, because
            // for annotated assignments like `X: Final = "value"`, the member may come
            // from the declaration chain (where `ty` is the declared type, e.g. `Unknown`)
            // rather than the binding chain (where `ty` is the actual value type).
            let Some(member_value_type) = effects.enum_value(enum_info, &member.name).await? else {
                return Ok(());
            };

            // TODO ideally this would be a syntactic check that only matches on literal `...`
            // in the source, rather than matching on the type. But this would require storing
            // additional information in `EnumMetadata`.
            let is_ellipsis = effects.is_ellipsis(member_value_type).await?;
            // `auto()` values are computed at runtime by the enum metaclass,
            // so we can't validate them against _value_ or __init__ at the type level.
            let is_auto = effects.enum_auto(enum_info, &member.name).await?;
            let skip_type_check = (effects.in_stub().await? && is_ellipsis)
                || is_auto
                || effects
                    .step(|| enum_info.value_construction.metaclass_may_transform_values)
                    .await?;

            if !skip_type_check {
                if let Some(new_function) = effects
                    .step(|| enum_info.value_construction.new.function())
                    .await?
                {
                    effects
                        .enum_constructor(
                            request,
                            new_function,
                            effects.step(|| Type::from(class)).await?,
                            member_value_type,
                            EnumConstructorMethod::New,
                        )
                        .await?;
                }

                if let Some(init_function) = effects
                    .step(|| enum_info.value_construction.init.function())
                    .await?
                {
                    effects
                        .enum_constructor(
                            request,
                            init_function,
                            instance_of_class,
                            member_value_type,
                            EnumConstructorMethod::Init,
                        )
                        .await?;
                } else if effects
                    .step(|| {
                        enum_info
                            .value_construction
                            .can_validate_with_value_annotation()
                    })
                    .await?
                    && let Some(expected_type) =
                        effects.step(|| enum_info.value_annotation_type()).await?
                    && !effects.assignable(member_value_type, expected_type).await?
                {
                    effects
                        .report(
                            request,
                            effects
                                .step(|| OverrideReport::EnumValue {
                                    expected: expected_type,
                                    actual: member_value_type,
                                })
                                .await?,
                        )
                        .await?;
                }
            }
        }
    }

    let mut subclass_overrides_superclass_declaration = effects.step(|| false).await?;
    let mut has_dynamic_superclass = effects.step(|| false).await?;
    let mut has_typeddict_in_mro = effects.step(|| false).await?;
    let mut liskov_diagnostic_emitted = effects.step(|| false).await?;
    let mut missing_override_target: Option<MissingOverrideTarget<'db>> =
        effects.step(|| None).await?;
    let mut overridden_final_method: Option<(ClassType<'db>, SmallVec<[FunctionType<'db>; 1]>)> =
        effects.step(|| None).await?;
    let mut overridden_final_variable: Option<(ClassType<'db>, Option<Definition<'db>>)> =
        effects.step(|| None).await?;
    let is_private_member = effects
        .local(
            member.name.len().checked_add(1),
            Some(size_of::<bool>()),
            || is_mangled_private(member.name.as_str()),
        )
        .await?;
    let mut subclass_variable_kind: Option<Option<VariableKind>> = effects.step(|| None).await?;

    // Track the first superclass that defines this method so we can distinguish inherited
    // conflicts from violations introduced by the child.
    let mut inherited_method_owner = effects.step(|| None).await?;
    let mut immediate_parent_variable_kind: Option<(ClassType<'db>, VariableKind)> =
        effects.step(|| None).await?;

    if !is_private_member {
        let mut base_cursor = effects.step(|| bases.iter()).await?;
        while let Some(class_base) = effects.step(|| base_cursor.next().copied()).await? {
            let superclass = match class_base {
                ClassBase::Protocol | ClassBase::Generic => continue,
                ClassBase::Any | ClassBase::Dynamic(_) => {
                    has_dynamic_superclass = effects.step(|| true).await?;
                    continue;
                }
                ClassBase::Divergent(_) => {
                    has_dynamic_superclass = effects.step(|| true).await?;
                    continue;
                }
                ClassBase::TypedDict(_) => {
                    has_typeddict_in_mro = effects.step(|| true).await?;
                    continue;
                }
                ClassBase::Class(class) => class,
            };

            // If the member is not defined on the class itself, skip it. Functional named tuples
            // have synthesized members but no class body in which to look up their definitions.
            let (superclass_symbol, method_kind) =
                if let Some((superclass_literal, superclass_specialization)) =
                    effects.static_identity(superclass).await?
                {
                    let (superclass_scope, symbol) = effects
                        .scope_symbol(superclass_literal, &member.name)
                        .await?;
                    if let Some((id, is_bound_or_declared)) = symbol {
                        if !is_bound_or_declared {
                            continue;
                        }
                        effects
                            .step(|| (Some((superclass_scope, id)), MethodKind::default()))
                            .await?
                    } else {
                        if effects
                            .synthesized_member(
                                superclass_literal,
                                superclass_specialization,
                                &member.name,
                            )
                            .await?
                            .is_none()
                        {
                            continue;
                        }
                        let kind = effects.code_generator(superclass_literal).await?;
                        effects
                            .step(|| (None, kind.map(MethodKind::Synthesized).unwrap_or_default()))
                            .await?
                    }
                } else if matches!(
                    effects.class_literal(superclass).await?,
                    ClassLiteral::DynamicNamedTuple(_)
                ) && !effects
                    .own_member(superclass, &member.name)
                    .await?
                    .is_undefined()
                {
                    effects
                        .step(|| (None, MethodKind::Synthesized(CodeGeneratorKind::NamedTuple)))
                        .await?
                } else {
                    continue;
                };

            let superclass_instance_member =
                effects.lookup_member(superclass, &member.name).await?;
            let Place::Defined(DefinedPlace {
                ty: superclass_type,
                ..
            }) = superclass_instance_member.place
            else {
                // If not defined on any superclass, no point in continuing to walk up the MRO
                break;
            };

            subclass_overrides_superclass_declaration = effects.step(|| true).await?;

            // Record the first overridden superclass member that is subject to the missing override
            // decorator check so that we can later confirm that the overriding definition is indeed
            // marked with the decorator.
            if effects
                .step(|| {
                    configuration.check_missing_overrides() && missing_override_target.is_none()
                })
                .await?
                && !effects
                    .local(
                        member
                            .name
                            .len()
                            .checked_add(1)
                            .and_then(|n| n.checked_mul(4)),
                        Some(size_of::<bool>()),
                        || is_constructor_like_method(&member.name),
                    )
                    .await?
            {
                let definition = match superclass_symbol {
                    Some((scope, symbol)) => effects.symbol_definition(scope, symbol).await?,
                    None => None,
                };
                missing_override_target = effects
                    .step(|| {
                        Some(MissingOverrideTarget {
                            superclass,
                            definition,
                        })
                    })
                    .await?;
            }

            inherited_method_owner = effects
                .step(|| inherited_method_owner.or(Some(superclass)))
                .await?;

            if effects
                .step(|| {
                    (configuration.check_final_method_overridden()
                        && overridden_final_method.is_none())
                        || (configuration.check_final_variable_overridden()
                            && overridden_final_variable.is_none())
                })
                .await?
            {
                let own_class_member = effects.own_member(superclass, &member.name).await?;

                if effects
                    .step(|| {
                        configuration.check_final_method_overridden()
                            && overridden_final_method.is_none()
                    })
                    .await?
                    && let Some((superclass_scope, superclass_symbol_id)) = superclass_symbol
                    && let Some(ty) = effects
                        .step(|| own_class_member.ignore_possibly_undefined())
                        .await?
                {
                    // TODO: `@final` should be more like a type qualifier:
                    // we should also recognise `@final`-decorated methods that don't end up
                    // as being function- or property-types (because they're wrapped by other
                    // decorators that transform the type into something else).
                    let underlying_functions = effects.functions(ty).await?;
                    let mut functions = effects.step(|| underlying_functions.iter()).await?;
                    let mut is_final = effects.step(|| false).await?;
                    while let Some(function) = effects.step(|| functions.next().copied()).await? {
                        if effects
                            .has_decorator(function, FunctionDecorators::FINAL)
                            .await?
                        {
                            is_final = effects.step(|| true).await?;
                            break;
                        }
                    }
                    if is_final
                        && effects
                            .is_function_definition(superclass_scope, superclass_symbol_id)
                            .await?
                    {
                        overridden_final_method = effects
                            .step(|| Some((superclass, underlying_functions)))
                            .await?;
                    }
                }

                if effects
                    .step(|| {
                        configuration.check_final_variable_overridden()
                            && overridden_final_variable.is_none()
                            && own_class_member
                                .qualifiers()
                                .contains(TypeQualifiers::FINAL)
                    })
                    .await?
                {
                    // Find the declaration definition in the superclass for the secondary
                    // annotation.
                    let superclass_definition = match superclass_symbol {
                        Some((scope, id)) => effects.first_declaration(scope, id).await?,
                        None => None,
                    };
                    overridden_final_variable = effects
                        .step(|| Some((superclass, superclass_definition)))
                        .await?;
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

            if !effects
                .step(|| configuration.check_liskov_violations())
                .await?
            {
                continue;
            }

            if effects
                .step(|| configuration.check_attribute_liskov_violations())
                .await?
            {
                if let Some(superclass_variable_kind) = effects
                    .effective_variable_kind(superclass, &member.name)
                    .await?
                {
                    if immediate_parent_variable_kind.is_none() {
                        immediate_parent_variable_kind = effects
                            .step(|| Some((superclass, superclass_variable_kind)))
                            .await?;
                    }

                    let subclass_kind = if let Some(kind) = subclass_variable_kind {
                        kind
                    } else {
                        let own = effects.own_member(class, &member.name).await?;
                        let kind = effects
                            .variable_kind(own.inner, subclass_instance_member)
                            .await?;
                        subclass_variable_kind = effects.step(|| Some(kind)).await?;
                        kind
                    };

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
                                effects.definition_kind(*first_reachable_definition).await?,
                                DefinitionKind::Assignment(_)
                                    | DefinitionKind::AugmentedAssignment(_)
                            )
                        {
                            continue;
                        }

                        if let Some((immediate_parent, immediate_parent_kind)) =
                            immediate_parent_variable_kind
                            && immediate_parent != superclass
                            && effects.is_subclass(immediate_parent, superclass).await?
                            && immediate_parent_kind != superclass_variable_kind
                        {
                            continue;
                        }

                        let superclass_definition = match superclass_symbol {
                            Some((scope, id)) => effects.symbol_definition(scope, id).await?,
                            None => None,
                        };
                        effects
                            .report(
                                request,
                                effects
                                    .step(|| OverrideReport::Attribute {
                                        superclass,
                                        definition: superclass_definition,
                                        subclass_kind,
                                        superclass_kind: superclass_variable_kind,
                                    })
                                    .await?,
                            )
                            .await?;
                        liskov_diagnostic_emitted = effects.step(|| true).await?;
                        continue;
                    }
                }
            }

            if !effects
                .step(|| configuration.check_method_liskov_violations())
                .await?
            {
                continue;
            }

            let Type::FunctionLiteral(subclass_function) = member.ty else {
                continue;
            };

            // Constructor signatures may differ unless `@override` requests compatibility.
            if effects
                .local(
                    member
                        .name
                        .len()
                        .checked_add(1)
                        .and_then(|n| n.checked_mul(4)),
                    Some(size_of::<bool>()),
                    || is_constructor_like_method(&member.name),
                )
                .await?
                && !effects
                    .has_decorator(subclass_function, FunctionDecorators::OVERRIDE)
                    .await?
            {
                continue;
            }

            // Synthesized `__replace__` methods on dataclasses are not checked
            if effects
                .local(
                    member.name.len().checked_add(1),
                    Some(size_of::<bool>()),
                    || {
                        &member.name == "__replace__"
                            && class_kind.is_some_and(CodeGeneratorKind::is_dataclass_like)
                    },
                )
                .await?
            {
                continue;
            }

            let Some((subclass_override_type, superclass_override_type)) = effects
                .override_types(
                    class,
                    &member.name,
                    type_on_subclass_instance,
                    superclass_type,
                )
                .await?
            else {
                continue;
            };

            if effects
                .assignable(subclass_override_type, superclass_override_type)
                .await?
            {
                continue;
            }

            // Do not repeat a violation that already exists in the parent's hierarchy.
            // See: https://github.com/astral-sh/ty/issues/2000
            if let Some(method_owner) = inherited_method_owner
                && method_owner != superclass
                && effects
                    .inherited_violation(
                        bases,
                        method_owner,
                        superclass,
                        superclass_type,
                        &member.name,
                    )
                    .await?
            {
                continue;
            }

            effects
                .report(
                    request,
                    effects
                        .step(|| OverrideReport::Method {
                            function: subclass_function,
                            superclass,
                            superclass_type,
                            kind: method_kind,
                            subclass_override: subclass_override_type,
                            superclass_override: superclass_override_type,
                        })
                        .await?,
                )
                .await?;
            liskov_diagnostic_emitted = effects.step(|| true).await?;
        }
    }

    if !subclass_overrides_superclass_declaration && !has_dynamic_superclass {
        if has_typeddict_in_mro {
            if !effects
                .fallback_member(KnownClass::TypedDictFallback, &member.name)
                .await?
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = effects.step(|| true).await?;
            }
        } else if class_kind == Some(CodeGeneratorKind::NamedTuple) {
            if !effects
                .fallback_member(KnownClass::NamedTupleFallback, &member.name)
                .await?
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = effects.step(|| true).await?;
            }
        }
    }

    if let Some(target) = missing_override_target
        && effects
            .definition_kind(*first_reachable_definition)
            .await?
            .is_function_def()
    {
        effects.missing_override(request, target).await?;
    }

    if !subclass_overrides_superclass_declaration
        && !has_dynamic_superclass
        && (
            // `first_reachable_definition` belongs to the file currently being checked,
            // so reading its kind does not inspect another module's definition here.
            effects
                .definition_kind(*first_reachable_definition)
                .await?
                .is_function_def()
        )
    {
        effects.explicit_override(request).await?;
    }

    if let Some((superclass, superclass_method)) = overridden_final_method {
        effects
            .report(
                request,
                effects
                    .step(|| OverrideReport::FinalMethod(superclass, &superclass_method))
                    .await?,
            )
            .await?;
    }

    if let Some((superclass, superclass_definition)) = overridden_final_variable {
        effects
            .report(
                request,
                effects
                    .step(|| OverrideReport::FinalVariable(superclass, superclass_definition))
                    .await?,
            )
            .await?;
    }
    Ok(())
}

pub(super) struct OrdinaryRemainingEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

impl<'db> RemainingOverrideEffects<'db> for OrdinaryRemainingEffects<'_, 'db, '_> {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn named_tuple_conflict(
        &self,
        literal: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<Option<(ClassType<'db>, Option<Definition<'db>>)>, Self::Error> {
        Ok(crate::types::legacy_inline(
            super::namedtuple_fields::conflicting_named_tuple_field_with(
                literal,
                name,
                &super::namedtuple_fields::OrdinaryNamedTupleFieldEffects {
                    db: self.context.db(),
                },
            ),
        ))
    }

    async fn report(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        report: OverrideReport<'_, 'db>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let env = &context.program_environment();
        let MemberWithDefinition {
            member,
            first_reachable_definition,
        } = request.member;
        match report {
            OverrideReport::NamedTuple(superclass, overridden_field_declaration) => {
                if let Some(builder) = context.report_lint(
                    &INVALID_NAMED_TUPLE_OVERRIDE,
                    first_reachable_definition.focus_range(db, context.module()),
                ) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Cannot override NamedTuple field `{}` inherited from `{}`",
                        member.name,
                        superclass.name(db)
                    ));
                    diagnostic.info("Subclass members are not allowed to reuse inherited NamedTuple field names");
                    if let Some(first_declaration) = overridden_field_declaration
                        && first_declaration.file(db) == context.file()
                    {
                        diagnostic.annotate(
                            Annotation::secondary(
                                context
                                    .span(first_declaration.kind(db).full_range(context.module())),
                            )
                            .message(format_args!(
                                "Inherited NamedTuple field `{}` declared here",
                                member.name
                            )),
                        );
                    }
                }
            }
            OverrideReport::EnumValue { expected, actual } => {
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
                        expected.display(db, env),
                        actual.display(db, env)
                    ));
                }
            }
            OverrideReport::Attribute {
                superclass,
                definition,
                subclass_kind,
                superclass_kind,
            } => {
                report_invalid_attribute_override(
                    context,
                    &member.name,
                    *first_reachable_definition,
                    superclass,
                    definition,
                    subclass_kind,
                    superclass_kind,
                );
            }
            OverrideReport::Method {
                function,
                superclass,
                superclass_type,
                kind,
                subclass_override,
                superclass_override,
            } => {
                report_invalid_method_override(
                    context,
                    &member.name,
                    request.class,
                    *first_reachable_definition,
                    function,
                    superclass,
                    superclass_type,
                    kind,
                    || subclass_override.assignability_error_context(db, env, superclass_override),
                );
            }
            OverrideReport::FinalMethod(superclass, functions) => {
                report_overridden_final_method(
                    context,
                    &member.name,
                    *first_reachable_definition,
                    member.ty,
                    superclass,
                    request.class,
                    functions,
                );
            }
            OverrideReport::FinalVariable(superclass, definition) => {
                report_overridden_final_variable(
                    context,
                    &member.name,
                    *first_reachable_definition,
                    superclass,
                    request.class,
                    definition,
                );
            }
        }
        Ok(())
    }

    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error> {
        Ok(definition.kind(self.context.db()))
    }

    async fn enum_value(
        &self,
        info: &EnumMetadata<'db>,
        name: &Name,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(info.members.get(name).copied())
    }

    async fn enum_auto(&self, info: &EnumMetadata<'db>, name: &Name) -> Result<bool, Self::Error> {
        Ok(info.auto_members.contains(name))
    }

    async fn is_ellipsis(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(
            matches!(ty, Type::NominalInstance(instance) if instance.has_known_class(self.context.db(), KnownClass::EllipsisType)),
        )
    }

    async fn in_stub(&self) -> Result<bool, Self::Error> {
        Ok(self.context.in_stub())
    }

    async fn enum_constructor(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        function: FunctionType<'db>,
        receiver: Type<'db>,
        value: Type<'db>,
        method: EnumConstructorMethod,
    ) -> Result<(), Self::Error> {
        check_enum_member_against_constructor_method(
            self.context,
            function,
            receiver,
            value,
            &request.member.member.name,
            request.member.first_reachable_definition,
            method,
        );
        Ok(())
    }

    async fn static_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        Ok(class.static_class_literal(self.context.db()))
    }

    async fn scope_symbol(
        &self,
        literal: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>), Self::Error> {
        let db = self.context.db();
        let scope = literal.body_scope(db);
        let table = place_table(db, scope);
        Ok((
            scope,
            table.symbol_id(name).map(|id| {
                let symbol = table.symbol(id);
                (id, symbol.is_bound() || symbol.is_declared())
            }),
        ))
    }

    async fn synthesized_member(
        &self,
        literal: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &Name,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(literal.own_synthesized_member(
            self.context.db(),
            &self.context.program_environment(),
            specialization,
            None,
            name,
        ))
    }

    async fn code_generator(
        &self,
        literal: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        Ok(CodeGeneratorKind::from_class(
            self.context.db(),
            literal.into(),
        ))
    }

    async fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error> {
        Ok(class.class_literal(self.context.db()))
    }

    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_class_member(
            self.context.db(),
            &self.context.program_environment(),
            None,
            name,
        ))
    }

    async fn lookup_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(lookup_override_member(
            self.context.db(),
            &self.context.program_environment(),
            class,
            name,
        ))
    }

    async fn symbol_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        Ok(symbol_definition(self.context.db(), scope, symbol))
    }

    async fn first_declaration(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        Ok(use_def_map(self.context.db(), scope)
            .end_of_scope_symbol_declarations(symbol)
            .find_map(|decl| decl.declaration.definition()))
    }

    async fn functions(
        &self,
        ty: Type<'db>,
    ) -> Result<SmallVec<[FunctionType<'db>; 1]>, Self::Error> {
        Ok(extract_underlying_functions(self.context.db(), ty))
    }

    async fn has_decorator(
        &self,
        function: FunctionType<'db>,
        decorator: FunctionDecorators,
    ) -> Result<bool, Self::Error> {
        Ok(function.has_known_decorator(self.context.db(), decorator))
    }

    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Self::Error> {
        Ok(is_function_definition(self.context.db(), scope, symbol))
    }

    async fn effective_variable_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Option<VariableKind>, Self::Error> {
        Ok(effective_superclass_variable_kind(
            self.context.db(),
            class,
            name.clone(),
        ))
    }

    async fn variable_kind(
        &self,
        own: PlaceAndQualifiers<'db>,
        instance: PlaceAndQualifiers<'db>,
    ) -> Result<Option<VariableKind>, Self::Error> {
        Ok(variable_kind(
            self.context.db(),
            &self.context.program_environment(),
            own,
            instance,
        ))
    }

    async fn is_subclass(
        &self,
        child: ClassType<'db>,
        parent: ClassType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(child.is_subclass_of(
            self.context.db(),
            &self.context.program_environment(),
            parent,
        ))
    }

    async fn override_types(
        &self,
        class: ClassType<'db>,
        name: &Name,
        subclass: Type<'db>,
        superclass: Type<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error> {
        let db = self.context.db();
        let env = &self.context.program_environment();
        Ok(method_override_types(
            db,
            env,
            bind_new_for_override(db, env, class, name, subclass),
            bind_new_for_override(db, env, class, name, superclass),
        ))
    }

    async fn assignable(&self, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error> {
        Ok(source.is_assignable_to(
            self.context.db(),
            &self.context.program_environment(),
            target,
        ))
    }

    async fn inherited_violation(
        &self,
        bases: &[ClassBase<'db>],
        owner: ClassType<'db>,
        superclass: ClassType<'db>,
        superclass_type: Type<'db>,
        name: &Name,
    ) -> Result<bool, Self::Error> {
        Ok(is_inherited_method_violation(
            self.context.db(),
            &self.context.program_environment(),
            bases,
            owner,
            superclass,
            superclass_type,
            name,
        ))
    }

    async fn fallback_member(
        &self,
        class: KnownClass,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(class
            .to_instance(self.context.db(), &self.context.program_environment())
            .member(self.context.db(), &self.context.program_environment(), name))
    }

    async fn missing_override(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        target: MissingOverrideTarget<'db>,
    ) -> Result<(), Self::Error> {
        check_missing_overrides(self.context, &request.member.member, request.scope, target);
        Ok(())
    }

    async fn explicit_override(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
    ) -> Result<(), Self::Error> {
        check_explicit_overrides(
            self.context,
            &request.member.member,
            request.scope,
            request.class,
        );
        Ok(())
    }
}
