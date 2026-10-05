//! Checks relating to invalid method overrides in subclasses,
//! including (but not limited to) violations of the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

pub(in crate::types) mod inherited_selection;
pub(in crate::types) mod local_functions;
pub(in crate::types) mod member_entry;
pub(in crate::types) mod namedtuple_fields;
pub(in crate::types) mod remaining;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) mod runtime;
pub(in crate::types) mod variable_kind;
pub(in crate::types) mod validation;

use bitflags::bitflags;
use ruff_db::{
    diagnostic::{Annotation, Span},
    files::FileRange,
};
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_python_stdlib::identifiers::is_mangled_private;
use rustc_hash::FxHashSet;

use crate::{
    Db, ProgramEnvironment,
    lint::{LintId, RuleSelection},
    place::{DefinedPlace, Place, PlaceAndQualifiers},
    types::{
        CallableType, ClassBase, ClassType, IntersectionType, KnownClass, Parameter,
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
            report_invalid_method_override,
        },
        enums::{EnumMetadata, is_enum_class_by_inheritance},
        function::{FunctionDecorators, FunctionType},
        list_members::{
            Member, MemberWithDefinition, all_end_of_scope_members,
        },
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
    match validation::check_class_sync(
        class,
        inconsistent_generic_bases,
        validation::OverrideCheckFacts,
        &validation::OrdinaryOverrideCheckEffects { context },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
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
    let env = &context.program_environment();

    let direct_bases = match inherited_selection::select_inherited_direct_bases_sync(
        class,
        &inherited_selection::OrdinaryInheritedBaseSelectionEffects { context },
    ) {
        Ok(bases) => bases,
        Err(never) => match never {},
    };
    let Some(direct_bases) = direct_bases else {
        return;
    };

    let constraints = ConstraintSetBuilder::new();
    if direct_bases.iter().enumerate().any(|(index, left)| {
        direct_bases[index + 1..]
            .iter()
            .any(|right| !left.could_coexist_in_mro_with(db, env, *right, &constraints))
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
    let receiver = Type::instance(db, env, class_specialized);
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
        // TODO: Include synthesized members when checking inherited conflicts. For example,
        // `Ordered.__gt__` is synthesized here and is incompatible with `AcceptsObject.__gt__`:
        //
        // ```python
        // from functools import total_ordering
        //
        // @total_ordering
        // class Ordered:
        //     def __lt__(self, other: Ordered) -> bool: ...
        //
        // class AcceptsObject:
        //     def __gt__(self, other: object) -> bool: ...
        //
        // class Conflict(Ordered, AcceptsObject): ...
        // ```
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
                source_method_contract(db, env, owner, receiver, name)
            else {
                continue;
            };

            for contract_owner in mro[index + 1..].iter().copied() {
                let Some((contract_decorator, contract_ty)) =
                    source_method_contract(db, env, contract_owner, receiver, name)
                else {
                    continue;
                };
                let Some((selected_ty, contract_ty)) =
                    method_override_types(db, env, selected_ty, contract_ty)
                else {
                    continue;
                };
                if selected_decorator == contract_decorator
                    && selected_ty.is_assignable_to(db, env, contract_ty)
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
                    let parent_receiver = Type::instance(db, env, owner);
                    let Some((parent_decorator, parent_ty)) =
                        source_method_contract(db, env, owner, parent_receiver, name)
                    else {
                        continue;
                    };
                    let Some((ancestor_decorator, ancestor_ty)) = source_method_contract(
                        db,
                        env,
                        parent_contract_owner,
                        parent_receiver,
                        name,
                    ) else {
                        continue;
                    };
                    if parent_decorator != ancestor_decorator
                        || !is_assignable_method_override(db, env, parent_ty, ancestor_ty)
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
                    || selected_ty.assignability_error_context(db, env, contract_ty),
                );
                continue 'members;
            }
        }
    }
}

/// Returns a source-defined method bound to the class whose MRO is being checked.
fn source_method_contract<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
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
        .own_class_member(db, env, None, name)
        .inner
        .place
        .raw_type()?
    else {
        return None;
    };
    let ty = Type::FunctionLiteral(function)
        .try_call_dunder_get(db, env, Some(receiver), receiver.to_meta_type(db, env))
        .unwrap_or_else(|error| Some(error.fallback()))?
        .return_type;
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
    let env = ProgramEnvironment::from_scope(class.body_scope(db));
    if !is_enum_class_by_inheritance(db, &env, class) {
        return false;
    }

    if matches!(
        name.as_str(),
        "__repr__" | "__str__" | "__format__" | "__reduce_ex__"
    ) {
        return selected_owner.is_known(db, KnownClass::Enum)
            || contract_owner.is_known(db, KnownClass::Enum);
    }

    env.python_version(db) >= PythonVersion::PY311
        && Type::ClassLiteral(class.into()).is_subtype_of(
            db,
            &env,
            KnownClass::Flag.to_subclass_of(db, &env),
        )
        && matches!(
            name.as_str(),
            "__or__" | "__and__" | "__xor__" | "__ror__" | "__rand__" | "__rxor__" | "__invert__"
        )
        && (selected_owner.is_known(db, KnownClass::Flag)
            || contract_owner.is_known(db, KnownClass::Flag))
}

fn check_class_declaration<'db>(
    context: &InferContext<'db, '_>,
    configuration: OverrideRulesConfig,
    enum_info: Option<&EnumMetadata<'db>>,
    class: ClassType<'db>,
    class_scope: ScopeId<'db>,
    bases: &[ClassBase<'db>],
    member: &MemberWithDefinition<'db>,
) {
    match member_entry::check_class_declaration_sync(
        member_entry::OverrideMemberRequest {
            configuration,
            enum_info,
            class,
            scope: class_scope,
            bases,
            member,
        },
        &member_entry::OrdinaryOverrideMemberEffects { context },
    ) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

fn check_named_tuple_attribute<'db>(
    context: &InferContext<'db, '_>,
    class_scope: ScopeId<'db>,
    member: &Member<'db>,
) {
    let db = context.db();
    if let Some(symbol_id) = place_table(db, class_scope).symbol_id(&member.name)
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

fn check_remaining_class_declaration<'db>(
    context: &InferContext<'db, '_>,
    request: member_entry::OverrideMemberRequest<'_, 'db>,
    instance_of_class: Type<'db>,
    subclass_instance_member: PlaceAndQualifiers<'db>,
    type_on_subclass_instance: Type<'db>,
    literal: StaticClassLiteral<'db>,
    class_kind: Option<CodeGeneratorKind<'db>>,
) {
    crate::types::legacy_inline(remaining::check_remaining_with(
        request,
        instance_of_class,
        subclass_instance_member,
        type_on_subclass_instance,
        literal,
        class_kind,
        &remaining::OrdinaryRemainingEffects { context },
    ));
}

/// Look up `__new__` on the class so generated and decorated callables retain `cls`
/// until they are bound for the override comparison.
fn lookup_override_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    name: &Name,
) -> PlaceAndQualifiers<'db> {
    match member_entry::lookup_override_member_sync(
        class,
        name,
        member_entry::OverrideLookupFacts,
        &member_entry::OrdinaryOverrideLookupEffects { db, env },
    ) {
        Ok(member) => member,
        Err(never) => match never {},
    }
}

/// Resolve `__new__` descriptors and normalize callable objects before binding the
/// constructor's implicit `cls`. This consumes a classmethod's bound `cls` or a
/// callable instance's `__call__` receiver first, as for constructor calls.
fn bind_new_for_override<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    name: &Name,
    ty: Type<'db>,
) -> Type<'db> {
    if name != "__new__" {
        return ty;
    }
    let receiver = Type::from(class);
    let instance_of_class = Type::instance(db, env, class);
    let Some(callables) = receiver
        .resolve_dunder_new_callable(db, env, Place::bound(ty), None)
        .place
        .ignore_possibly_undefined()
        .and_then(|ty| ty.try_upcast_to_callable(db, env))
    else {
        return ty;
    };
    // Overloads specialized for other subclasses do not constrain this override.
    // Compare call signatures independently of descriptor behavior.
    callables
        .map(|callable| callable.bind_self(db, env, receiver, instance_of_class))
        .to_type(db, env)
}

/// Returns whether the selected inherited method already violates this ancestor's contract.
///
/// The parent can inherit the method without defining an override. Its hierarchy may already
/// combine conflicting contracts that are absent from the method-defining class's hierarchy.
fn is_inherited_method_violation<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bases: &[ClassBase<'db>],
    method_owner: ClassType<'db>,
    superclass: ClassType<'db>,
    superclass_type: Type<'db>,
    name: &Name,
) -> bool {
    // An earlier MRO entry can select a different method in its own hierarchy. Check each
    // ancestor that inherits the method actually selected by the child's MRO.
    bases
        .iter()
        .filter_map(|base| base.into_class())
        .filter(|parent| {
            parent
                .iter_mro(db)
                .any(|base| base == ClassBase::Class(method_owner))
        })
        .any(|parent| {
            let Place::Defined(DefinedPlace {
                ty: parent_type, ..
            }) = lookup_override_member(db, env, parent, name).place
            else {
                return false;
            };
            // An inherited violation belongs to the parent's own receiver domain.
            let parent_type = bind_new_for_override(db, env, parent, name, parent_type);
            let superclass_type = bind_new_for_override(db, env, parent, name, superclass_type);
            if is_assignable_method_override(db, env, parent_type, superclass_type) {
                return false;
            }

            // Check the parent's own specializations: a valid override of `Base[Any]` can become
            // invalid when the child also inherits `Base[int]`. Include implicit ancestors such
            // as `object` and explicit inheritance paths for specializations omitted from the MRO.
            parent
                .iter_mro(db)
                .skip(1)
                .filter_map(ClassBase::into_class)
                .chain(parent.iter_explicit_ancestors(db, env).skip(1))
                .filter(|ancestor| ancestor.class_literal(db) == superclass.class_literal(db))
                .any(|ancestor| {
                    let Place::Defined(DefinedPlace {
                        ty: ancestor_type, ..
                    }) = lookup_override_member(db, env, ancestor, name).place
                    else {
                        return false;
                    };
                    !is_assignable_method_override(
                        db,
                        env,
                        parent_type,
                        bind_new_for_override(db, env, parent, name, ancestor_type),
                    )
                })
        })
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
    env: &ProgramEnvironment<'db>,
    subclass_type: Type<'db>,
    superclass_type: Type<'db>,
) -> bool {
    method_override_types(db, env, subclass_type, superclass_type).is_some_and(
        |(subclass_type, superclass_type)| subclass_type.is_assignable_to(db, env, superclass_type),
    )
}

fn method_override_types<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    subclass_type: Type<'db>,
    superclass_type: Type<'db>,
) -> Option<(Type<'db>, Type<'db>)> {
    let (subclass_type, superclass_type) = match (subclass_type, superclass_type) {
        (Type::BoundMethod(subclass_method), Type::BoundMethod(superclass_method))
            if let Some(superclass_signature) = superclass_method.unbound_signatures(db) =>
        {
            let explicit_receiver = match superclass_signature.overloads.as_slice() {
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

            let typing_self_type = subclass_method.typing_self_type(db);
            let receiver =
                explicit_receiver.map_or(subclass_method.self_instance(db), |receiver| {
                    let receiver = receiver.bind_self_typevars(db, env, typing_self_type);
                    IntersectionType::from_elements(
                        db,
                        env,
                        [subclass_method.self_instance(db), receiver],
                    )
                });

            // Both signatures describe calls on the subclass. In particular, inherited `Self`
            // annotations refer to the subclass even when the receiver is implicitly annotated.
            (
                subclass_method
                    .callables_with_receiver(db, env, receiver, typing_self_type)?
                    .to_type(db, env),
                superclass_method
                    .callables_with_receiver(db, env, receiver, typing_self_type)?
                    .to_type(db, env),
            )
        }
        _ => (subclass_type, superclass_type),
    };

    let superclass_callable = superclass_type
        .try_upcast_to_callable(db, env)?
        .map(|callable| callable.into_regular(db));

    Some((subclass_type, superclass_callable.to_type(db, env)))
}

/// Whether an attribute declaration is a class variable or an instance variable.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize)]
pub(super) enum VariableKind {
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
#[salsa::tracked(configuration = (pub(super) EffectiveSuperclassVariableKindConfiguration), attempt = ReturnOnly, returns(copy), heap_size=ruff_memory_usage::heap_size)]
pub(super) fn effective_superclass_variable_kind<'db>(
    db: &'db dyn Db,
    superclass: ClassType<'db>,
    name: Name,
) -> Option<VariableKind> {
    let env = &ProgramEnvironment::from_file(superclass.class_literal(db).program_file(db));
    crate::types::legacy_inline(variable_kind::effective_variable_kind_with(
        superclass,
        &name,
        &variable_kind::OrdinaryVariableKindEffects { db, env },
    ))
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
#[salsa::tracked(configuration = (pub(super) IsFunctionDefinitionConfiguration), attempt = ReturnOnly, returns(copy), heap_size=ruff_memory_usage::heap_size)]
pub(super) fn is_function_definition<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> bool {
    crate::types::legacy_inline(variable_kind::is_function_definition_with(
        scope,
        symbol,
        &variable_kind::OrdinaryFunctionDefinitionEffects { db },
    ))
}

/// Returns the variable kind for an attribute if it should participate in `ClassVar` override checks.
fn variable_kind<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class_member: PlaceAndQualifiers<'db>,
    instance_member: PlaceAndQualifiers<'db>,
) -> Option<VariableKind> {
    crate::types::legacy_inline(variable_kind::variable_kind_with(
        class_member,
        instance_member,
        &variable_kind::OrdinaryVariableKindEffects { db, env },
    ))
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
    diagnostic.set_primary_annotation_message(format_args!(
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
    pub(in crate::types) struct OverrideRulesConfig: u16 {
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
        Self::from_rule_selection(rule_selection)
    }
}

impl OverrideRulesConfig {
    pub(in crate::types) fn from_rule_selection(rule_selection: &RuleSelection) -> Self {
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
pub(in crate::types) struct LocalOverrideDefinition {
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

#[derive(Debug, Clone, Copy)]
pub(in crate::types) struct MissingOverrideTarget<'db> {
    superclass: ClassType<'db>,
    /// The source definition for the overridden superclass member, if one is available.
    definition: Option<Definition<'db>>,
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
    let override_module =
        if context.program_environment().python_version(db) >= PythonVersion::PY312 {
            "typing"
        } else {
            "typing_extensions"
        };
    diagnostic.info(format_args!(
        "Decorate the method with `@{override_module}.override` to make the override explicit"
    ));

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
    crate::types::legacy_inline(local_functions::invalid_explicit_override_definition_with(
        member,
        subclass_scope,
        &local_functions::OrdinaryLocalOverrideEffects { context },
    ))
}

fn missing_override_definition<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    subclass_scope: ScopeId<'db>,
) -> Option<LocalOverrideDefinition> {
    crate::types::legacy_inline(local_functions::missing_override_definition_with(
        member,
        subclass_scope,
        &local_functions::OrdinaryLocalOverrideEffects { context },
    ))
}

fn check_post_init_signature<'db>(
    context: &InferContext<'db, '_>,
    class: ClassType<'db>,
    member: &Member<'db>,
    definition: Definition<'db>,
    policy: CodeGeneratorKind<'db>,
) {
    let db = context.db();

    let Some((static_class, spec)) = class.static_class_literal(db) else {
        return;
    };
    let env = &context.program_environment();

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
        .with_annotated_type(Type::instance(db, env, class));

    let following_parameters = init_var_fields.map(|(name, field)| {
        Parameter::positional_only(Some(name.clone())).with_annotated_type(field.declared_ty)
    });

    let parameters =
        Parameters::standard(std::iter::chain([first_parameter], following_parameters));

    let expected_signature = CallableType::single(db, Signature::new(parameters, Type::object()));

    if member
        .ty
        .is_assignable_to(db, env, Type::Callable(expected_signature))
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
pub(in crate::types) enum EnumConstructorMethod {
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
    let env = &context.program_environment();

    // The enum metaclass unpacks tuple values as positional args:
    //   MEMBER = (a, b, c)  →  __new__(cls, a, b, c) / __init__(self, a, b, c)
    //   MEMBER = x          →  __new__(cls, x) / __init__(self, x)
    let args: Vec<Type<'db>> = if let Type::NominalInstance(instance) = member_value_type
        && let Some(spec) = instance.tuple_spec(db, env)
    {
        if let Tuple::Fixed(fixed) = &*spec {
            fixed.all_elements().to_vec()
        } else {
            // Variable-length tuples: can't determine exact args, skip validation.
            return;
        }
    } else {
        vec![member_value_type]
    };

    let call_args = CallArguments::positional(args);
    let call_args = call_args.with_self(Some(bound_self_type));

    let constraints = ConstraintSetBuilder::new();
    let result = Type::FunctionLiteral(function)
        .bindings(db, env)
        .match_parameters(db, env, &call_args)
        .check_types(
            db,
            env,
            &constraints,
            &call_args,
            TypeContext::default(),
            &[],
        );

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
                Type::FunctionLiteral(function).display(db, env),
            ));
        }
    }
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) fn effective_superclass_variable_kind_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<EffectiveSuperclassVariableKindConfiguration> {
    effective_superclass_variable_kind::fn_ingredient_(db, db.zalsa())
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) fn is_function_definition_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<IsFunctionDefinitionConfiguration> {
    is_function_definition::fn_ingredient_(db, db.zalsa())
}
