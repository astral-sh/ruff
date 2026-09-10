use crate::ProgramEnvironment;
use rustc_hash::FxHashSet;
use smallvec::{SmallVec, smallvec_inline};

use crate::{
    Db, FxOrderSet,
    place::Place,
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, ClassType, FindLegacyTypeVarsVisitor,
        FunctionType, InternedType, KnownBoundMethodType, KnownClass, KnownInstanceType,
        LiteralValueTypeKind, MemberLookupPolicy, Parameter, Parameters, Signature,
        SubclassOfInner, Type, TypeContext, TypeMapping, TypeVarBoundOrConstraints, UnionType,
        constraints::{ConstraintSet, IteratorConstraintsExtension},
        function::OverloadLiteral,
        known_instance::{FunctoolsPartialInstance, MethodWrapperKind},
        relation::{TypeRelation, TypeRelationChecker},
        signatures::{CallableSignature, PartialSignatureApplication},
        visitor, walk_signature,
    },
};
use ty_python_core::definition::Definition;

impl<'db> Type<'db> {
    /// The function descriptor representation, independently of its literal or synthesized payload.
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

    /// Implements function, staticmethod and classmethod `__get__` for either payload form.
    pub(super) fn function_like_descriptor_get(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        // Specializing a stored `__get__` can expand a ParamSpec into several callable
        // alternatives. Each retains its own receiver and signature after binding.
        match self {
            Type::Union(union) => {
                return union.try_map(db, env, |alternative| {
                    alternative.function_like_descriptor_get(db, env, instance, owner)
                });
            }
            Type::TypeAlias(alias) => {
                return alias
                    .value_type(db)
                    .function_like_descriptor_get(db, env, instance, owner);
            }
            _ => {}
        }
        let kind = self.function_like_kind(db)?;
        let receiver = match kind {
            CallableTypeKind::StaticMethodLike => return Some(self.underlying_function(db)),
            CallableTypeKind::ClassMethodLike => owner
                .filter(|owner| !owner.is_none(db))
                .or_else(|| instance.map(|instance| instance.to_meta_type(db, env))),
            _ => instance,
        };
        Some(receiver.map_or_else(
            || self.underlying_function(db),
            |receiver| {
                Type::BoundMethod(super::BoundMethodType::from_callable(
                    db,
                    self,
                    env.program(db),
                    receiver,
                ))
            },
        ))
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
            },
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
        context: CallableUpcastContext<'db>,
    ) -> Option<CallableTypes<'db>> {
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context);
        }

        match self {
            Type::Callable(callable) => Some(CallableTypes::one(callable)),

            Type::Dynamic(_) => Some(CallableTypes::one(CallableType::function_like(
                db,
                Signature::dynamic(self),
            ))),
            Type::Divergent(_) => Some(CallableTypes::one(CallableType::function_like(
                db,
                Signature::dynamic(self),
            ))),

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

            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                let call_symbol = self
                    .member_lookup_with_policy(
                        db,
                        env,
                        "__call__",
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place;

                if let Place::Defined(place) = call_symbol
                    && place.is_definitely_defined()
                {
                    place
                        .ty
                        .try_upcast_to_callable_with_policy_and_context(db, env, policy, context)
                        // The callable instance itself doesn't inherit the descriptor behavior of
                        // its `__call__` method.
                        .map(|callables| callables.map(|callable| callable.into_regular(db)))
                } else {
                    None
                }
            }
            Type::ClassLiteral(class_literal) => {
                Some(class_literal.identity_specialization(db).into_callable(db))
            }

            Type::GenericAlias(alias) => Some(ClassType::Generic(alias).into_callable(db)),

            Type::NewTypeInstance(newtype) => newtype
                .concrete_base_type(db)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context),

            Type::SubclassOf(subclass_of_ty) if policy == UpcastPolicy::Sound => {
                Some(CallableTypes::one(CallableType::function_like(
                    db,
                    Signature::new(Parameters::top(), subclass_of_ty.to_instance(db, env)),
                )))
            }

            // TODO: This is unsound so in future we can consider an opt-in option to disable it.
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(class) => Some(class.into_callable(db)),
                SubclassOfInner::Protocol(protocol) => protocol.class_origin(db).map(|origin| {
                    if protocol.materialization_kind(db).is_some() {
                        // The origin supplies the constructor, but the actual receiver retains
                        // `Top[P]` or `Bottom[P]`. Infer with both so instance-returning overloads
                        // are materialized without replacing explicit non-instance returns.
                        (*origin).into_callable_with_receiver(db, self)
                    } else {
                        (*origin).into_callable(db)
                    }
                }),
                SubclassOfInner::TypeVar(tvar) => {
                    match tvar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            let upcast_callables = bound
                                .constructor_for_typevar_bound(db, env)
                                .try_upcast_to_callable_with_policy_and_context(
                                    db, env, policy, context,
                                )?;
                            Some(upcast_callables.map(|callable| {
                                let signatures = callable
                                    .signatures(db)
                                    .into_iter()
                                    .map(|sig| sig.clone().with_return_type(Type::TypeVar(tvar)));
                                callable.with_signatures(
                                    db,
                                    CallableSignature::from_overloads(signatures),
                                )
                            }))
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            let mut callables = SmallVec::new();
                            for constraint in constraints.elements(db) {
                                let element_upcast = constraint
                                    .to_meta_type(db, env)
                                    .try_upcast_to_callable_with_policy_and_context(
                                        db, env, policy, context,
                                    )?;
                                for callable in element_upcast.into_inner() {
                                    let signatures =
                                        callable.signatures(db).into_iter().map(|sig| {
                                            sig.clone().with_return_type(Type::TypeVar(tvar))
                                        });
                                    callables.push(callable.with_signatures(
                                        db,
                                        CallableSignature::from_overloads(signatures),
                                    ));
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
                    let element_callable = element
                        .try_upcast_to_callable_with_policy_and_context(db, env, policy, context)?;
                    callables.extend(element_callable.into_inner());
                }
                Some(CallableTypes::new(callables))
            }

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(enum_literal) => enum_literal
                    .enum_class_instance(db, env)
                    .try_upcast_to_callable_with_policy_and_context(db, env, policy, context),
                _ => None,
            },

            Type::TypeAlias(alias) => alias
                .value_type(db)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context),

            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(callable)) => callable
                .inner(db)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context)
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
                    alternatives.try_upcast_to_callable_with_policy(db, env, policy)
                }),

            Type::EnumComplement(complement) => complement
                .remaining_literal_union(db, env)
                .try_upcast_to_callable_with_policy_and_context(db, env, policy, context),

            // TODO
            Type::DataclassDecorator(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CallableUpcastContext<'db> {
    recursive_definition: Option<Definition<'db>>,
}

impl<'db> CallableUpcastContext<'db> {
    fn is_recursive_reference(self, db: &'db dyn Db, function: FunctionType<'db>) -> bool {
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
    ///   to that instance. The result is a [`super::BoundMethodType`], retaining the unbound
    ///   function and the captured receiver separately. It has `types.MethodType` identity
    ///   and does not bind again on subsequent attribute access.
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
    /// Calling [`CallableType::bind_self`] removes this marker without removing a parameter.
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

    /// A synthesized `staticmethod` descriptor wrapping function signatures.
    ///
    /// Class and instance access both return a [`Self::FunctionLike`] callable with the
    /// original signatures. The descriptor and the resulting function have distinct nominal
    /// identities and binding behavior: accessing the descriptor never supplies a receiver,
    /// but the function it returns can subsequently bind through its own `__get__`.
    /// The raw descriptor exposes the function through `__func__` and `__wrapped__`.
    StaticMethodLike,

    /// A synthesized `classmethod` descriptor wrapping function signatures.
    ///
    /// Class and instance access both produce a [`super::BoundMethodType`] whose captured
    /// receiver is the owner class. The method's `__func__` is an ordinary function-like
    /// callable with the unbound signatures. `Self` in the bound signatures refers to an
    /// instance of the captured class, independently of the class-object receiver.
    ///
    /// We retain the wrapped call signatures on this descriptor for decorator inference,
    /// although an unapplied `classmethod` descriptor itself is not callable at runtime.
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
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
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

    /// Create a callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> CallableType<'db> {
        Self::single(db, Signature::unknown())
    }

    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::FunctionLike)
    }

    /// Nominal runtime identity is independent of the callable's accepted arguments.
    pub(super) fn runtime_class(self, db: &'db dyn Db) -> Option<KnownClass> {
        match self.kind(db) {
            CallableTypeKind::FunctionLike => Some(KnownClass::FunctionType),
            CallableTypeKind::StaticMethodLike => Some(KnownClass::Staticmethod),
            CallableTypeKind::ClassMethodLike => Some(KnownClass::Classmethod),
            _ => None,
        }
    }

    fn is_dunder_paramspec(self, db: &'db dyn Db) -> bool {
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
        Self::paramspec_value_from_signatures(db, self.signatures(db).clone())
    }

    /// Returns the reduced callable produced by partially applying selected overloads.
    pub(crate) fn partially_apply(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        overloads: impl IntoIterator<Item = PartialSignatureApplication<'db>>,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            CallableSignature::partially_apply(db, env, overloads)?,
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

    pub(crate) fn bind_self(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Option<Type<'db>>,
    ) -> CallableType<'db> {
        if self.is_dunder_paramspec(db) {
            return self.into_regular(db);
        }

        self.with_signatures(db, self.signatures(db).bind_self(db, env, self_type))
    }

    pub(crate) fn into_function_like(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::FunctionLike)
    }

    pub(crate) fn into_dunder_paramspec(self, db: &'db dyn Db) -> CallableType<'db> {
        self.with_kind(db, CallableTypeKind::DunderParamSpec)
    }

    pub(crate) fn apply_self(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Type<'db>,
    ) -> CallableType<'db> {
        self.apply_self_with_receiver(db, env, self_type, self_type)
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
        if let TypeMapping::RescopeReturnCallables(replacements) = type_mapping {
            return replacements.get(&self).copied().unwrap_or(self);
        }

        self.with_signatures(
            db,
            self.signatures(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.signatures(db)
            .find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
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
    fn new(callables: SmallVec<[CallableType<'db>; 1]>) -> Self {
        assert!(!callables.is_empty(), "CallableTypes should not be empty");
        CallableTypes(callables)
    }

    pub(crate) fn one(callable: CallableType<'db>) -> Self {
        CallableTypes(smallvec_inline![callable])
    }

    pub(crate) fn from_elements(callables: impl IntoIterator<Item = CallableType<'db>>) -> Self {
        let callables: SmallVec<_> = callables.into_iter().collect();
        assert!(!callables.is_empty(), "CallableTypes should not be empty");
        CallableTypes(callables)
    }

    pub(crate) fn exactly_one(self) -> Option<CallableType<'db>> {
        match self.0.as_slice() {
            [single] => Some(*single),
            _ => None,
        }
    }

    pub(super) fn as_slice(&self) -> &[CallableType<'db>] {
        &self.0
    }

    fn into_inner(self) -> SmallVec<[CallableType<'db>; 1]> {
        self.0
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, CallableType<'db>> {
        self.0.iter()
    }

    pub(crate) fn into_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        assert!(!self.0.is_empty(), "CallableTypes should not be empty");
        UnionType::from_elements(db, env, self.0.into_iter().map(Type::Callable))
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

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Check whether one callable type has the given relation to another callable type.
    ///
    /// See [`Type::is_subtype_of`] and [`Type::is_assignable_to`] for more details.
    pub(super) fn check_callable_pair(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if target.runtime_class(db).is_some()
            && target.runtime_class(db) != source.runtime_class(db)
        {
            return self.never();
        }

        self.check_callable_signature_pair(db, source.signatures(db), target.signatures(db))
    }

    pub(super) fn check_callables_vs_callable(
        &self,
        db: &'db dyn Db,
        source: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        source.iter().when_all(db, self.constraints, |element| {
            self.check_callable_pair(db, *element, target)
        })
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
