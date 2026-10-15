use super::DynamicType;
use super::ErrorContext;
use super::constraints::IteratorConstraintsExtension;
use super::infer::TransparentCallableReturn;
use super::protocol_class::StructuralMemberPriority;
use crate::types::TypeOperations;
use crate::types::recursive::RecursiveReferenceMapper;
use crate::types::recursive::TypeQueryVisit;
use crate::types::recursive::{TypeInspectionKey, TypeQueryKey};
use rustc_hash::FxHashSet;
use smallvec::{SmallVec, smallvec_inline};
use std::borrow::Cow;

use crate::{
    Db, FxOrderSet,
    place::Place,
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, CallableBinding,
        ClassType, CycleKind, FindLegacyTypeVarsVisitor, FunctionType, InternedType,
        KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueTypeKind,
        MemberLookupPolicy, Parameter, Parameters, Signature, SubclassOfInner, Type, TypeContext,
        TypeMapping, TypeQuery, TypeQueryContext, TypeVarBoundOrConstraints, UnionType,
        function::OverloadLiteral,
        known_instance::{FunctoolsPartialInstance, MethodWrapperKind},
        recursive::TypeExpansionContext,
        relation::TypeRelation,
        signatures::{CallableSignature, PartialSignatureApplication},
        variance::{VarianceInferable, VarianceTerm},
        visitor, walk_signature,
    },
};
use ty_python_core::definition::Definition;

struct FunctionDescriptorGet<'db> {
    instance: Option<Type<'db>>,
    owner: Option<Type<'db>>,
}

impl<'db> TypeQuery<'db> for FunctionDescriptorGet<'db> {
    fn recursion_key(&self) -> TypeQueryKey<'db> {
        TypeQueryKey::FunctionDescriptorGet(self.instance, self.owner)
    }

    type Output = Option<Type<'db>>;

    fn on_cycle(
        &self,
        _context: &TypeQueryVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        (): (),
    ) -> Self::Output {
        None
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        context: &TypeQueryVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        (): (),
    ) -> Self::Output {
        let env = context.environment();
        if let Type::Union(union) = ty {
            return union.try_map(db, env, |alternative| context.visit(db, *alternative));
        }
        let kind = ty.function_like_kind(db)?;
        let receiver = match kind {
            CallableTypeKind::StaticMethodLike => return Some(ty.underlying_function(db)),
            CallableTypeKind::ClassMethodLike => self
                .owner
                .filter(|owner| !owner.is_none(db))
                .or_else(|| self.instance.map(|instance| instance.to_meta_type(db, env))),
            _ => self.instance,
        };
        Some(receiver.map_or_else(
            || ty.underlying_function(db),
            |receiver| {
                Type::BoundMethod(super::BoundMethodType::from_callable(
                    db,
                    ty,
                    env.program(db),
                    receiver,
                ))
            },
        ))
    }
}

impl<'db> Type<'db> {
    pub(super) fn function_like_kind(self, db: &'db dyn Db) -> Option<CallableTypeKind> {
        match self {
            Type::FunctionLiteral(function) => Some(function.callable_type_kind(db)),
            Type::Callable(callable) if callable.is_method_like(db) => Some(callable.kind(db)),
            Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => {
                Some(match wrapper.kind(db) {
                    MethodWrapperKind::Staticmethod => CallableTypeKind::StaticMethodLike,
                    MethodWrapperKind::Classmethod => CallableTypeKind::ClassMethodLike,
                })
            }
            _ => None,
        }
    }

    pub(super) fn is_classmethod(self, db: &'db dyn Db) -> bool {
        self.function_like_kind(db) == Some(CallableTypeKind::ClassMethodLike)
    }

    /// Returns the function exposed by descriptor access or a bound method's `__func__`.
    pub(super) fn underlying_function(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::FunctionLiteral(function) => {
                Type::FunctionLiteral(function.underlying_function(db))
            }
            Type::Callable(callable) if callable.is_method_like(db) => {
                Type::Callable(callable.into_function_like(db))
            }
            Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => wrapper.wrapped(db),
            _ => self,
        }
    }

    /// Model the effect of `__get__` on functions, staticmethods, and
    /// classmethods.
    ///
    /// See [`Self::try_call_dunder_get`] for general descriptor access, including user-defined
    /// `__get__` methods.
    pub(super) fn function_like_dunder_get(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        TypeQueryContext::with_root(env, FunctionDescriptorGet { instance, owner }, |context| {
            context.visit(db, self)
        })
    }

    /// Create a callable type with a single non-overloaded signature.
    pub(crate) fn single_callable(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::single(db, signature))
    }

    /// Create a non-overloaded, function-like callable type with a single signature.
    ///
    /// A function-like callable will bind `self` when accessed as an attribute on an instance.
    pub(crate) fn function_like_callable(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::function_like(db, signature))
    }

    /// Create a non-overloaded callable type which represents the value bound to a `ParamSpec`
    /// type variable.
    pub(crate) fn paramspec_value_callable(
        db: &'db dyn Db,
        parameters: Parameters<'db>,
    ) -> Type<'db> {
        Type::Callable(CallableType::paramspec_value(db, parameters))
    }

    pub(crate) fn try_upcast_to_callable(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Option<CallableTypes<'db>> {
        self.try_upcast_to_callable_with_policy(db, env, UpcastPolicy::default())
    }

    pub(crate) fn try_upcast_to_callable_with_recursive_fallback(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        recursive_definition: Option<Definition<'db>>,
    ) -> Option<CallableTypes<'db>> {
        self.try_upcast_to_callable_with_policy_and_context(
            db,
            env,
            UpcastPolicy::default(),
            &CallableUpcastContext::new(env, recursive_definition),
            None,
        )
    }

    pub(crate) fn try_upcast_to_callable_with_policy(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        policy: UpcastPolicy,
    ) -> Option<CallableTypes<'db>> {
        self.try_upcast_to_callable_with_policy_and_context(
            db,
            env,
            policy,
            &CallableUpcastContext::new(env, None),
            None,
        )
    }

    fn try_upcast_to_callable_with_policy_and_context(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        policy: UpcastPolicy,
        context: &CallableUpcastContext<'_, 'db>,
        receiver: Option<Type<'db>>,
    ) -> Option<CallableTypes<'db>> {
        let key = self;
        // Stored functions and bound methods do not follow a deferred type definition here.
        // Binding a receiver can ask a relation to inspect the same method; that relation owns
        // the recursive assumption, so conversion must not reject it as an unfinished projection.
        if matches!(
            self,
            Type::Callable(_) | Type::FunctionLiteral(_) | Type::BoundMethod(_)
        ) {
            return context.active.visit_active_with_expansion(
                db,
                self,
                &context.mapping,
                |result| {
                    result.ok().and_then(|expansion| {
                        self.try_upcast_to_callable_impl(
                            db, env, policy, context, receiver, expansion,
                        )
                    })
                },
            );
        }
        env.context()
            .state()
            .callable_upcasts
            .get_or_init(|| Box::new(crate::types::CycleDetector::new(())))
            .visit_active_with_expansion(
                db,
                (
                    key,
                    (policy, context.recursive_definition),
                    receiver.unwrap_or(Type::Never),
                ),
                &context.mapping,
                |result| match result {
                    Ok(expansion) => self
                        .try_upcast_to_callable_impl(db, env, policy, context, receiver, expansion),
                    Err(kind)
                        if matches!(
                            self,
                            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
                        ) =>
                    {
                        Some(CallableTypes::one(CallableType::constructor_cycle(
                            db, self, kind,
                        )))
                    }
                    Err(_) => None,
                },
            )
    }

    fn try_upcast_to_callable_impl(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        policy: UpcastPolicy,
        context: &CallableUpcastContext<'_, 'db>,
        receiver: Option<Type<'db>>,
        expansion: &crate::types::recursive::TypeExpansionContext<'_, '_, 'db>,
    ) -> Option<CallableTypes<'db>> {
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.try_upcast_to_callable_with_policy_and_context(
                db, env, policy, context, receiver,
            );
        }

        match self {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::Callable(callable) => Some(CallableTypes::one(callable)),

            Type::Dynamic(_) => Some(CallableTypes::one(CallableType::function_like(
                db,
                Signature::dynamic(self),
            ))),
            Type::Divergent(_) => Some(CallableTypes::one(CallableType::function_like(
                db,
                Signature::dynamic(self),
            ))),

            Type::Recursive(recursive) => expansion
                .expand_recursive(db, recursive)
                .into_unfolded()?
                .upcast_to_callable(db, policy, context, receiver),

            Type::FunctionLiteral(function_literal)
                if context.is_recursive_reference(db, function_literal) =>
            {
                Some(CallableTypes::one(CallableType::bottom(db)))
            }
            Type::FunctionLiteral(function_literal) => {
                Some(CallableTypes::one(function_literal.into_callable_type(db)))
            }
            Type::BoundMethod(bound_method)
                if bound_method
                    .function(db)
                    .is_some_and(|function| context.is_recursive_reference(db, function)) =>
            {
                Some(CallableTypes::one(CallableType::bottom(db)))
            }
            Type::BoundMethod(bound_method) => bound_method.callables(db, env),

            Type::TypeVar(typevar) => match typevar.require_bound_or_constraints(db, env) {
                TypeVarBoundOrConstraints::UpperBound(bound) => bound
                    .try_upcast_to_callable_with_policy_and_context(
                        db,
                        env,
                        policy,
                        context,
                        Some(receiver.unwrap_or(self)),
                    ),
                // TODO: Preserve `Self` while validating the receiver against each constraint.
                TypeVarBoundOrConstraints::Constraints(_) => None,
            },

            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                let call_symbol = self
                    .member_lookup_with_policy_and_receiver(
                        db,
                        env,
                        "__call__",
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        receiver,
                    )
                    .unwrap_or_else(|error| error.fallback_member(db))
                    .member(db)
                    .place;

                if let Place::Defined(place) = call_symbol
                    && place.is_definitely_defined()
                {
                    place
                        .ty
                        .try_upcast_to_callable_with_policy_and_context(
                            db, env, policy, context, None,
                        )
                        // The callable instance itself doesn't inherit the descriptor behavior of
                        // its `__call__` method.
                        .map(|callables| callables.map(|callable| callable.into_regular(db)))
                } else {
                    None
                }
            }
            Type::ClassLiteral(class_literal) => Some(
                class_literal
                    .identity_specialization(db)
                    .into_callable(db, env),
            ),

            Type::GenericAlias(alias) => Some(ClassType::Generic(alias).into_callable(db, env)),

            Type::NewTypeInstance(newtype) => newtype
                .concrete_base_type(db, env)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context, receiver),

            Type::SubclassOf(subclass_of_ty) if policy == UpcastPolicy::Sound => {
                Some(CallableTypes::one(CallableType::function_like(
                    db,
                    Signature::new(Parameters::top(), subclass_of_ty.to_instance(db, env)),
                )))
            }

            // TODO: This is unsound so in future we can consider an opt-in option to disable it.
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(class) => Some(class.into_callable(db, env)),
                SubclassOfInner::Protocol(protocol) => protocol.class_origin(db).map(|origin| {
                    if protocol.materialization_kind(db).is_some() {
                        // The origin supplies the constructor, but the actual receiver retains
                        // `Top[P]` or `Bottom[P]`. Infer with both so instance-returning overloads
                        // are materialized without replacing explicit non-instance returns.
                        (*origin).into_callable_with_receiver(db, env, self)
                    } else {
                        (*origin).into_callable(db, env)
                    }
                }),
                SubclassOfInner::TypeVar(tvar) => {
                    match tvar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            let upcast_callables = bound
                                .constructor_for_typevar_bound(db, env)
                                .try_upcast_to_callable_with_policy_and_context(
                                    db, env, policy, context, None,
                                )?;
                            Some(
                                upcast_callables.map(|callable| {
                                    callable.with_return_type(db, Type::TypeVar(tvar))
                                }),
                            )
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            let mut callables = SmallVec::new();
                            for constraint in constraints.elements(db) {
                                let element_upcast = constraint
                                    .to_meta_type(db, env)
                                    .try_upcast_to_callable_with_policy_and_context(
                                        db, env, policy, context, None,
                                    )?;
                                for callable in element_upcast.into_inner() {
                                    callables
                                        .push(callable.with_return_type(db, Type::TypeVar(tvar)));
                                }
                            }
                            Some(CallableTypes::new(callables))
                        }
                    }
                }
                SubclassOfInner::Dynamic(_) => Some(CallableTypes::one(CallableType::single(
                    db,
                    Signature::new(Parameters::unknown(), Type::from(subclass_of_ty)),
                ))),
            },

            Type::Union(union) => {
                let mut callables = SmallVec::new();
                for element in union.elements(db) {
                    let element_callable = element.try_upcast_to_callable_with_policy_and_context(
                        db, env, policy, context, receiver,
                    )?;
                    callables.extend(element_callable.into_inner());
                }
                Some(CallableTypes::new(callables))
            }

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(enum_literal) => enum_literal
                    .enum_class_instance(db, env)
                    .try_upcast_to_callable_with_policy_and_context(
                        db, env, policy, context, receiver,
                    ),
                _ => None,
            },

            Type::TypeAlias(alias) => expansion
                .alias_body(db, alias)
                .upcast_to_callable(db, policy, context, receiver),

            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(callable)) => callable
                .inner(db)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context, None)
                .map(|callables| callables.map(|callable| callable.into_regular(db))),

            Type::KnownBoundMethod(method) => method.callables(db, env),

            Type::WrapperDescriptor(wrapper_descriptor) => {
                Some(CallableTypes::one(CallableType::new(
                    db,
                    CallableSignature::from_overloads(wrapper_descriptor.signatures(db, env)),
                    CallableTypeKind::Regular,
                )))
            }

            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => {
                Some(CallableTypes::one(CallableType::single(
                    db,
                    Signature::new(
                        Parameters::standard([Parameter::positional_only(None)
                            .with_annotated_type(newtype.base(db).instance_type(db, env))]),
                        Type::NewTypeInstance(newtype),
                    ),
                )))
            }

            Type::Never
            | Type::DataclassTransformer(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_) => None,

            Type::KnownInstance(
                KnownInstanceType::FunctoolsPartial(partial)
                | KnownInstanceType::FunctoolsPartialCall(partial),
            ) => Some(CallableTypes::one(partial.partial(db))),

            Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => {
                wrapper.callables(db, env)
            }

            Type::Intersection(intersection) => intersection
                .finite_alternative_union(db, env)
                .and_then(|alternatives| {
                    alternatives.try_upcast_to_callable_with_policy_and_context(
                        db, env, policy, context, receiver,
                    )
                }),

            Type::EnumComplement(complement) => complement
                .remaining_literal_union(db, env)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context, receiver),

            // TODO
            Type::DataclassDecorator(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::BoundSuper(_) => None,
        }
    }
}

pub(super) struct CallableUpcastContext<'env, 'db> {
    recursive_definition: Option<Definition<'db>>,
    active: crate::types::CycleDetector<'db, (), Type<'db>, (), 3>,
    mapping: ApplyTypeMappingVisitor<'env, 'db>,
}

impl<'env, 'db> CallableUpcastContext<'env, 'db> {
    pub(super) fn check(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        policy: UpcastPolicy,
        receiver: Option<Type<'db>>,
    ) -> Option<CallableTypes<'db>> {
        ty.try_upcast_to_callable_with_policy_and_context(
            db,
            self.mapping.env,
            policy,
            self,
            receiver,
        )
    }

    fn new(env: &'env TypeOperations<'db>, recursive_definition: Option<Definition<'db>>) -> Self {
        Self {
            recursive_definition,
            active: crate::types::CycleDetector::new(()),
            mapping: ApplyTypeMappingVisitor::new(env),
        }
    }

    fn is_recursive_reference(&self, db: &'db dyn Db, function: FunctionType<'db>) -> bool {
        self.recursive_definition
            .is_some_and(|definition| function.contains_definition(db, definition))
    }
}

/// The behavior we assume for a [`CallableType`] beyond its call signatures.
///
/// A callable's signature alone does not determine its runtime class, attributes, truthiness,
/// or whether it can act as a descriptor. The `CallableTypeKind` records which of these
/// properties we know or assume. Calls use the stored signature without reference to the
/// `CallableTypeKind`. Accessing `__call__`, however, returns the original callable type,
/// preserving both its signatures and its kind.
///
/// For [`Self::FunctionLike`], [`Self::StaticMethodLike`], and [`Self::ClassMethodLike`], the
/// LSP server emits method semantic tokens on attribute access, allowing editors to highlight
/// these attributes as methods. We also preserve these kinds when a decorator returns a
/// `Callable`, assuming that the decorator preserves the decorated function's descriptor behavior.
///
/// For example, `decorate` below returns a new function whose signature matches the original
/// method. We give the result the [`Self::FunctionLike`] kind, so `Example().method` still
/// binds `self`, even though the `Callable` return annotation does not guarantee that behavior:
///
/// ```python
/// from collections.abc import Callable
///
/// def decorate[**P, R](function: Callable[P, R]) -> Callable[P, R]:
///     def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
///         return function(*args, **kwargs)
///     return wrapper
///
/// class Example:
///     @decorate
///     def method(self, value: int) -> str:
///         return str(value)
///
/// Example().method(1)  # Returns "1"; no explicit self argument is needed.
/// ```
///
/// [`Self::ParamSpecValue`] is different to the other variants in that it does not describe
/// a runtime callable object. Instead, it uses the callable representation to store parameter
/// lists for type inference.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum CallableTypeKind {
    /// Arbitrary callable objects, as described by a `typing.Callable` annotation.
    ///
    /// These can be functions, bound methods, classes, or instances with a `__call__` method.
    /// We know their call signatures but do not assume a particular runtime class: truthiness
    /// is ambiguous, the metatype is `type`, and member lookup exposes only `object` attributes
    /// and `__call__`. In particular, function attributes such as `__name__` are not guaranteed.
    ///
    /// We do not bind a receiver when these callables are accessed as attributes. In this
    /// example, `Calculator().add` still requires both integer arguments: accessing `add`
    /// through the instance does not supply that instance as its first argument.
    ///
    /// ```python
    /// from collections.abc import Callable
    ///
    /// class Add:
    ///     def __call__(self, left: int, right: int) -> int:
    ///         return left + right
    ///
    /// class Calculator:
    ///     add: Callable[[int, int], int] = Add()
    ///
    /// Calculator.add(1, 2)  # Returns 3.
    /// Calculator().add(1, 2)  # Returns 3; both arguments are still required.
    /// ```
    ///
    /// As a heuristic, class-member lookup converts dunder attributes of this kind to
    /// [`Self::FunctionLike`] if their signatures can accept parameters. See
    /// [`Self::DunderParamSpec`] for the exception for `Callable[P, R]` attributes.
    ///
    /// For example, `len(Sized())` implicitly calls `__len__` on the `Sized` instance. We
    /// treat the `Callable`-typed `__len__` as a function, so accessing `Sized().__len__`
    /// binds the `instance` parameter and gives it the signature `() -> int`:
    ///
    /// ```python
    /// from collections.abc import Callable
    ///
    /// def length(instance: "Sized") -> int:
    ///     return 1
    ///
    /// class Sized:
    ///     __len__: Callable[["Sized"], int] = length
    ///
    /// len(Sized())  # Returns 1.
    /// ```
    Regular,

    /// Callable objects modeled as instances of Python's `types.FunctionType`.
    ///
    /// A [`Type::FunctionLiteral`] identifies a particular function definition. This kind
    /// represents functions with the given signatures without requiring that identity. It is
    /// also used for lambdas and synthesized methods of dataclasses and named tuples.
    ///
    /// We model these callables as follows:
    ///
    /// - These callables are always truthy.
    /// - Member lookup uses `types.FunctionType`, exposing attributes such as `__name__`,
    ///   `__qualname__`, `__module__`, `__code__`, `__defaults__`, and `__annotations__`.
    ///   `__call__` retains the callable's precise signatures.
    /// - Their metatype is the `types.FunctionType` class literal.
    /// - They are subtypes of `types.FunctionType`. They can also satisfy a
    ///   [`Self::Regular`] callable type with a compatible signature, but a regular callable
    ///   cannot satisfy a function-like callable type merely by having a compatible signature.
    /// - They use `types.FunctionType` as their owner type when constructing `super()`.
    /// - They act as [non-data descriptors][descriptor-protocol]: access through a class leaves
    ///   the signature unchanged, while access through an instance binds the first parameter
    ///   to that instance, producing a [`super::BoundMethodType`] that does not bind again.
    /// - Like function literals, they defer binding `typing.Self` until the receiver is known
    ///   from the call's arguments, as illustrated below.
    ///
    /// In this example, `Base.identity` is function-like because of the decorator. Retrieving
    /// it from `Base` does not fix `Self` to `Base`: the subsequent call passes a `Child`
    /// instance, so both the receiver's type and the return type are `Child`.
    ///
    /// ```python
    /// from collections.abc import Callable
    /// from typing import Self, reveal_type
    ///
    /// def preserve[**P, R](function: Callable[P, R]) -> Callable[P, R]:
    ///     return function
    ///
    /// class Base:
    ///     @preserve
    ///     def identity(self) -> Self:
    ///         return self
    ///
    /// class Child(Base):
    ///     pass
    ///
    /// identity = Base.identity
    /// reveal_type(identity(Child()))  # Child
    /// ```
    ///
    /// When inferring a mutable collection's element type, we generalize function literals to
    /// function-like callables. This allows the list below to contain other functions with
    /// compatible signatures, rather than restricting it to the single function `first`:
    ///
    /// ```python
    /// def first(value: int) -> str:
    ///     return str(value)
    ///
    /// def second(value: int) -> str:
    ///     return f"{value}!"
    ///
    /// callbacks = [first]  # Inferred as list[(value: int) -> str].
    /// callbacks.append(second)
    /// ```
    ///
    /// [descriptor-protocol]: https://docs.python.org/3/howto/descriptor.html#descriptor-protocol
    FunctionLike,

    /// A `Callable[P, R]`-typed dunder attribute whose parameters come from a `ParamSpec`.
    ///
    /// This has the runtime assumptions of [`Self::Regular`]: truthiness is ambiguous,
    /// member lookup exposes `object` attributes, and we do not treat these callables as
    /// descriptors. The separate kind prevents the dunder descriptor heuristic from turning
    /// it into [`Self::FunctionLike`] after `P` is specialized: the specialized parameters
    /// already describe the callable's arguments.
    ///
    /// In the example below, specializing `P` to `[str]` gives `callback.__call__` the signature
    /// `(str, /) -> int`. Binding a receiver would incorrectly remove its `str` parameter:
    ///
    /// ```python
    /// from collections.abc import Callable
    ///
    /// class Callback[**P]:
    ///     __call__: Callable[P, int]
    ///
    /// class Length(Callback[[str]]):
    ///     def __call__(self, text: str) -> int:
    ///         return len(text)
    ///
    /// def invoke(callback: Callback[[str]]) -> int:
    ///     return callback("hello")
    ///
    /// invoke(Length())  # Returns 5.
    /// ```
    ///
    /// This variant is used to represent the callable object itself; [`Self::ParamSpecValue`]
    /// represents the parameter list substituted for `P`.
    DunderParamSpec,

    /// A callable with the descriptor behavior of `staticmethod`.
    StaticMethodLike,

    /// A callable with the descriptor behavior of `classmethod`.
    ClassMethodLike,

    /// An internal representation of the value bound to a `typing.ParamSpec` type variable.
    ///
    /// Unlike the other variants, this does not represent a callable object in its entirety:
    /// it represents only the parameter lists substituted for a `ParamSpec`.
    ///
    /// We reuse callable signatures to store the parameter lists, including overloads, with
    /// `Unknown` return types as placeholders. Specialization extracts these parameters into
    /// `Callable[P, R]`, `Concatenate`, or paired `P.args`/`P.kwargs` annotations while preserving
    /// the enclosing callable's return type. A single signature is displayed as a parameter
    /// list, without a return type.
    ///
    /// This kind also distinguishes gradual `...` parameter lists and their top and bottom
    /// materializations from ordinary callable types in type-relation checks. It does not
    /// carry the runtime `typing.ParamSpec` instance behavior of a `ParamSpec` declaration.
    ParamSpecValue,
}

/// A "policy" enum that describes how `type[]` types should be upcast
/// to `Callable` types.
///
/// `type[T]` is generally considered assignable to
/// `Callable[<constructor signature of T>, T]` in Python, and most
/// type-checking in Python uses assignability rather than subtyping
/// when determining whether to emit errors on code, so -- despite its
/// scary name -- [`UpcastPolicy::Unsound`] is actually the policy that
/// you probably want in most situations. We *have* to use
/// [`UpcastPolicy::Sound`], however, when doing subtyping or redundancy
/// checks, because constructor signatures in subclasses are not checked
/// for Liskov substitutability: `type[S]` may not be a subtype of
/// `Callable[<constructor signature of T>, T]` even if `S` is a subtype
/// of `T`. If this unsoundness leaked into our union simplification or
/// subtyping checks, it would ead to nontransitivity of subtyping,
/// breaking fundamental assumptions in our model.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub(crate) enum UpcastPolicy {
    /// Only upcast types to callables in a sound fashion.
    ///
    /// This means that `type[T]` is upcast to `Top[Callable[..., T]]`
    /// rather than `Callable[<constructor signature of T>, T]`,
    /// since the former is sound while the latter is not.
    Sound,

    /// Allow unsound upcasts to callables, such as treating `type[T]` as
    /// `Callable[<constructor signature of T>, T`.
    #[default]
    Unsound,
}

impl From<TypeRelation> for UpcastPolicy {
    fn from(relation: TypeRelation) -> Self {
        match relation {
            TypeRelation::Subtyping | TypeRelation::Redundancy { .. } => UpcastPolicy::Sound,
            TypeRelation::Assignability => UpcastPolicy::Unsound,
        }
    }
}

/// This type represents the set of all callable objects with a certain, possibly overloaded,
/// signature.
///
/// It can be written in type expressions using `typing.Callable`. `lambda` expressions are
/// inferred directly as `CallableType`s; all function-literal types are subtypes of a
/// `CallableType`.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct CallableType<'db> {
    #[returns(ref)]
    pub(super) signature_source: SignatureSource<'db>,

    #[returns(copy)]
    pub(super) kind: CallableTypeKind,

    /// The declaration on which `@deprecated` wrapped this callable. Retain the declaration
    /// for diagnostic names, source annotations, and deduplication, independently of binding kind.
    #[returns(copy)]
    pub(crate) deprecated: Option<OverloadLiteral<'db>>,
}

/// A callable stores signatures directly or projects the callable interface of another type.
/// The signature payload can be borrowed when looking up an interned callable.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum SignatureSource<'db, S = CallableSignature<'db>> {
    /// Signatures stored directly. These include:
    ///
    /// - Callable annotations, such as `Callable[[int], str]`.
    /// - Function signatures converted from [`FunctionType`].
    /// - Synthesized signatures, such as tuple `__getitem__` overloads and `TypedDict` methods.
    /// - `ParamSpec` argument lists represented as callable signatures.
    ///
    /// "Explicit" describes how the signatures are provided; their types can still be inferred.
    Explicit(S),

    /// A callable interface obtained from another type, retaining the source of its recursion.
    Projected(ProjectedSignature<'db>),
}

/// The callable interface of a class or instance, with the original type retained.
/// The applied signatures are private: semantic consumers inspect them through the operation
/// that guards the projection, rather than treating them as independent stored signatures.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct ProjectedSignature<'db> {
    source: Type<'db>,
    callable: Option<CallableType<'db>>,
}

impl<'db> ProjectedSignature<'db> {
    pub(super) fn source(&self) -> Type<'db> {
        self.source
    }

    pub(super) fn walk_inputs<V: visitor::TypeVisitor<'db> + ?Sized>(
        &self,
        db: &'db dyn Db,
        visitor: &V,
    ) {
        visitor.visit_type(db, self.source);
        if let Some(callable) = self.callable {
            walk_callable_type(db, callable, visitor);
        } else {
            visitor.notify_skipped_lazy_type_attributes();
        }
    }
}

enum ParameterSource<'db> {
    Explicit(&'db CallableSignature<'db>),
}

/// Stored parameter annotations and declaration metadata, without access to a return type.
#[derive(Clone, Copy)]
pub(super) struct CallableParameters<'db> {
    parameters: &'db Parameters<'db>,
    declaration: Option<&'db Signature<'db>>,
}

impl<'db> CallableParameters<'db> {
    pub(super) fn parameters(self) -> &'db Parameters<'db> {
        self.parameters
    }
    pub(super) fn generic_context(self) -> Option<super::GenericContext<'db>> {
        self.declaration
            .and_then(|signature| signature.generic_context)
    }
    pub(super) fn definition(self) -> Option<Definition<'db>> {
        self.declaration.and_then(Signature::definition)
    }
    pub(super) fn source_overload_index(self) -> Option<usize> {
        self.declaration.and_then(Signature::source_overload_index)
    }
    pub(super) fn with_return_type(self, return_type: Type<'db>) -> Signature<'db> {
        match self.declaration {
            Some(signature) => signature.clone().with_return_type(return_type),
            None => Signature::new(self.parameters.clone(), return_type),
        }
    }
}

// Borrowed signatures hash identically to owned signatures. Salsa only clones them when
// inserting a new callable, rather than on every lookup of an existing callable.
impl<'db> salsa::Lookup<SignatureSource<'db>> for SignatureSource<'db, &CallableSignature<'db>> {
    fn into_owned(self) -> SignatureSource<'db> {
        match self {
            Self::Explicit(signatures) => SignatureSource::Explicit(signatures.clone()),

            Self::Projected(projection) => SignatureSource::Projected(projection),
        }
    }
}

impl<'db> salsa::HashEqLike<SignatureSource<'db, &CallableSignature<'db>>>
    for SignatureSource<'db>
{
    fn eq(&self, other: &SignatureSource<'db, &CallableSignature<'db>>) -> bool {
        match (self, other) {
            (Self::Explicit(left), SignatureSource::Explicit(right)) => left == *right,

            (Self::Projected(left), SignatureSource::Projected(right)) => left == right,
            _ => false,
        }
    }
}

pub(super) fn walk_callable_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    ty: CallableType<'db>,
    visitor: &V,
) {
    if visitor.should_visit_lazy_type_attributes() {
        if ty
            .inspect_signatures(db, visitor.program_environment(), |signatures| {
                signatures.walk(db, visitor);
            })
            .is_none()
        {
            visitor.notify_skipped_lazy_type_attributes();
        }
    } else {
        match ty.signature_source(db) {
            SignatureSource::Explicit(signatures) => {
                for signature in signatures {
                    walk_signature(db, signature, visitor);
                }
            }

            SignatureSource::Projected(projection) => {
                visitor.notify_skipped_lazy_type_attributes();
                projection.walk_inputs(db, visitor);
            }
        }
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CallableType<'_> {}

/// Signatures borrowed while their expansion guard remains active.
///
/// Consumers can perform named operations, but cannot extract a signature or its return type.
pub(crate) struct ExpandedSignatures<'scope, 'db> {
    signatures: &'scope CallableSignature<'db>,
    env: &'scope TypeOperations<'db>,
}

impl<'scope, 'db> ExpandedSignatures<'scope, 'db> {
    /// Summarize annotation variance for diagnostics while the return type remains guarded.
    pub(super) fn single_annotation_variances(
        self,
        db: &'db dyn Db,
        variable: BoundTypeVarIdentity<'db>,
    ) -> Option<(Vec<(usize, super::TypeVarVariance)>, super::TypeVarVariance)> {
        let [signature] = self.signatures.overloads.as_slice() else {
            return None;
        };
        let parameters = signature
            .parameters()
            .iter()
            .filter_map(|parameter| {
                let index = parameter.source_parameter_index()?;
                let ty = match parameter.annotated_type() {
                    Type::TypeVar(typevar) if typevar.paramspec_attr(db).is_some() => {
                        Type::TypeVar(typevar.without_paramspec_attr(db))
                    }
                    ty => ty,
                };
                Some((
                    index,
                    ty.with_polarity(super::TypeVarVariance::Contravariant)
                        .variance_of(db, self.env, variable)
                        .evaluate(db),
                ))
            })
            .collect();
        Some((
            parameters,
            signature
                .return_ty
                .variance_of(db, self.env, variable)
                .evaluate(db),
        ))
    }

    /// Inspect signatures already stored as finite values, without expanding a callable.
    pub(super) fn from_stored(
        signatures: &'scope CallableSignature<'db>,
        env: &'scope TypeOperations<'db>,
    ) -> Self {
        Self { signatures, env }
    }

    pub(super) fn structural_member_priority(
        self,
        db: &'db dyn Db,
    ) -> super::protocol_class::StructuralMemberPriority {
        let env = self.env;
        let is_recursive_type = |ty| {
            super::visitor::any_over_type_expanding_aliases(db, env, ty, |nested| {
                matches!(nested, Type::ProtocolInstance(_))
            })
        };
        let finite_priority = match self.signatures.iter().len() {
            0 => return StructuralMemberPriority::Recursive,
            1 => StructuralMemberPriority::Simple,
            _ => StructuralMemberPriority::FiniteOverload,
        };

        let is_recursive = self.signatures.iter().any(|signature| {
            signature
                .receiver_constraint_types()
                .chain(
                    signature
                        .parameters()
                        .iter()
                        .skip(usize::from(
                            signature.has_implicit_positional_receiver_annotation(),
                        ))
                        .map(Parameter::annotated_type),
                )
                .chain(std::iter::once(signature.return_ty))
                .any(is_recursive_type)
        });
        if is_recursive {
            return StructuralMemberPriority::Recursive;
        }

        finite_priority
    }

    pub(super) fn collect_property_returns(
        self,
        output: &mut super::protocol_class::PropertyReturns<'db>,
    ) {
        output.collect(self.signatures);
    }

    pub(super) fn compare_protocol_method<'c>(
        self,
        db: &'db dyn Db,
        checker: &super::relation::TypeRelationChecker<'_, 'c, 'db>,
        callable: CallableType<'db>,
        required: CallableType<'db>,
        implementation_receiver: Type<'db>,
        protocol_receiver: Type<'db>,
    ) -> super::constraints::ConstraintSet<'db, 'c> {
        checker.check_protocol_method_signatures(
            db,
            callable,
            required,
            implementation_receiver,
            protocol_receiver,
            self.signatures,
        )
    }

    pub(super) fn supports_protocol_variance_inference(self, db: &'db dyn Db) -> bool {
        self.signatures.iter().all(|signature| {
            signature
                .parameters()
                .iter()
                .map(Parameter::annotated_type)
                .chain(std::iter::once(signature.return_ty))
                .all(|ty| super::protocol_class::supports_protocol_variance_type(db, self.env, ty))
        })
    }

    pub(super) fn overload_inconsistency(
        self,
        db: &'db dyn Db,
        overload: &Signature<'db>,
    ) -> Option<super::infer::OverloadInconsistency<'db>> {
        super::infer::OverloadInconsistency::check(db, self.env, self.signatures, overload)
    }

    pub(super) fn infer_specialization(
        &self,
        formal: &ExpandedSignatures<'_, 'db>,
        builder: &mut super::generics::SpecializationBuilder<'db, '_>,
    ) -> Result<(), super::generics::SpecializationError<'db>> {
        builder.infer_signature_pair(formal.signatures, self.signatures)
    }

    pub(super) fn rebind_return_typevars(
        self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        inspector: &super::recursive::TypeInspector<'_, 'db>,
        replacements: &crate::FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
    ) -> CallableType<'db> {
        let signatures = inspector.map_signatures(
            db,
            self.signatures,
            &TypeMapping::ApplySpecialization(
                super::generics::ApplySpecialization::ReturnCallables(replacements),
            ),
            TypeContext::default(),
        );
        let context = super::generics::GenericContext::from_typevar_instances(
            db,
            self.env,
            replacements.values().copied(),
        );
        callable.with_signatures(db, signatures.with_inherited_generic_context(db, context))
    }

    pub(super) fn inherit_generic_context(
        self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        context: super::generics::GenericContext<'db>,
    ) -> CallableType<'db> {
        callable.with_signatures(
            db,
            self.signatures.with_inherited_generic_context(db, context),
        )
    }

    pub(super) fn collect_definition_signatures(
        self,
        output: &mut super::function::DefinitionSignatures<'db>,
    ) {
        output.extend(self.signatures);
    }

    pub(super) fn transparent_decorator_parameters(
        self,
        db: &'db dyn Db,
    ) -> Option<(
        BoundTypeVarInstance<'db>,
        super::infer::TransparentCallableReturn<'db>,
    )> {
        let env = self.env;
        let [signature] = self.signatures.overloads.as_slice() else {
            return None;
        };
        let paramspec = signature.parameters().as_paramspec()?;
        let return_typevar = if let Some(typevar) = signature.return_ty.as_typevar() {
            TransparentCallableReturn::TypeVar(typevar)
        } else {
            let specialization =
                signature
                    .return_ty
                    .known_specialization(db, env, KnownClass::Awaitable)?;
            let [inner] = specialization.types(db) else {
                return None;
            };
            TransparentCallableReturn::Awaitable(inner.as_typevar()?)
        };
        Some((paramspec, return_typevar))
    }

    pub(super) fn has_only_unknown_context(self) -> bool {
        self.signatures.iter().all(|signature| {
            let parameters = signature.parameters().as_slice();
            (signature.return_ty.is_dynamic()
                && parameters
                    .iter()
                    .all(|parameter| parameter.annotated_type().is_dynamic()))
                || (parameters.is_empty()
                    && matches!(
                        signature.return_ty,
                        Type::Dynamic(DynamicType::UnspecializedTypeVar)
                    ))
        })
    }

    pub(super) fn lambda_context(
        self,
        tcx: TypeContext<'db>,
    ) -> Option<super::infer::ContextualLambda<'db>> {
        let [signature] = self.signatures.overloads.as_slice() else {
            return None;
        };
        Some(super::infer::ContextualLambda::from_signature(
            signature, tcx,
        ))
    }

    pub(super) fn total_ordering_method(self, db: &'db dyn Db) -> CallableType<'db> {
        let env = self.env;
        let bool_ty = KnownClass::Bool.to_instance(db, env);

        let signatures =
            CallableSignature::from_overloads(self.signatures.iter().map(|signature| {
                // The generated methods return a union of the root method's return type
                // and `bool`. This is because `@total_ordering` synthesizes methods like:
                //     def __gt__(self, other): return not (self == other or self < other)
                // If `__lt__` returns `int`, then `__gt__` could return `int | bool`.
                let return_ty = UnionType::from_two_elements(db, env, signature.return_ty, bool_ty);
                Signature::new_generic(
                    signature.generic_context,
                    signature.parameters().clone(),
                    return_ty,
                )
            }));
        CallableType::new(db, signatures, CallableTypeKind::FunctionLike)
    }

    pub(super) fn all_returns_awaitable(self, db: &'db dyn Db) -> bool {
        let env = self.env;
        // Gradual returns and Never do not establish that a call should be awaited.
        let awaitable = KnownClass::Awaitable
            .to_instance(db, env)
            .top_materialization(db, env);
        self.signatures.iter().all(|signature| {
            !signature.return_ty.is_equivalent_to(db, env, Type::Never)
                && signature.return_ty.is_subtype_of(db, env, awaitable)
        })
    }

    pub(super) fn descriptor_setter_domain(
        self,
        db: &'db dyn Db,
        self_ty: Type<'db>,
        receiver_ty: Type<'db>,
    ) -> Option<Type<'db>> {
        super::attribute_write::signatures_setter_domain(
            db,
            self.env,
            self.signatures,
            self_ty,
            receiver_ty,
        )
    }

    pub(super) fn has_single_signature(&self) -> bool {
        self.signatures.overloads.len() == 1
    }

    pub(super) fn single_generic_context(self) -> Option<super::GenericContext<'db>> {
        let [signature] = self.signatures.overloads.as_slice() else {
            return None;
        };
        signature.generic_context
    }

    pub(super) fn single_parameter_metadata(
        self,
    ) -> Option<Vec<super::signatures::ParameterMetadata<'db>>> {
        let [signature] = self.signatures.overloads.as_slice() else {
            return None;
        };
        Some(
            signature
                .parameters()
                .iter()
                .map(Parameter::metadata)
                .collect(),
        )
    }

    pub(super) fn collect_generic_contexts(
        self,
        db: &'db dyn Db,
        collector: &super::call::bind::GenericContextCollector<'_, 'db>,
        visit: &visitor::TypeVisit<'_, 'db, super::call::bind::GenericContextCollector<'_, 'db>>,
    ) {
        collector.visit_signatures(db, self.signatures, visit);
    }

    /// Return `true` if a callable has at least one overload and none return `Never`.
    ///
    /// Return-type disjointness is a pragmatic approximation for method members: a callable returning
    /// `Never` could satisfy otherwise-incompatible signatures, so it must not establish disjointness.
    pub(super) fn has_only_non_never_returns(&self, db: &'db dyn Db) -> bool {
        let env = self.env;
        let mut signatures = self.signatures.iter();
        let Some(first) = signatures.next() else {
            return false;
        };
        !first.return_ty.has_outer_type(db, env, |ty| ty.is_never())
            && signatures.all(|signature| {
                !signature
                    .return_ty
                    .has_outer_type(db, env, |ty| ty.is_never())
            })
    }

    pub(super) fn compare_return_disjointness<'c>(
        self,
        db: &'db dyn Db,
        target: &ExpandedSignatures<'_, 'db>,
        checker: &super::relation::DisjointnessChecker<'_, 'c, 'db>,
    ) -> super::constraints::ConstraintSet<'db, 'c> {
        let env = self.env;
        // Disjointness distributes over unions. Comparing the overload arms directly
        // avoids building and normalizing a union of recursive return types.
        self.signatures
            .iter()
            .when_all(db, checker.constraints, |source| {
                target
                    .signatures
                    .iter()
                    .when_all(db, checker.constraints, |target| {
                        let result =
                            checker.check_type_pair(db, source.return_ty, target.return_ty);
                        if let Some(context) = checker.report_context()
                            && result.is_always_satisfied(db, env, checker.inferable)
                        {
                            context.push(ErrorContext::DisjointReturnTypes {
                                left: source.return_ty,
                                right: target.return_ty,
                            });
                        }
                        result
                    })
            })
    }

    pub(super) fn display_callable(
        self,
        display: &super::display::DisplayCallableType<'_, 'db>,
        writer: &mut super::display::TypeWriter<'db>,
    ) -> std::fmt::Result {
        display.fmt_signatures(self.signatures, writer)
    }

    pub(super) fn display_bound_method(
        self,
        display: &super::display::DisplayRepresentation<'_, 'db>,
        writer: &mut super::display::TypeWriter<'db>,
        function: FunctionType<'db>,
        receiver: Type<'db>,
        signature_receiver: Type<'db>,
    ) -> std::fmt::Result {
        display.fmt_bound_method_signatures(
            self.signatures,
            writer,
            function,
            receiver,
            signature_receiver,
        )
    }

    pub(super) fn bind_bound_method(
        self,
        db: &'db dyn Db,
        bound_method: super::BoundMethodType<'db>,
    ) -> super::Bindings<'db> {
        let env = self.env;
        let signature = self.signatures;

        let self_instance = bound_method.self_instance(db);
        let signature_receiver = bound_method.signature_receiver(db);
        // Class-based protocol member lookup has already specialized the method for this
        // receiver. Bake an implicit positional receiver into the signature instead of
        // checking it structurally again during call inference.
        let protocol_receiver_is_specialized = self_instance
            .as_protocol_instance()
            .is_some_and(|protocol| protocol.class_origin(db).is_some())
            && signature
                .overloads
                .iter()
                .all(Signature::has_implicit_positional_receiver_annotation);
        // Synthesized signatures can contain `Self` without a function's generic
        // context, so substitute it when capturing the receiver.
        if bound_method.function(db).is_none()
            || protocol_receiver_is_specialized
            || signature_receiver != self_instance
        {
            let mut binding = CallableBinding::from_overloads(
                Type::BoundMethod(bound_method),
                signature.overloads.iter().cloned(),
            )
            .with_bound_type(signature_receiver);
            binding.bake_bound_type_into_overloads(db, env);
            binding.into()
        } else {
            // Solve exact receiver constraints before checking the other arguments, but
            // retain the receiver itself for call inference and receiver diagnostics.
            let overloads = signature.overloads.iter().flat_map(|overload| {
                if overload.has_receiver_determined_method_typevar(db, env)
                    && let Some(specialized) = overload.specialize_for_bound_receiver(
                        db,
                        env,
                        self_instance,
                        bound_method.typing_self_type(db, env),
                    )
                {
                    specialized.overloads
                } else {
                    smallvec_inline![overload.clone()]
                }
            });

            CallableBinding::from_overloads(Type::BoundMethod(bound_method), overloads)
                .with_bound_type(self_instance)
                .into()
        }
    }

    pub(super) fn bindings(
        self,
        callable: Type<'db>,
        receiver: Option<Type<'db>>,
    ) -> super::Bindings<'db> {
        let binding =
            super::CallableBinding::from_overloads(callable, self.signatures.iter().cloned());
        match receiver {
            Some(receiver) => binding.with_bound_type(receiver).into(),
            None => binding.into(),
        }
    }

    pub(crate) fn return_behavior(self, db: &'db dyn Db) -> (bool, bool, bool) {
        let mut none_return_never = true;
        let mut all_return_never = true;
        let mut any_generic = false;
        for signature in self.signatures {
            let returns_never = signature
                .return_ty
                .is_equivalent_to(db, self.env, Type::Never);
            none_return_never &= !returns_never;
            all_return_never &= returns_never;
            any_generic |= signature.return_ty.has_typevar(db, self.env);
        }
        (none_return_never, all_return_never, any_generic)
    }

    pub(super) fn can_suppress_exceptions(self, db: &'db dyn Db, is_async: bool) -> bool {
        let env = self.env;
        let exception_type = KnownClass::BaseException.to_subclass_of(db, env);
        let exception_instance = KnownClass::BaseException.to_instance(db, env);
        for signature in self.signatures {
            if signature
                .parameters()
                .get_positional(0)
                .is_some_and(|parameter| {
                    parameter
                        .annotated_type()
                        .is_disjoint_from(db, env, exception_type)
                })
                || signature
                    .parameters()
                    .get_positional(1)
                    .is_some_and(|parameter| {
                        parameter
                            .annotated_type()
                            .is_disjoint_from(db, env, exception_instance)
                    })
            {
                continue;
            }

            let return_type = if is_async {
                let Ok(awaited) = signature.return_ty.try_await(db, env) else {
                    continue;
                };
                awaited
            } else {
                signature.return_ty
            };

            if return_type.is_equivalent_to(db, env, KnownClass::Bool.to_instance(db, env))
                || return_type.is_equivalent_to(db, env, Type::bool_literal(true))
            {
                return true;
            }
        }
        false
    }

    pub(super) fn compare<'c>(
        self,
        db: &'db dyn Db,
        target: &ExpandedSignatures<'_, 'db>,
        checker: &super::relation::TypeRelationChecker<'_, 'c, 'db>,
    ) -> super::constraints::ConstraintSet<'db, 'c> {
        checker.check_callable_signature_pair(db, self.signatures, target.signatures)
    }

    pub(super) fn walk(self, db: &'db dyn Db, visitor: &(impl visitor::TypeVisitor<'db> + ?Sized)) {
        for signature in self.signatures {
            walk_signature(db, signature, visitor);
        }
    }

    fn collect_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'_, 'db>,
    ) {
        self.signatures
            .find_legacy_typevars_impl(db, self.env, binding_context, typevars, visitor);
    }

    fn into_functools_partial_instance(self, db: &'db dyn Db) -> Type<'db> {
        let return_ty = self
            .signatures
            .overload_return_type_or_unknown(db, self.env);
        KnownClass::FunctoolsPartial.to_specialized_instance(db, self.env, &[return_ty])
    }

    pub(super) fn returns_non_subclass(self, db: &'db dyn Db, instance: Type<'db>) -> bool {
        self.signatures
            .iter()
            .any(|signature| !signature.return_ty.is_assignable_to(db, self.env, instance))
    }
}

impl<'db> CallableType<'db> {
    /// Bind only receivers whose annotations were supplied implicitly by their declaration.
    pub(super) fn bind_implicit_receiver(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        receiver: Type<'db>,
        self_type: Type<'db>,
    ) -> Self {
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => self.with_signatures(
                db,
                CallableSignature::from_overloads(signatures.iter().map(|signature| {
                    if signature.has_implicit_positional_receiver_annotation() {
                        signature.bind_self_with_receiver(db, env, Some(receiver), Some(self_type))
                    } else {
                        signature.clone()
                    }
                })),
            ),

            SignatureSource::Projected(projection) => self.with_signature_source(
                db,
                SignatureSource::Projected(ProjectedSignature {
                    source: projection.source,
                    callable: projection.callable.map(|callable| {
                        callable.bind_implicit_receiver(db, env, receiver, self_type)
                    }),
                }),
            ),
        }
    }

    /// Merge stored approximations without evaluating a deferred signature.
    pub(super) fn normalize_stored_cycle(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Option<Self> {
        match (self.signature_source(db), previous.signature_source(db)) {
            (SignatureSource::Explicit(current), SignatureSource::Explicit(previous)) => {
                Some(self.with_signatures(db, current.cycle_normalized(db, env, previous, cycle)))
            }
            (SignatureSource::Projected(current), SignatureSource::Projected(previous))
                if current.source == previous.source =>
            {
                Some(self.with_signature_source(
                    db,
                    SignatureSource::Projected(ProjectedSignature {
                        source: current.source,
                        callable: match (current.callable, previous.callable) {
                            (Some(current), Some(previous)) => {
                                Some(current.normalize_stored_cycle(db, env, previous, cycle)?)
                            }
                            (current, previous) => current.or(previous),
                        },
                    }),
                ))
            }
            _ => None,
        }
    }
    /// Inspect parameters and their declaration metadata without opening a deferred return.
    pub(super) fn parameter_signatures(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = CallableParameters<'db>> {
        match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => {
                signatures.iter().map(|signature| CallableParameters {
                    parameters: signature.parameters(),
                    declaration: Some(signature),
                })
            }
        }
    }

    /// Preserve the established bottom approximation for exact constructor cycles. A growing
    /// specialization has no finite signature, so retain its source as an incomplete projection.
    pub(super) fn constructor_cycle(db: &'db dyn Db, source: Type<'db>, kind: CycleKind) -> Self {
        match kind {
            CycleKind::Exact => Self::bottom(db),
            CycleKind::MatchingIdentity | CycleKind::Reentrant => {
                Self::incomplete_projection(db, source)
            }
        }
    }

    /// Retain the source of an unfinished conversion without manufacturing a complete signature.
    fn incomplete_projection(db: &'db dyn Db, source: Type<'db>) -> Self {
        Self::new_internal(
            db,
            SignatureSource::<CallableSignature<'db>>::Projected(ProjectedSignature {
                source,
                callable: None,
            }),
            CallableTypeKind::Regular,
            None,
        )
    }

    pub(super) fn bind_protocol_receiver(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        self_type: Option<Type<'db>>,
    ) -> Self {
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => self.with_signatures(
                db,
                signatures.bind_self_with_receiver(db, env, self_type, self_type),
            ),

            SignatureSource::Projected(projection) => self.with_signature_source(
                db,
                SignatureSource::Projected(ProjectedSignature {
                    source: projection.source,
                    callable: projection
                        .callable
                        .map(|callable| callable.bind_protocol_receiver(db, env, self_type)),
                }),
            ),
        }
        .into_regular(db)
    }

    fn parameter_source(self, db: &'db dyn Db) -> ParameterSource<'db> {
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => ParameterSource::Explicit(signatures),

            SignatureSource::Projected(projection) => projection
                .callable
                .unwrap_or_else(|| {
                    Self::new(
                        db,
                        CallableSignature::from_overloads([]),
                        CallableTypeKind::Regular,
                    )
                })
                .parameter_source(db),
        }
    }

    /// Read declaration metadata without exposing a projected signature's return type.
    pub(super) fn generic_contexts(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = Option<super::GenericContext<'db>>> {
        match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => {
                signatures.iter().map(|signature| signature.generic_context)
            }
        }
    }

    /// Change which variables a signature binds, retaining its projection and deferred returns.
    pub(super) fn map_generic_contexts(
        self,
        db: &'db dyn Db,
        mut map: impl FnMut(Option<super::GenericContext<'db>>) -> Option<super::GenericContext<'db>>,
    ) -> Self {
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => self.with_signatures(
                db,
                CallableSignature::from_overloads(signatures.iter().map(|signature| {
                    let mut signature = signature.clone();
                    signature.generic_context = map(signature.generic_context);
                    signature
                })),
            ),

            SignatureSource::Projected(projection) => self.with_signature_source(
                db,
                SignatureSource::Projected(ProjectedSignature {
                    source: projection.source,
                    callable: projection
                        .callable
                        .map(|callable| callable.map_generic_contexts(db, map)),
                }),
            ),
        }
    }

    pub(super) fn map_recursive_references(
        self,
        db: &'db dyn Db,
        mapper: &RecursiveReferenceMapper<'db>,
    ) -> Self {
        let source = match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => {
                SignatureSource::Explicit(signatures.map_recursive_references(db, mapper))
            }

            SignatureSource::Projected(projection) => {
                SignatureSource::Projected(ProjectedSignature {
                    source: mapper.map_type(db, projection.source),
                    callable: projection
                        .callable
                        .map(|callable| callable.map_recursive_references(db, mapper)),
                })
            }
        };
        self.with_signature_source(db, source)
    }

    /// Construct a callable with explicit signatures and the given binding behavior.
    pub(crate) fn new<S>(db: &'db dyn Db, signatures: S, kind: CallableTypeKind) -> Self
    where
        SignatureSource<'db, S>: salsa::Lookup<SignatureSource<'db>> + std::hash::Hash,
        SignatureSource<'db>: salsa::HashEqLike<SignatureSource<'db, S>>,
    {
        Self::new_internal(db, SignatureSource::Explicit(signatures), kind, None)
    }

    /// Replace return types without evaluating a deferred body that will be discarded.
    fn with_return_type(self, db: &'db dyn Db, return_ty: Type<'db>) -> Self {
        let signatures = match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => CallableSignature::from_overloads(
                signatures
                    .iter()
                    .cloned()
                    .map(|signature| signature.with_return_type(return_ty)),
            ),
        };
        self.with_signatures(db, signatures)
    }

    /// Read signatures already stored on this callable without inferring a deferred body.
    fn explicit_signatures(self, db: &'db dyn Db) -> Option<&'db CallableSignature<'db>> {
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => Some(signatures),
            SignatureSource::Projected(_) => None,
        }
    }

    /// Inspect signatures while their expansion remains active for the entire operation.
    /// Recursive helpers use the caller's context, and an active expansion returns `None`.
    pub(super) fn inspect_signatures<R>(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        inspect: impl FnOnce(ExpandedSignatures<'_, 'db>) -> R,
    ) -> Option<R> {
        let context = env.context();
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => {
                Some(inspect(ExpandedSignatures { signatures, env }))
            }

            SignatureSource::Projected(projection) => context
                .expand_signature(db, env, self, || {
                    projection.callable?.inspect_signatures(db, env, inspect)
                })
                .flatten(),
        }
    }

    /// Inspect a signature under the caller's existing recursion policy.
    ///
    /// The operation keeps its guard active while consuming the signature. Substitution has
    /// a separate guard: comparisons track pairs, and rendering retains a bounded prefix of
    /// recursive specializations, so neither can use a single type as its traversal key.
    pub(super) fn inspect_signatures_in_operation<R, Mode>(
        self,
        db: &'db dyn Db,
        expansion: &TypeExpansionContext<'_, '_, 'db, Mode>,
        inspect: impl FnOnce(ExpandedSignatures<'_, 'db>) -> R,
    ) -> Option<R> {
        let signatures = match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => Cow::Borrowed(signatures),

            SignatureSource::Projected(projection) => {
                Cow::Owned(expansion.expand_signature(db, self, |env| {
                    projection
                        .callable?
                        .inspect_signatures(db, env, |signatures| signatures.signatures.clone())
                })??)
            }
        };
        Some(inspect(ExpandedSignatures {
            signatures: &signatures,
            env: expansion.environment(),
        }))
    }

    /// Start a complete signature inspection. Recursive consumers keep the supplied inspector.
    pub(crate) fn inspect_signatures_root<R>(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        inspect: impl FnOnce(
            &super::recursive::TypeInspector<'_, 'db>,
            ExpandedSignatures<'_, 'db>,
        ) -> R,
    ) -> Option<R> {
        super::recursive::TypeInspector::scope(
            env,
            TypeInspectionKey::InspectSignaturesRoot,
            |traversal| {
                traversal.inspect_signatures(db, self, |signatures| inspect(traversal, signatures))
            },
        )
    }

    /// Return the variance of this callable's parameter and return types.
    pub(super) fn variance_of(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => signatures.variance_of(db, env, typevar),

            SignatureSource::Projected(projection) => projection
                .callable
                .map_or(super::TypeVarVariance::Invariant.into(), |callable| {
                    callable.variance_of(db, env, typevar)
                }),
        }
    }

    /// Inspect every parameter list without evaluating a deferred return type.
    pub(crate) fn parameter_lists(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = &'db Parameters<'db>> {
        match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => {
                signatures.overloads.iter().map(Signature::parameters)
            }
        }
    }

    /// Whether any stored signature declares type parameters.
    pub(crate) fn has_generic_context(self, db: &'db dyn Db) -> bool {
        match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => signatures
                .iter()
                .any(|signature| signature.generic_context.is_some()),
        }
    }

    /// Return parameters when exactly one overload is available.
    pub(crate) fn single_parameters(self, db: &'db dyn Db) -> Option<&'db Parameters<'db>> {
        match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => {
                let [signature] = signatures.overloads.as_slice() else {
                    return None;
                };
                Some(signature.parameters())
            }
        }
    }

    pub(crate) fn with_deprecated(self, db: &'db dyn Db, deprecated: OverloadLiteral<'db>) -> Self {
        Self::new_internal(
            db,
            self.signature_source(db),
            self.kind(db),
            Some(deprecated),
        )
    }

    /// Replace the signatures without losing binding behavior or deprecation metadata.
    pub(crate) fn with_signatures<S>(self, db: &'db dyn Db, signatures: S) -> Self
    where
        SignatureSource<'db, S>: salsa::Lookup<SignatureSource<'db>> + std::hash::Hash,
        SignatureSource<'db>: salsa::HashEqLike<SignatureSource<'db, S>>,
    {
        Self::new_internal(
            db,
            SignatureSource::Explicit(signatures),
            self.kind(db),
            self.deprecated(db),
        )
    }

    pub(crate) fn with_kind(self, db: &'db dyn Db, kind: CallableTypeKind) -> Self {
        Self::new_internal(db, self.signature_source(db), kind, self.deprecated(db))
    }

    pub(crate) fn single(db: &'db dyn Db, signature: Signature<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(signature),
            CallableTypeKind::Regular,
        )
    }

    pub(crate) fn function_like(db: &'db dyn Db, signature: Signature<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(signature),
            CallableTypeKind::FunctionLike,
        )
    }

    fn paramspec_value(db: &'db dyn Db, parameters: Parameters<'db>) -> CallableType<'db> {
        Self::paramspec_value_from_signatures(
            db,
            CallableSignature::single(Signature::new(parameters, Type::unknown())),
        )
    }

    pub(super) fn paramspec_value_from_signatures(
        db: &'db dyn Db,
        signatures: CallableSignature<'db>,
    ) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::from_overloads(
                signatures
                    .overloads
                    .into_iter()
                    .map(Signature::into_paramspec_value),
            ),
            CallableTypeKind::ParamSpecValue,
        )
    }

    pub(crate) fn is_bottom_paramspec_value(self, db: &'db dyn Db) -> bool {
        if self.kind(db) != CallableTypeKind::ParamSpecValue {
            return false;
        }
        let Some(signatures) = self.explicit_signatures(db) else {
            return false;
        };
        let [signature] = signatures.overloads.as_slice() else {
            return false;
        };
        signature.parameters().is_bottom()
    }

    pub(crate) fn is_top_paramspec_value(self, db: &'db dyn Db) -> bool {
        if self.kind(db) != CallableTypeKind::ParamSpecValue {
            return false;
        }
        let Some(signatures) = self.explicit_signatures(db) else {
            return false;
        };
        let [signature] = signatures.overloads.as_slice() else {
            return false;
        };
        signature.parameters().is_top()
    }

    /// Create a callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> CallableType<'db> {
        Self::single(db, Signature::unknown())
    }

    /// Create the fully static `Top[Callable[..., object]]` type.
    pub(crate) fn top(db: &'db dyn Db) -> CallableType<'db> {
        Self::single(db, Signature::new(Parameters::top(), Type::object()))
    }

    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::FunctionLike)
    }

    pub(super) fn runtime_class(self, db: &'db dyn Db) -> Option<KnownClass> {
        match self.kind(db) {
            CallableTypeKind::FunctionLike => Some(KnownClass::FunctionType),
            CallableTypeKind::StaticMethodLike => Some(KnownClass::Staticmethod),
            CallableTypeKind::ClassMethodLike => Some(KnownClass::Classmethod),
            _ => None,
        }
    }

    pub(super) fn is_dunder_paramspec(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::DunderParamSpec)
    }

    pub(crate) fn is_regular(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::Regular)
    }

    pub(crate) fn is_classmethod_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::ClassMethodLike)
    }

    pub(crate) fn is_staticmethod_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::StaticMethodLike)
    }

    /// Returns `true` if this callable represents a function used as a class member.
    pub fn is_method_like(self, db: &'db dyn Db) -> bool {
        matches!(
            self.kind(db),
            CallableTypeKind::FunctionLike
                | CallableTypeKind::StaticMethodLike
                | CallableTypeKind::ClassMethodLike
        )
    }

    pub(crate) fn into_regular(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::Regular)
    }

    /// Retain every parameter signature and its generic context, but erase return types
    /// that do not participate in a `ParamSpec` specialization.
    pub(crate) fn into_paramspec_value(self, db: &'db dyn Db) -> CallableType<'db> {
        let signatures = match self.parameter_source(db) {
            ParameterSource::Explicit(signatures) => signatures.clone(),
        };
        Self::paramspec_value_from_signatures(db, signatures)
    }

    /// Returns the reduced callable produced by partially applying selected overloads.
    pub(crate) fn partially_apply(
        db: &'db dyn Db,
        overloads: impl IntoIterator<Item = PartialSignatureApplication<'db>>,
    ) -> Option<Self> {
        let overloads: SmallVec<[_; 1]> = overloads.into_iter().collect();
        Some(Self::new(
            db,
            CallableSignature::partially_apply(db, overloads)?,
            CallableTypeKind::Regular,
        ))
    }

    /// Reifies this callable as the nominal `functools.partial[T]` instance for its return type.
    pub(crate) fn into_functools_partial_instance(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Type<'db> {
        self.inspect_signatures(db, env, |signatures| {
            signatures.into_functools_partial_instance(db)
        })
        .unwrap_or_else(|| {
            KnownClass::FunctoolsPartial.to_specialized_instance(db, env, &[Type::unknown()])
        })
    }

    /// Wraps this reduced callable as a synthetic `functools.partial(...)` instance type.
    pub(crate) fn into_precise_functools_partial_instance(
        self,
        db: &'db dyn Db,
        wrapped: Type<'db>,
    ) -> Type<'db> {
        Type::KnownInstance(KnownInstanceType::FunctoolsPartial(
            FunctoolsPartialInstance::new(db, InternedType::new(db, wrapped), self),
        ))
    }

    /// Binds a method receiver, specializing its signatures and removing incompatible overloads.
    ///
    /// `typing_self_type` is used to replace `typing.Self`, which differs from `receiver_type`
    /// for class methods.
    pub(super) fn bind_self(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        receiver_type: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> CallableType<'db> {
        let source = match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => SignatureSource::Explicit(
                signatures.bind_method_receiver(db, env, receiver_type, typing_self_type),
            ),

            SignatureSource::Projected(projection) => {
                SignatureSource::Projected(ProjectedSignature {
                    source: projection.source,
                    callable: projection.callable.map(|callable| {
                        callable.bind_self(db, env, receiver_type, typing_self_type)
                    }),
                })
            }
        };
        Self::new_internal(db, source, CallableTypeKind::Regular, self.deprecated(db))
    }

    pub(crate) fn into_function_like(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::FunctionLike)
    }

    pub(crate) fn into_dunder_paramspec(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::DunderParamSpec)
    }

    pub(crate) fn apply_self_with_receiver(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        receiver_type: Type<'db>,
        self_type: Type<'db>,
    ) -> CallableType<'db> {
        let source = match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => SignatureSource::Explicit(
                signatures.apply_self_with_receiver(db, env, receiver_type, self_type),
            ),

            SignatureSource::Projected(projection) => {
                SignatureSource::Projected(ProjectedSignature {
                    source: projection.source,
                    callable: projection.callable.map(|callable| {
                        callable.apply_self_with_receiver(db, env, receiver_type, self_type)
                    }),
                })
            }
        };
        Self::new_internal(db, source, self.kind(db), self.deprecated(db))
    }

    /// Create a callable type which represents a fully-static "bottom" callable.
    ///
    /// Specifically, this represents a callable type with a single signature:
    /// `(*args: object, **kwargs: object) -> Never`.
    pub(crate) fn bottom(db: &'db dyn Db) -> CallableType<'db> {
        Self::new(db, CallableSignature::bottom(), CallableTypeKind::Regular)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => self.with_signatures(
                db,
                signatures.recursive_type_normalized_impl(db, env, div, nested)?,
            ),

            SignatureSource::Projected(projection) => self.with_signature_source(
                db,
                SignatureSource::Projected(ProjectedSignature {
                    source: projection
                        .source
                        .recursive_type_normalized_impl(db, env, div, nested)?,
                    callable: match projection.callable {
                        Some(callable) => {
                            Some(callable.recursive_type_normalized_impl(db, env, div, nested)?)
                        }
                        None => None,
                    },
                }),
            ),
        })
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        if let TypeMapping::RescopeReturnCallables(replacements) = type_mapping {
            return replacements.get(&self).copied().unwrap_or(self);
        }

        let source = match self.signature_source(db) {
            SignatureSource::Explicit(signatures) => {
                let mapped = signatures.apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                if &mapped == signatures {
                    return self;
                }
                SignatureSource::Explicit(mapped)
            }

            SignatureSource::Projected(projection) => {
                SignatureSource::Projected(ProjectedSignature {
                    source: projection.source.apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ),
                    callable: projection.callable.map(|callable| {
                        callable.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                    }),
                })
            }
        };
        Self::new_internal(db, source, self.kind(db), self.deprecated(db))
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'_, 'db>,
    ) {
        visitor.visit_structure(db, Type::Callable(self), |expansion| {
            expansion.inspect_signatures(db, self, |signatures| {
                signatures.collect_legacy_typevars(db, binding_context, typevars, visitor);
            });
        });
    }
}

/// Converting a type "into a callable" can possibly return a _union_ of callables. Eventually,
/// when coercing that result to a single type, you'll get a `UnionType`. But this lets you handle
/// that result as a list of `CallableType`s before merging them into a `UnionType` should that be
/// helpful.
///
/// Note that this type is guaranteed to contain at least one callable. If you need to support "no
/// callables" as a possibility, use `Option<CallableTypes>`.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct CallableTypes<'db>(SmallVec<[CallableType<'db>; 1]>);

impl<'db> CallableTypes<'db> {
    fn new(mut callables: SmallVec<[CallableType<'db>; 1]>) -> Self {
        assert!(!callables.is_empty(), "CallableTypes should not be empty");
        // Repeated alternatives do not change a union. Removing them also lets recursive
        // constructor queries converge when each iteration adds the same `__init__` callable.
        if callables.len() > 1 {
            let mut seen = FxHashSet::default();
            callables.retain(|callable| seen.insert(*callable));
            callables.shrink_to_fit();
        }
        CallableTypes(callables)
    }

    pub(crate) fn one(callable: CallableType<'db>) -> Self {
        CallableTypes(smallvec_inline![callable])
    }

    pub(crate) fn from_elements(callables: impl IntoIterator<Item = CallableType<'db>>) -> Self {
        Self::new(callables.into_iter().collect())
    }

    pub(crate) fn exactly_one(&self) -> Option<CallableType<'db>> {
        match self.0.as_slice() {
            [single] => Some(*single),
            _ => None,
        }
    }

    fn into_inner(self) -> SmallVec<[CallableType<'db>; 1]> {
        self.0
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, CallableType<'db>> {
        self.0.iter()
    }

    /// Inspect the calling conventions of every alternative without inferring returns.
    pub(crate) fn parameter_lists(
        &self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = &'db Parameters<'db>> {
        self.0
            .iter()
            .flat_map(move |callable| callable.parameter_lists(db))
    }

    pub(crate) fn to_type(&self, db: &'db dyn Db, env: &TypeOperations<'db>) -> Type<'db> {
        assert!(!self.0.is_empty(), "CallableTypes should not be empty");
        UnionType::from_elements(db, env, self.0.iter().copied().map(Type::Callable))
    }

    pub(crate) fn map(self, mut f: impl FnMut(CallableType<'db>) -> CallableType<'db>) -> Self {
        Self::from_elements(self.0.iter().map(|element| f(*element)))
    }

    /// Merges reduced callables into one precise `functools.partial(...)` instance type.
    pub(crate) fn into_precise_functools_partial_instance(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        wrapped: Type<'db>,
    ) -> Type<'db> {
        if let Some(callable) = self.exactly_one() {
            return callable.into_precise_functools_partial_instance(db, wrapped);
        }
        let mut overloads = Vec::new();
        let mut seen_overloads = FxHashSet::default();

        for callable in self.0 {
            callable.inspect_signatures_root(db, env, |_, signatures| {
                for signature in signatures.signatures {
                    let signature = signature.clone();
                    let dedup_key = signature
                        .clone()
                        .with_definition(None)
                        .with_source_overload_index(None);
                    if seen_overloads.insert(dedup_key) {
                        overloads.push(signature);
                    }
                }
            });
        }

        debug_assert!(!overloads.is_empty(), "CallableTypes should not be empty");

        CallableType::new(
            db,
            CallableSignature::from_overloads(overloads),
            CallableTypeKind::Regular,
        )
        .into_precise_functools_partial_instance(db, wrapped)
    }
}

impl<'a, 'db> IntoIterator for &'a CallableTypes<'db> {
    type IntoIter = std::slice::Iter<'a, CallableType<'db>>;
    type Item = &'a CallableType<'db>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'db> CallableType<'db> {
    fn with_signature_source(self, db: &'db dyn Db, source: SignatureSource<'db>) -> Self {
        Self::new_internal(db, source, self.kind(db), self.deprecated(db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::tests::setup_db;
    use crate::types::{Parameter, Type};

    #[test]
    fn owned_and_borrowed_signatures_share_callable_identity() {
        let db = setup_db();
        let signatures =
            CallableSignature::single(Signature::new(Parameters::empty(), Type::object()));
        let owned = CallableType::new(&db, signatures.clone(), CallableTypeKind::Regular);
        let borrowed = CallableType::new(&db, &signatures, CallableTypeKind::Regular);
        assert_eq!(owned, borrowed);

        let replacement = CallableSignature::single(Signature::dynamic(Type::unknown()));
        let borrowed = owned.with_signatures(&db, &replacement);
        let owned = CallableType::new(&db, replacement, CallableTypeKind::Regular);
        assert_eq!(owned, borrowed);
    }

    #[test]
    fn paramspec_value_materializations_do_not_add_return_type() {
        let db = setup_db();
        let env = db.program_environment();
        let paramspec_value = Type::paramspec_value_callable(
            &db,
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::object())
            ]),
        );
        let bottom = paramspec_value.bottom_materialization(&db, &env);
        assert_eq!(paramspec_value, bottom);
        let top = paramspec_value.top_materialization(&db, &env);
        assert_eq!(paramspec_value, top);
    }
}
