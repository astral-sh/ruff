//! Applies decorators while preserving callable descriptor kinds and overload correlations.
//!
//! Ordinary inference uses the synchronous bodies. The asynchronous bodies expose the same
//! decisions through typed dependencies. An execution interruption leaves application unfinished;
//! semantic call errors retain their fallback types and deferred diagnostics.

use std::convert::Infallible;
use std::slice;

use itertools::Itertools;
use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::Definition;

use crate::types::call::{Bindings, CallArguments, CallError};
use crate::types::callable::{CallableTypeKind, CallableTypes};
use crate::types::class::ClassLiteral;
use crate::types::diagnostic::{
    DYNAMIC_FUNCTION_DECORATOR_RETURN, report_dynamic_function_decorator_return,
};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::generics::Specialization;
use crate::types::infer::{InferenceFlags, TypeInferenceBuilder};
use crate::types::known_instance::DeprecatedInstance;
use crate::types::set_theoretic::{RecursivelyDefined, UnionBuilder};
use crate::types::{
    BoundTypeVarInstance, CallableBinding, CallableType, KnownClass, KnownInstanceType,
    PropertyInstanceType, RecursiveType, Signature, Type, TypeAliasType, UnionType,
};
use crate::{Db, ProgramEnvironment};

use super::source_effects::FunctionDecoratorRequest;

pub(in crate::types::infer) struct DecoratorApplicationFacts;

/// A decorator-application dependency whose controlled implementation is unavailable.
#[cfg(feature = "experimental-analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecoratorApplicationOperation {
    FunctionKindUpdate,
    FunctionDeprecation,
    OverloadDeprecation,
    OverloadFinalization,
    RecursiveUnfold,
    UnboundRecursiveVariable,
    SignatureBinding,
    AwaitableSpecialization,
    DynamicReturnComparison,
    DynamicReturnDiagnostic,
}

pub(in crate::types::infer::builder) struct OrdinaryDecoratorApplicationEffects<'a, 'db> {
    pub(in crate::types::infer::builder) db: &'db dyn Db,
    pub(in crate::types::infer::builder) env: &'a ProgramEnvironment<'db>,
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum DecoratorTypeTransform {
    Wrap(CallableTypeKind),
    Propagate(CallableTypeKind),
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum TransparentCallableReturn<'db> {
    TypeVar(BoundTypeVarInstance<'db>),
    Awaitable(BoundTypeVarInstance<'db>),
}

shared_semantic_family! {
    #[synchronous(SynchronousDecoratorApplicationEffects)]
    pub(in crate::types::infer) trait DecoratorApplicationEffects<'db, 'ast> {
        type Error;
        type Union;
        #[operation(source)]
        async fn known_class(&self, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union_like(&self, ty: Type<'db>) -> Result<Option<UnionType<'db>>, Self::Error>;
        #[operation(source)]
        async fn function_kind(&self, function: FunctionType<'db>) -> Result<CallableTypeKind, Self::Error>;
        #[operation(source)]
        async fn callable_kind(&self, callable: CallableType<'db>) -> Result<CallableTypeKind, Self::Error>;
        #[operation(child)]
        async fn function_with_kind(&self, function: FunctionType<'db>, kind: CallableTypeKind) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn callable_with_kind(&self, callable: CallableType<'db>, kind: CallableTypeKind) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn function_with_deprecated(&self, function: FunctionType<'db>, deprecated: DeprecatedInstance<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn overload_with_deprecated(&self, overload: OverloadLiteral<'db>, deprecated: DeprecatedInstance<'db>) -> Result<OverloadLiteral<'db>, Self::Error>;
        #[operation(child)]
        async fn callable_with_deprecated(&self, callable: CallableType<'db>, overload: OverloadLiteral<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn upcast_callable(&self, ty: Type<'db>) -> Result<Option<CallableTypes<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_callable(&self, cursor: &mut slice::Iter<'_, CallableType<'db>>) -> Result<Option<CallableType<'db>>, Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_union(&self, cursor: &mut slice::Iter<'_, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_union(&self) -> Result<Self::Union, Self::Error>;
        #[operation(child)]
        async fn union_add(&self, union: &mut Self::Union, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_recursively_defined(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Self::Error>;
        #[operation(child)]
        async fn finish_union(&self, union: Self::Union, recursively_defined: RecursivelyDefined) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn map_union(&self, union: UnionType<'db>, transform: DecoratorTypeTransform) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn transform_type(&self, ty: Type<'db>, transform: DecoratorTypeTransform) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn unbound_recursive(&self) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn propagatable_kind(&self, ty: Type<'db>) -> Result<Option<CallableTypeKind>, Self::Error>;
        #[operation(child)]
        async fn try_call(&self, decorator: Type<'db>, decorated: Type<'db>) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error>;
        #[operation(child)]
        async fn return_type(&self, bindings: &Bindings<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn no_type_check(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn record_failed_call(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, decorator: &ast::Decorator, decorated: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn defer_failed_call(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, decorator: &ast::Decorator, decorated: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn transparent_result(&self, bindings: &Bindings<'db>, decorated: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn single_binding<'a>(&self, bindings: &'a Bindings<'db>) -> Result<Option<&'a CallableBinding<'db>>, Self::Error>;
        #[operation(local)]
        async fn single_matching_signature<'a>(&self, binding: &'a CallableBinding<'db>) -> Result<Option<&'a Signature<'db>>, Self::Error>;
        #[operation(child)]
        async fn bind_self(&self, signature: &Signature<'db>, bound_type: Type<'db>) -> Result<Signature<'db>, Self::Error>;
        #[operation(child)]
        async fn callable_paramspec_and_return(&self, ty: Type<'db>) -> Result<Option<(BoundTypeVarInstance<'db>, TransparentCallableReturn<'db>)>, Self::Error>;
        #[operation(source)]
        async fn single_signature(&self, callable: CallableType<'db>) -> Result<Option<&'db Signature<'db>>, Self::Error>;
        #[operation(child)]
        async fn known_awaitable(&self, ty: Type<'db>) -> Result<Option<Specialization<'db>>, Self::Error>;
        #[operation(source)]
        async fn single_type_argument(&self, specialization: Specialization<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn same_typevar(&self, left: BoundTypeVarInstance<'db>, right: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn function_callable(&self, function: FunctionType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn dynamic_return_enabled(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn equivalent_to_any(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_dynamic_return(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, decorator: &ast::Decorator, decorated: Type<'db>, bindings: &Bindings<'db>, function: &ast::StmtFunctionDef, inferred: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn apply(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, decorator: Type<'db>, decorated: Type<'db>, node: &ast::Decorator, function: Option<&ast::StmtFunctionDef>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn property_accessor_definition(&self, property: PropertyInstanceType<'db>, decorator: Type<'db>, decorated: Type<'db>, definition: Definition<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl DecoratorApplicationFacts {
        fn same_kind(&self, left: CallableTypeKind, right: CallableTypeKind) -> bool { left == right }
        fn wrapper_accepts_function(&self, current: CallableTypeKind, kind: CallableTypeKind) -> bool { current == CallableTypeKind::FunctionLike || current == kind }
        fn wrapper_accepts_callable(&self, current: CallableTypeKind, kind: CallableTypeKind) -> bool { matches!(current, CallableTypeKind::Regular | CallableTypeKind::FunctionLike) || current == kind }
        fn propagates(&self, kind: CallableTypeKind) -> bool { matches!(kind, CallableTypeKind::FunctionLike | CallableTypeKind::StaticMethodLike | CallableTypeKind::ClassMethodLike) }
        fn callable_cursor<'a, 'db>(&self, callables: &'a CallableTypes<'db>) -> slice::Iter<'a, CallableType<'db>> { callables.iter() }
        fn union_cursor<'a, 'db>(&self, elements: &'a [Type<'db>]) -> slice::Iter<'a, Type<'db>> { elements.iter() }
        fn prefix<'a, 'db>(&self, elements: &'a [Type<'db>], remaining: &slice::Iter<'_, Type<'db>>) -> slice::Iter<'a, Type<'db>> {
            // The cursor has just consumed the first changed member of this same slice.
            elements[..elements.len() - remaining.len() - 1].iter()
        }
        fn changed<'db>(&self, original: Type<'db>, mapped: Type<'db>) -> bool { original != mapped || matches!(mapped, Type::TypeAlias(_)) }
        fn fallback<'db>(&self, mapped: Option<Type<'db>>, original: Type<'db>) -> Type<'db> { mapped.unwrap_or(original) }
        fn diagnostic_function<'a>(&self, implementation: bool, function: &'a ast::StmtFunctionDef) -> Option<&'a ast::StmtFunctionDef> { (!implementation).then_some(function) }
        fn bound_type<'db>(&self, binding: &CallableBinding<'db>) -> Option<Type<'db>> { binding.bound_type }
        fn signature<'a, 'db>(&self, bound: &'a Option<Signature<'db>>, original: &'a Signature<'db>) -> &'a Signature<'db> { bound.as_ref().unwrap_or(original) }
        fn single_parameter<'db>(&self, signature: &Signature<'db>) -> Option<Type<'db>> { match signature.parameters().as_slice() { [parameter] => Some(parameter.annotated_type()), _ => None } }
        fn paramspec<'db>(&self, signature: &Signature<'db>) -> Option<BoundTypeVarInstance<'db>> { signature.parameters().as_paramspec() }
        fn return_type<'db>(&self, signature: &Signature<'db>) -> Type<'db> { signature.return_ty }
        fn matching_return_variables<'db>(&self, left: TransparentCallableReturn<'db>, right: TransparentCallableReturn<'db>) -> Option<(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)> {
            match (left, right) {
                (TransparentCallableReturn::TypeVar(left), TransparentCallableReturn::TypeVar(right)) | (TransparentCallableReturn::Awaitable(left), TransparentCallableReturn::Awaitable(right)) => Some((left, right)),
                _ => None,
            }
        }
    }

    #[synchronous(apply_function_decorator_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values(CallableTypeKind::StaticMethodLike, CallableTypeKind::ClassMethodLike, DecoratorTypeTransform::Wrap)]
    pub(in crate::types::infer) async fn apply_function_decorator_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, request: FunctionDecoratorRequest<'_, 'db>, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let FunctionDecoratorRequest { function, definition, overload_literal, decorator_ty, decorator_node, inferred_ty, is_decorated_overload_implementation } = request;
        let descriptor_kind = match decorator_ty {
            Type::ClassLiteral(class) => match effects.known_class(class).await? {
                Some(KnownClass::Staticmethod) => Some(CallableTypeKind::StaticMethodLike),
                Some(KnownClass::Classmethod) => Some(CallableTypeKind::ClassMethodLike),
                _ => None,
            },
            _ => None,
        };
        if let Some(kind) = descriptor_kind {
            let transform = DecoratorTypeTransform::Wrap(kind);
            let wrapped = if let Some(union) = effects.union_like(inferred_ty).await? {
                effects.map_union(union, transform).await?
            } else {
                effects.transform_type(inferred_ty, transform).await?
            };
            if let Some(wrapped) = wrapped { return Ok(wrapped); }
        }
        if let Type::KnownInstance(KnownInstanceType::Deprecated(deprecated)) = decorator_ty {
            match inferred_ty {
                Type::FunctionLiteral(function) => return effects.function_with_deprecated(function, deprecated).await,
                Type::Callable(callable) => {
                    let overload = effects.overload_with_deprecated(overload_literal, deprecated).await?;
                    return effects.callable_with_deprecated(callable, overload).await;
                }
                _ => {}
            }
        }
        let decorated_function = facts.diagnostic_function(is_decorated_overload_implementation, function);
        let result = effects.apply(builder, decorator_ty, inferred_ty, decorator_node, decorated_function).await?;
        if let Type::PropertyInstance(property) = result {
            return effects.property_accessor_definition(property, decorator_ty, inferred_ty, definition).await;
        }
        Ok(result)
    }

    #[synchronous(wrap_decorator_type_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn wrap_decorator_type_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        ty: Type<'db>, kind: CallableTypeKind, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        match effects.resolve_alias(ty).await? {
            Type::FunctionLiteral(function) => {
                if facts.wrapper_accepts_function(effects.function_kind(function).await?, kind) {
                    return Ok(Some(effects.function_with_kind(function, kind).await?));
                }
            }
            Type::Callable(callable) => {
                if facts.wrapper_accepts_callable(effects.callable_kind(callable).await?, kind) {
                    return Ok(Some(effects.callable_with_kind(callable, kind).await?));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    #[synchronous(map_decorator_union_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values(Type::Union)]
    pub(in crate::types::infer) async fn map_decorator_union_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        union: UnionType<'db>, transform: DecoratorTypeTransform, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let elements = effects.union_elements(union).await?;
        let mut cursor = facts.union_cursor(elements);
        #[cursor_loop]
        while let Some(ty) = effects.next_union(&mut cursor).await? {
            let Some(mapped) = effects.transform_type(ty, transform).await? else { return Ok(None); };
            // The builder unpacks `TypeAlias` nodes but preserves structural recursive types.
            // Rebuild for an alias result even when it is unchanged.
            if facts.changed(ty, mapped) {
                let mut builder = effects.new_union().await?;
                let mut prefix = facts.prefix(elements, &cursor);
                #[cursor_loop]
                while let Some(previous) = effects.next_union(&mut prefix).await? {
                    effects.union_add(&mut builder, previous).await?;
                }
                effects.union_add(&mut builder, mapped).await?;
                #[cursor_loop]
                while let Some(element) = effects.next_union(&mut cursor).await? {
                    let Some(mapped) = effects.transform_type(element, transform).await? else { return Ok(None); };
                    effects.union_add(&mut builder, mapped).await?;
                }
                let recursively_defined = effects.union_recursively_defined(union).await?;
                return Ok(Some(effects.finish_union(builder, recursively_defined).await?));
            }
        }
        Ok(Some(Type::Union(union)))
    }

    #[synchronous(decorator_callable_kind_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn decorator_callable_kind_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        decorated_ty: Type<'db>, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Option<CallableTypeKind>, E::Error> {
        // For FunctionLiteral, get the kind directly without computing the full signature.
        // This avoids a query cycle when the function has default parameter values, since
        // computing the signature requires evaluating those defaults which may trigger
        // deferred inference.
        if let Type::FunctionLiteral(function) = decorated_ty {
            return Ok(Some(effects.function_kind(function).await?));
        }
        let Some(callables) = effects.upcast_callable(decorated_ty).await? else { return Ok(None); };
        let mut cursor = facts.callable_cursor(&callables);
        let Some(first) = effects.next_callable(&mut cursor).await? else { return Ok(None); };
        let kind = effects.callable_kind(first).await?;
        #[cursor_loop]
        while let Some(callable) = effects.next_callable(&mut cursor).await? {
            if !facts.same_kind(kind, effects.callable_kind(callable).await?) { return Ok(None); }
        }
        if facts.propagates(kind) { Ok(Some(kind)) } else { Ok(None) }
    }

    #[synchronous(propagate_decorator_kind_sync)]
    #[capabilities(effects = DecoratorApplicationEffects)]
    #[passive_values(DecoratorTypeTransform::Propagate)]
    pub(in crate::types::infer) async fn propagate_decorator_kind_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        ty: Type<'db>, kind: CallableTypeKind, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        match ty {
            Type::Recursive(recursive) => {
                let Some(unfolded) = effects.unfold(recursive).await? else { return Ok(None); };
                effects.transform_type(unfolded, DecoratorTypeTransform::Propagate(kind)).await
            }
            Type::RecursiveVar(_) => effects.unbound_recursive().await,
            Type::Callable(callable) => Ok(Some(effects.callable_with_kind(callable, kind).await?)),
            Type::Union(union) => effects.map_union(union, DecoratorTypeTransform::Propagate(kind)).await,
            Type::TypeAlias(alias) => {
                let value = effects.alias_value(alias).await?;
                effects.transform_type(value, DecoratorTypeTransform::Propagate(kind)).await
            }
            // Intersections are currently not handled here because that would require
            // the decorator to be explicitly annotated as returning an intersection.
            Type::Intersection(_) | Type::EnumComplement(_) => Ok(None),
            // All other types cannot have a callable kind propagated to them.
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never | Type::FunctionLiteral(_)
            | Type::BoundMethod(_) | Type::KnownBoundMethod(_) | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_) | Type::DataclassTransformer(_) | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
            | Type::NominalInstance(_) | Type::ProtocolInstance(_) | Type::SpecialForm(_)
            | Type::KnownInstance(_) | Type::PropertyInstance(_) | Type::SlotDescriptor(_)
            | Type::AlwaysTruthy | Type::AlwaysFalsy | Type::LiteralValue(_) | Type::TypeVar(_)
            | Type::BoundSuper(_) | Type::TypeIs(_) | Type::TypeGuard(_) | Type::TypeForm(_)
            | Type::TypedDict(_) | Type::NewTypeInstance(_) => Ok(None),
        }
    }

    /// Apply a decorator to a function or class type and return the resulting type.
    ///
    /// Constructor semantics for class-like decorators are handled by `Type::bindings`, so we
    /// can always use `try_call` here.
    #[synchronous(apply_decorator_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values(DecoratorTypeTransform::Propagate)]
    pub(in crate::types::infer) async fn apply_decorator_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, decorator_ty: Type<'db>, decorated_ty: Type<'db>, decorator_node: &ast::Decorator,
        decorated_function: Option<&ast::StmtFunctionDef>, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let propagatable_kind = effects.propagatable_kind(decorated_ty).await?;
        let (return_ty, decorator_bindings) = match effects.try_call(decorator_ty, decorated_ty).await? {
            Ok(bindings) => (effects.return_type(&bindings).await?, Some(bindings)),
            Err(CallError(_, bindings)) => {
                effects.defer_failed_call(builder, decorator_node, decorated_ty).await?;
                (effects.return_type(&bindings).await?, None)
            }
        };

        // TODO: Remove this special case once the new constraint solver can preserve
        // per-overload ParamSpec/return correlations for transparent callable decorators.
        if let Some(ref bindings) = decorator_bindings
            && let Some(result) = effects.transparent_result(bindings, decorated_ty).await?
        {
            return Ok(result);
        }

        // When a method on a class is decorated with a function that returns a
        // `Callable`, assume that the returned callable is also function-like (or
        // classmethod-like or staticmethod-like). See "Decorating a method with
        // a `Callable`-typed decorator" in `callables_as_descriptors.md` for the
        // extended explanation.
        let inferred_ty = if let Some(kind) = propagatable_kind {
            let propagated = effects.transform_type(return_ty, DecoratorTypeTransform::Propagate(kind)).await?;
            facts.fallback(propagated, return_ty)
        } else {
            return_ty
        };

        if let Some(function) = decorated_function
            && let Some(ref bindings) = decorator_bindings
            && effects.dynamic_return_enabled(builder).await?
            && effects.equivalent_to_any(inferred_ty).await?
            && !effects.equivalent_to_any(decorated_ty).await?
        {
            effects.report_dynamic_return(builder, decorator_node, decorated_ty, bindings, function, inferred_ty).await?;
        }
        Ok(inferred_ty)
    }

    #[synchronous(defer_decorator_call_sync)]
    #[capabilities(effects = DecoratorApplicationEffects)]
    #[passive_values()]
    pub(in crate::types::infer) async fn defer_decorator_call_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, decorator: &ast::Decorator, input_ty: Type<'db>, effects: &E,
    ) -> Result<(), E::Error> {
        // We replay failed decorator applications after inference only to report call errors,
        // such as incompatible argument types or missing arguments. `@no_type_check` suppresses
        // these errors. Skip recording the calls here because the enclosing scope's post-inference
        // diagnostic context does not inherit this definition-local flag.
        if !effects.no_type_check(builder).await? {
            effects.record_failed_call(builder, decorator, input_ty).await?;
        }
        Ok(())
    }

    /// Preserves the decorated callable's overload signatures when the decorator's parameter
    /// and return callable use the same `ParamSpec` and both return `T`, or both return
    /// `Awaitable[T]`, for the same type variable `T`.
    ///
    /// Returns `None` when a single matching signature cannot establish this pattern, or when
    /// the decorated value is neither a function literal nor a callable.
    #[synchronous(transparent_callable_decorator_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn transparent_callable_decorator_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        bindings: &Bindings<'db>, decorated_ty: Type<'db>, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        if !matches!(decorated_ty, Type::FunctionLiteral(_) | Type::Callable(_)) { return Ok(None); }
        let Some(binding) = effects.single_binding(bindings).await? else { return Ok(None); };
        let Some(signature) = effects.single_matching_signature(binding).await? else { return Ok(None); };
        let bound_signature = if let Some(bound_type) = facts.bound_type(binding) {
            Some(effects.bind_self(signature, bound_type).await?)
        } else {
            None
        };
        let signature = facts.signature(&bound_signature, signature);
        let Some(parameter) = facts.single_parameter(signature) else { return Ok(None); };
        let Some((parameter_paramspec, parameter_return)) = effects.callable_paramspec_and_return(parameter).await? else { return Ok(None); };
        let Some((return_paramspec, return_return)) = effects.callable_paramspec_and_return(facts.return_type(signature)).await? else { return Ok(None); };
        if !effects.same_typevar(parameter_paramspec, return_paramspec).await? { return Ok(None); }
        let Some((left, right)) = facts.matching_return_variables(parameter_return, return_return) else { return Ok(None); };
        if !effects.same_typevar(left, right).await? { return Ok(None); }
        match decorated_ty {
            Type::FunctionLiteral(function) => Ok(Some(effects.function_callable(function).await?)),
            Type::Callable(_) => Ok(Some(decorated_ty)),
            _ => Ok(None),
        }
    }

    #[synchronous(callable_paramspec_and_return_sync)]
    #[capabilities(effects = DecoratorApplicationEffects, facts = DecoratorApplicationFacts)]
    #[passive_values(TransparentCallableReturn::TypeVar, TransparentCallableReturn::Awaitable)]
    pub(in crate::types::infer) async fn callable_paramspec_and_return_with<'db, 'ast, E: DecoratorApplicationEffects<'db, 'ast>>(
        ty: Type<'db>, facts: DecoratorApplicationFacts, effects: &E,
    ) -> Result<Option<(BoundTypeVarInstance<'db>, TransparentCallableReturn<'db>)>, E::Error> {
        let Type::Callable(callable) = effects.resolve_alias(ty).await? else { return Ok(None); };
        if !matches!(effects.callable_kind(callable).await?, CallableTypeKind::Regular) { return Ok(None); }
        let Some(signature) = effects.single_signature(callable).await? else { return Ok(None); };
        let Some(paramspec) = facts.paramspec(signature) else { return Ok(None); };
        let return_ty = facts.return_type(signature);
        let return_typevar = if let Type::TypeVar(typevar) = return_ty {
            TransparentCallableReturn::TypeVar(typevar)
        } else {
            let Some(specialization) = effects.known_awaitable(return_ty).await? else { return Ok(None); };
            let Some(Type::TypeVar(typevar)) = effects.single_type_argument(specialization).await? else { return Ok(None); };
            TransparentCallableReturn::Awaitable(typevar)
        };
        Ok(Some((paramspec, return_typevar)))
    }
}

impl<'db, 'ast> SynchronousDecoratorApplicationEffects<'db, 'ast>
    for OrdinaryDecoratorApplicationEffects<'_, 'db>
{
    type Error = Infallible;
    type Union = UnionBuilder<'db>;
    fn known_class(&self, class: ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.db))
    }
    fn union_like(&self, ty: Type<'db>) -> Result<Option<UnionType<'db>>, Self::Error> {
        Ok(ty.as_union_like(self.db))
    }
    fn function_kind(&self, function: FunctionType<'db>) -> Result<CallableTypeKind, Self::Error> {
        Ok(function.callable_type_kind(self.db))
    }
    fn callable_kind(&self, callable: CallableType<'db>) -> Result<CallableTypeKind, Self::Error> {
        Ok(callable.kind(self.db))
    }
    fn function_with_kind(
        &self,
        function: FunctionType<'db>,
        kind: CallableTypeKind,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::FunctionLiteral(
            function.with_descriptor_kind(self.db, kind),
        ))
    }
    fn callable_with_kind(
        &self,
        callable: CallableType<'db>,
        kind: CallableTypeKind,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Callable(callable.with_kind(self.db, kind)))
    }
    fn function_with_deprecated(
        &self,
        function: FunctionType<'db>,
        deprecated: DeprecatedInstance<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::FunctionLiteral(
            function.with_deprecated(self.db, deprecated),
        ))
    }
    fn overload_with_deprecated(
        &self,
        overload: OverloadLiteral<'db>,
        deprecated: DeprecatedInstance<'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error> {
        Ok(overload.with_deprecated(self.db, deprecated))
    }
    fn callable_with_deprecated(
        &self,
        callable: CallableType<'db>,
        overload: OverloadLiteral<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Callable(callable.with_deprecated(self.db, overload)))
    }
    fn upcast_callable(&self, ty: Type<'db>) -> Result<Option<CallableTypes<'db>>, Self::Error> {
        Ok(ty.try_upcast_to_callable(self.db, self.env))
    }
    fn next_callable(
        &self,
        cursor: &mut slice::Iter<'_, CallableType<'db>>,
    ) -> Result<Option<CallableType<'db>>, Self::Error> {
        Ok(cursor.next().copied())
    }
    fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(union.elements(self.db))
    }
    fn next_union(
        &self,
        cursor: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(cursor.next().copied())
    }
    fn new_union(&self) -> Result<Self::Union, Self::Error> {
        Ok(UnionBuilder::new(self.db, self.env))
    }
    fn union_add(&self, union: &mut Self::Union, ty: Type<'db>) -> Result<(), Self::Error> {
        union.add_in_place(ty);
        Ok(())
    }
    fn union_recursively_defined(
        &self,
        union: UnionType<'db>,
    ) -> Result<RecursivelyDefined, Self::Error> {
        Ok(union.recursively_defined(self.db))
    }
    fn finish_union(
        &self,
        union: Self::Union,
        recursively_defined: RecursivelyDefined,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(union.or_recursively_defined(recursively_defined).build())
    }
    fn map_union(
        &self,
        union: UnionType<'db>,
        transform: DecoratorTypeTransform,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        map_decorator_union_sync(union, transform, DecoratorApplicationFacts, self)
    }
    fn transform_type(
        &self,
        ty: Type<'db>,
        transform: DecoratorTypeTransform,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        match transform {
            DecoratorTypeTransform::Wrap(kind) => {
                wrap_decorator_type_sync(ty, kind, DecoratorApplicationFacts, self)
            }
            DecoratorTypeTransform::Propagate(kind) => {
                propagate_decorator_kind_sync(ty, kind, self)
            }
        }
    }
    fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(recursive.unfold(self.db, self.env).into_unfolded())
    }
    fn unbound_recursive(&self) -> Result<Option<Type<'db>>, Self::Error> {
        unreachable!("semantic operation on an unbound recursive variable")
    }
    fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(alias.value_type(self.db))
    }
    fn propagatable_kind(&self, ty: Type<'db>) -> Result<Option<CallableTypeKind>, Self::Error> {
        decorator_callable_kind_sync(ty, DecoratorApplicationFacts, self)
    }
    fn try_call(
        &self,
        decorator: Type<'db>,
        decorated: Type<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error> {
        let arguments = CallArguments::positional([decorated]);
        Ok(decorator.try_call(self.db, self.env, &arguments))
    }
    fn return_type(&self, bindings: &Bindings<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(bindings.return_type(self.db, self.env))
    }
    fn no_type_check(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        Ok(builder
            .inference_flags()
            .contains(InferenceFlags::IN_NO_TYPE_CHECK))
    }
    fn record_failed_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: &ast::Decorator,
        decorated: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .deferred_decorator_calls
            .push(((&decorator.expression).into(), decorated));
        Ok(())
    }
    fn defer_failed_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: &ast::Decorator,
        decorated: Type<'db>,
    ) -> Result<(), Self::Error> {
        defer_decorator_call_sync(builder, decorator, decorated, self)
    }
    fn transparent_result(
        &self,
        bindings: &Bindings<'db>,
        decorated: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        transparent_callable_decorator_sync(bindings, decorated, DecoratorApplicationFacts, self)
    }
    fn single_binding<'a>(
        &self,
        bindings: &'a Bindings<'db>,
    ) -> Result<Option<&'a CallableBinding<'db>>, Self::Error> {
        Ok(bindings.single_element())
    }
    fn single_matching_signature<'a>(
        &self,
        binding: &'a CallableBinding<'db>,
    ) -> Result<Option<&'a Signature<'db>>, Self::Error> {
        Ok(binding
            .matching_overloads()
            .exactly_one()
            .ok()
            .map(|(_, overload)| &overload.signature))
    }
    fn bind_self(
        &self,
        signature: &Signature<'db>,
        bound_type: Type<'db>,
    ) -> Result<Signature<'db>, Self::Error> {
        Ok(signature.bind_self(self.db, self.env, Some(bound_type)))
    }
    fn callable_paramspec_and_return(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<(BoundTypeVarInstance<'db>, TransparentCallableReturn<'db>)>, Self::Error>
    {
        callable_paramspec_and_return_sync(ty, DecoratorApplicationFacts, self)
    }
    fn single_signature(
        &self,
        callable: CallableType<'db>,
    ) -> Result<Option<&'db Signature<'db>>, Self::Error> {
        Ok(match callable.signatures(self.db).overloads.as_slice() {
            [signature] => Some(signature),
            _ => None,
        })
    }
    fn known_awaitable(&self, ty: Type<'db>) -> Result<Option<Specialization<'db>>, Self::Error> {
        Ok(ty.known_specialization(self.db, self.env, KnownClass::Awaitable))
    }
    fn single_type_argument(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match specialization.types(self.db) {
            [inner] => Some(*inner),
            _ => None,
        })
    }
    fn same_typevar(
        &self,
        left: BoundTypeVarInstance<'db>,
        right: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(left.is_same_typevar_as(self.db, right))
    }
    fn function_callable(&self, function: FunctionType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Callable(function.into_callable_type(self.db)))
    }
    fn dynamic_return_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        Ok(builder
            .context
            .is_lint_enabled(&DYNAMIC_FUNCTION_DECORATOR_RETURN))
    }
    fn equivalent_to_any(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.is_equivalent_to(self.db, self.env, Type::any()))
    }
    fn report_dynamic_return(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: &ast::Decorator,
        decorated: Type<'db>,
        bindings: &Bindings<'db>,
        function: &ast::StmtFunctionDef,
        inferred: Type<'db>,
    ) -> Result<(), Self::Error> {
        report_dynamic_function_decorator_return(
            &builder.context,
            decorator,
            decorated,
            bindings,
            function,
            inferred,
        );
        Ok(())
    }
    fn apply(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: Type<'db>,
        decorated: Type<'db>,
        node: &ast::Decorator,
        function: Option<&ast::StmtFunctionDef>,
    ) -> Result<Type<'db>, Self::Error> {
        apply_decorator_sync(
            builder,
            decorator,
            decorated,
            node,
            function,
            DecoratorApplicationFacts,
            self,
        )
    }
    fn property_accessor_definition(
        &self,
        property: PropertyInstanceType<'db>,
        decorator: Type<'db>,
        decorated: Type<'db>,
        definition: Definition<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::PropertyInstance(property.with_accessor_definition(
            self.db, decorator, decorated, definition,
        )))
    }
}
