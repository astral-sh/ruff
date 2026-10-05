pub(in crate::types) mod conversion;
pub(super) mod evaluation;
pub(in crate::types) mod function_descriptor;
#[cfg(feature = "experimental-analysis")]
mod runtime;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) use runtime::register_callable_values;
#[cfg(test)]
pub(crate) mod scheduled_probe;

use self::function_descriptor::{
    FunctionBindingFacts, OrdinaryFunctionBinding, bind_function_descriptor_sync,
    function_like_kind_sync, underlying_function_sync,
};
use crate::ProgramEnvironment;
use crate::types::signatures::effects::{
    LegacyInlineEffects, SignatureEffects, SignatureResult, legacy_inline,
};
use rustc_hash::FxHashSet;
use smallvec::{SmallVec, smallvec_inline};
use std::ops::ControlFlow;

use crate::{
    Db,
    types::{
        ApplyTypeMappingVisitor, DescriptorOrigin, FunctionType, InternedType, KnownClass,
        KnownInstanceType, Parameters, Signature, Type,
        TypeContext, TypeMapping, UnionType,
        constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet},
        cyclic::{CallableExpansion, CallableRecursionGuard},
        function::OverloadLiteral,
        known_instance::FunctoolsPartialInstance,
        relation::{TypeRelation, TypeRelationChecker},
        signatures::{CallableSignature, PartialSignatureApplication},
        visitor, walk_signature,
    },
};
use ty_python_core::definition::Definition;

/// Semantic dependencies reached while converting a type into its callable signatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableConversionOperation {
    RecursionGuard,
    GuardCycle,
    RecursiveReference,
    DynamicSignature,
    RecursiveTypeUnfold,
    ClassIdentity,
    NewTypeBase,
    SubclassInstance,
    ProtocolConstructor,
    TypeVarBoundOrConstraints,
    EnumInstance,
    TypeAliasValue,
    KnownBoundMethod,
    WrapperSignature,
    MethodWrapper,
    PartialSignature,
    NewTypeSignature,
    IntersectionAlternatives,
    EnumComplement,
    Continuation,
    RuntimeUnion,
    Constructor,
    CallMember,
    BoundMethod,
}

/// The semantic inputs to callable conversion, independent of its evaluation stack.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct CallableConversionRequest<'db> {
    ty: Type<'db>,
    policy: UpcastPolicy,
    recursive_definition: Option<Definition<'db>>,
    /// Applies to an unknown callable value, not a concrete callable's return annotation.
    unknown_is_recovery: bool,
}

impl<'db> CallableConversionRequest<'db> {
    #[cfg(test)]
    pub(super) fn source_type(self) -> Type<'db> {
        self.ty
    }

    pub(super) fn new(ty: Type<'db>, policy: UpcastPolicy) -> Self {
        Self {
            ty,
            policy,
            recursive_definition: None,
            unknown_is_recovery: false,
        }
    }

    pub(in crate::types) fn requires_recursion_guard(self) -> bool {
        matches!(
            self.ty,
            Type::NominalInstance(_) | Type::ProtocolInstance(_)
        )
    }

    fn is_recursive_reference(self, db: &'db dyn Db, function: FunctionType<'db>) -> bool {
        self.recursive_definition
            .is_some_and(|definition| function.contains_definition(db, definition))
    }

    pub(super) fn from_descriptor(ty: Type<'db>, origin: DescriptorOrigin<'db>) -> Self {
        Self {
            unknown_is_recovery: origin.return_contains_recursive_recovery,
            ..Self::new(ty, UpcastPolicy::default())
        }
    }

    pub(super) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Option<CallableTypes<'db>> {
        self.ty.try_upcast_to_callable_with_policy_and_context(
            db,
            env,
            self.policy,
            CallableUpcastContext {
                recursive_definition: self.recursive_definition,
                recursion_guard,
                unknown_is_recovery: self.unknown_is_recovery,
            },
        )
    }
}

impl<'db> Type<'db> {
    pub(super) fn function_like_kind(self, db: &'db dyn Db) -> Option<CallableTypeKind> {
        match function_like_kind_sync(self, FunctionBindingFacts, &OrdinaryFunctionBinding { db }) {
            Ok(kind) => kind,
            Err(error) => match error {},
        }
    }

    /// Returns the function exposed by descriptor access or a bound method's `__func__`.
    pub(super) fn underlying_function(self, db: &'db dyn Db) -> Type<'db> {
        match underlying_function_sync(self, FunctionBindingFacts, &OrdinaryFunctionBinding { db })
        {
            Ok(function) => function,
            Err(error) => match error {},
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
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        match function_descriptor_sync(
            self,
            env,
            instance,
            owner,
            &InlineFunctionDescriptor { db, env },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    fn bind_function_descriptor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        match bind_function_descriptor_sync(
            self,
            env,
            instance,
            owner,
            &OrdinaryFunctionBinding { db },
        ) {
            Ok(bound) => bound,
            Err(error) => match error {},
        }
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
        env: &ProgramEnvironment<'db>,
    ) -> Option<CallableTypes<'db>> {
        self.try_upcast_to_callable_with_policy(db, env, UpcastPolicy::default())
    }

    pub(crate) fn try_upcast_to_callable_with_recursive_fallback(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursive_definition: Option<Definition<'db>>,
    ) -> Option<CallableTypes<'db>> {
        self.try_upcast_to_callable_with_policy_and_context(
            db,
            env,
            UpcastPolicy::default(),
            CallableUpcastContext {
                recursive_definition,
                ..CallableUpcastContext::default()
            },
        )
    }

    pub(super) fn try_upcast_to_callable_from_descriptor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Option<CallableTypes<'db>> {
        CallableConversionRequest::from_descriptor(self, origin).evaluate(
            db,
            env,
            Some(recursion_guard),
        )
    }

    pub(crate) fn try_upcast_to_callable_with_policy(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        policy: UpcastPolicy,
    ) -> Option<CallableTypes<'db>> {
        self.try_upcast_to_callable_with_policy_and_context(
            db,
            env,
            policy,
            CallableUpcastContext::default(),
        )
    }

    fn try_upcast_to_callable_with_policy_and_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        policy: UpcastPolicy,
        context: CallableUpcastContext<'_, 'db>,
    ) -> Option<CallableTypes<'db>> {
        if context.recursion_guard.is_none()
            && CallableConversionRequest::new(self, policy).requires_recursion_guard()
        {
            let recursion_guard = CallableRecursionGuard::for_constructor(db, env, self);
            return self.try_upcast_to_callable_with_policy_and_context(
                db,
                env,
                policy,
                CallableUpcastContext {
                    recursion_guard: Some(&recursion_guard),
                    ..context
                },
            );
        }
        if let Some(recursion_guard) = context.recursion_guard {
            return recursion_guard.visit(
                db,
                env,
                (CallableExpansion::Upcast, self),
                || Some(CallableTypes::one(CallableType::bottom(db))),
                || {
                    Some(CallableTypes::one(CallableType::single(
                        db,
                        Signature::recursion_recovery(),
                    )))
                },
                #[cfg(test)]
                || {
                    Some(CallableTypes::one(CallableType::single(
                        db,
                        Signature::unknown(),
                    )))
                },
                || self.try_upcast_to_callable_impl(db, env, policy, context),
            );
        }
        self.try_upcast_to_callable_impl(db, env, policy, context)
    }

    fn try_upcast_to_callable_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        policy: UpcastPolicy,
        context: CallableUpcastContext<'_, 'db>,
    ) -> Option<CallableTypes<'db>> {
        evaluation::conversion(
            db,
            env,
            CallableConversionRequest {
                ty: self,
                policy,
                recursive_definition: context.recursive_definition,
                unknown_is_recovery: context.unknown_is_recovery,
            },
            context.recursion_guard,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CallableUpcastContext<'a, 'db> {
    recursive_definition: Option<Definition<'db>>,
    recursion_guard: Option<&'a CallableRecursionGuard<'db>>,
    /// Applies to an unknown callable value, not a concrete callable's return annotation.
    unknown_is_recovery: bool,
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

impl CallableTypeKind {
    pub(in crate::types) fn runtime_class(self) -> Option<KnownClass> {
        match self {
            CallableTypeKind::FunctionLike => Some(KnownClass::FunctionType),
            CallableTypeKind::StaticMethodLike => Some(KnownClass::Staticmethod),
            CallableTypeKind::ClassMethodLike => Some(KnownClass::Classmethod),
            _ => None,
        }
    }
}

impl From<TypeRelation> for UpcastPolicy {
    fn from(relation: TypeRelation) -> Self {
        match relation {
            TypeRelation::Subtyping
            | TypeRelation::Redundancy { .. }
            | TypeRelation::SubtypingAssuming => UpcastPolicy::Sound,
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
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct CallableType<'db> {
    #[returns(ref)]
    pub(crate) signatures: CallableSignature<'db>,

    #[returns(copy)]
    pub(super) kind: CallableTypeKind,

    /// The declaration on which `@deprecated` wrapped this callable. Retain the declaration
    /// for diagnostic names, source annotations, and deduplication, independently of binding kind.
    #[returns(copy)]
    pub(crate) deprecated: Option<OverloadLiteral<'db>>,
}

pub(super) fn walk_callable_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    ty: CallableType<'db>,
    visitor: &V,
) {
    for signature in &ty.signatures(db).overloads {
        walk_signature(db, signature, visitor);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CallableType<'_> {}

impl<'db> CallableType<'db> {
    pub(crate) fn new<S>(db: &'db dyn Db, signatures: S, kind: CallableTypeKind) -> Self
    where
        S: salsa::Lookup<CallableSignature<'db>> + std::hash::Hash,
        CallableSignature<'db>: salsa::HashEqLike<S>,
    {
        Self::new_internal(db, signatures, kind, None)
    }

    pub(crate) fn with_deprecated(self, db: &'db dyn Db, deprecated: OverloadLiteral<'db>) -> Self {
        Self::new_internal(db, self.signatures(db), self.kind(db), Some(deprecated))
    }

    /// Replace the signatures without losing binding behavior or deprecation metadata.
    pub(crate) fn with_signatures<S>(self, db: &'db dyn Db, signatures: S) -> Self
    where
        S: salsa::Lookup<CallableSignature<'db>> + std::hash::Hash,
        CallableSignature<'db>: salsa::HashEqLike<S>,
    {
        Self::new_internal(db, signatures, self.kind(db), self.deprecated(db))
    }

    pub(crate) fn with_kind(self, db: &'db dyn Db, kind: CallableTypeKind) -> Self {
        Self::new_internal(db, self.signatures(db), kind, self.deprecated(db))
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
        let [signature] = self.signatures(db).overloads.as_slice() else {
            return false;
        };
        signature.parameters().is_bottom()
    }

    pub(crate) fn is_top_paramspec_value(self, db: &'db dyn Db) -> bool {
        if self.kind(db) != CallableTypeKind::ParamSpecValue {
            return false;
        }
        let [signature] = self.signatures(db).overloads.as_slice() else {
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
        self.kind(db).runtime_class()
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
        FunctionBindingFacts.is_method_like(self.kind(db))
    }

    pub(crate) fn into_regular(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::Regular)
    }

    /// Retain every parameter signature and its generic context, but erase return types
    /// that do not participate in a `ParamSpec` specialization.
    pub(crate) fn into_paramspec_value(self, db: &'db dyn Db) -> CallableType<'db> {
        Self::paramspec_value_from_signatures(db, self.signatures(db).clone())
    }

    /// Returns the reduced callable produced by partially applying selected overloads.
    pub(crate) fn partially_apply(
        db: &'db dyn Db,
        overloads: impl IntoIterator<Item = PartialSignatureApplication<'db>>,
    ) -> Option<Self> {
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
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        let return_ty = self.signatures(db).overload_return_type_or_unknown(db, env);
        KnownClass::FunctoolsPartial.to_specialized_instance(db, env, &[return_ty])
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
        env: &ProgramEnvironment<'db>,
        receiver_type: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> CallableType<'db> {
        Self::new_internal(
            db,
            self.signatures(db)
                .bind_method_receiver(db, env, receiver_type, typing_self_type),
            CallableTypeKind::Regular,
            self.deprecated(db),
        )
    }

    pub(crate) fn into_function_like(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::FunctionLike)
    }

    pub(crate) fn apply_self_with_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver_type: Type<'db>,
        self_type: Type<'db>,
    ) -> CallableType<'db> {
        self.with_signatures(
            db,
            self.signatures(db)
                .apply_self_with_receiver(db, env, receiver_type, self_type),
        )
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
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(
            self.with_signatures(
                db,
                self.signatures(db)
                    .recursive_type_normalized_impl(db, env, div, nested)?,
            ),
        )
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        legacy_inline(crate::types::signatures::mapping::map_callable_type_with(
            db,
            self,
            type_mapping,
            tcx,
            visitor,
            &crate::types::signatures::mapping::InlineSignatureMappingEffects,
        ))
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
    pub(super) fn new(mut callables: SmallVec<[CallableType<'db>; 1]>) -> Self {
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

    /// Iterates over every signature of every callable alternative without merging the
    /// alternatives into an overloaded callable.
    pub(crate) fn signatures(&self, db: &'db dyn Db) -> impl Iterator<Item = &'db Signature<'db>> {
        self.0
            .iter()
            .flat_map(move |callable| callable.signatures(db))
    }

    pub(crate) fn to_type(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
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
        wrapped: Type<'db>,
    ) -> Type<'db> {
        let mut overloads = Vec::new();
        let mut seen_overloads = FxHashSet::default();

        for callable in self.0 {
            for signature in callable.signatures(db) {
                let signature = signature.clone();
                let dedup_key = signature
                    .clone()
                    .with_definition(None)
                    .with_source_overload_index(None);
                if seen_overloads.insert(dedup_key) {
                    overloads.push(signature);
                }
            }
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

impl<'state, 'c, 'db> TypeRelationChecker<'state, 'c, 'db> {
    /// Check whether one callable type has the given relation to another callable type.
    ///
    /// See [`Type::is_subtype_of`] and [`Type::is_assignable_to`] for more details.
    pub(super) fn check_callable_pair(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        legacy_inline(self.check_callable_pair_with(db, &LegacyInlineEffects, source, target))
    }

    pub(crate) async fn check_callable_pair_with<E: SignatureEffects<'state, 'db, 'c>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> SignatureResult<'db, 'c, E::Error> {
        effects
            .local(
                Some(4),
                size_of::<CallableType<'db>>().checked_mul(2),
                || (),
            )
            .await?;
        let target_class = effects.callable_runtime_class(db, target).await?;
        if target_class.is_some()
            && target_class != effects.callable_runtime_class(db, source).await?
        {
            return effects.local(Some(1), Some(0), || self.never()).await;
        }
        let source = effects.callable_signatures(db, source).await?;
        let target = effects.callable_signatures(db, target).await?;
        effects
            .local(
                Some(2),
                size_of::<&CallableSignature<'db>>().checked_mul(2),
                || (),
            )
            .await?;
        self.check_callable_signature_pair_with(db, effects, source, target)
            .await
    }

    pub(super) fn check_callables_vs_callable(
        &self,
        db: &'db dyn Db,
        source: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        legacy_inline(self.check_callables_vs_callable_with(
            db,
            &LegacyInlineEffects,
            source,
            target,
        ))
    }

    pub(crate) async fn check_callables_vs_callable_with<E: SignatureEffects<'state, 'db, 'c>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        source: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> SignatureResult<'db, 'c, E::Error> {
        let (mut fold, mut elements) = effects
            .local(Some(2), Some(0), || {
                (
                    ConstraintFold::new(self.constraints, ConstraintFoldKind::All),
                    source.into_iter(),
                )
            })
            .await?;
        while let Some(element) = effects
            .local(Some(4), Some(0), || {
                elements.next().copied()
            })
            .await?
        {
            let when = self
                .check_callable_pair_with(db, effects, element, target)
                .await?;
            if let ControlFlow::Break(when) = effects.push_constraints(db, &mut fold, when).await? {
                return Ok(when);
            }
        }
        effects.finish_constraints(db, &mut fold).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::tests::setup_db;
    use crate::types::{Parameter, Type};

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

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousFunctionDescriptorEffects)]
pub(in crate::types) trait FunctionDescriptorEffects<'db> {
 type Error;
 #[operation(checkpoint)]
 async fn checkpoint(&self) -> Result<(), Self::Error>;
 #[operation(child)]
 async fn union(&self, union: UnionType<'db>, instance: Option<Type<'db>>, owner: Option<Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
 #[operation(child)]
 async fn alias(&self, alias: crate::types::TypeAliasType<'db>, instance: Option<Type<'db>>, owner: Option<Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
 #[operation(child)]
 async fn bind(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, instance: Option<Type<'db>>, owner: Option<Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
}
#[synchronous(function_descriptor_sync)]
#[capabilities(effects = FunctionDescriptorEffects)]
#[passive_values(Type::Union, Type::TypeAlias, Type::FunctionLiteral, Type::Callable, Type::KnownInstance, KnownInstanceType::MethodWrapper)]
pub(in crate::types) async fn function_descriptor_with<'db, E: FunctionDescriptorEffects<'db>>(ty: Type<'db>, env: &ProgramEnvironment<'db>, instance: Option<Type<'db>>, owner: Option<Type<'db>>, effects: &E) -> Result<Option<Type<'db>>, E::Error> {
 effects.checkpoint().await?;
 match ty {
  // ParamSpec specialization can produce a union of function descriptors.
  Type::Union(union) => effects.union(union, instance, owner).await,
  Type::TypeAlias(alias) => effects.alias(alias, instance, owner).await,
  Type::FunctionLiteral(_) | Type::Callable(_) | Type::KnownInstance(KnownInstanceType::MethodWrapper(_)) => effects.bind(ty, env, instance, owner).await,
  _ => Ok(None),
 }
}
}
struct InlineFunctionDescriptor<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}
impl<'db> SynchronousFunctionDescriptorEffects<'db> for InlineFunctionDescriptor<'_, 'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn union(
        &self,
        union: UnionType<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(union.try_map(self.db, self.env, |alternative| {
            alternative.function_like_dunder_get(self.db, self.env, instance, owner)
        }))
    }
    fn alias(
        &self,
        alias: crate::types::TypeAliasType<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(alias
            .value_type(self.db)
            .function_like_dunder_get(self.db, self.env, instance, owner))
    }
    fn bind(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty.bind_function_descriptor(self.db, env, instance, owner))
    }
}
