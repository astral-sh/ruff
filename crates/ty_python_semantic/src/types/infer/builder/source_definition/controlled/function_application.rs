//! Controlled decorator application uses the shared decisions and synthetic invocation.

use std::slice;

use itertools::Itertools;
use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::call::{Bindings, CallArguments, CallError};
use crate::types::callable::{CallableTypeKind, CallableTypes};
use crate::types::class::ClassLiteral;
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::diagnostic::DYNAMIC_FUNCTION_DECORATOR_RETURN;
use crate::types::function::descriptor::FunctionTypeDescriptorEffects;
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::generics::Specialization;
use crate::types::infer::builder::function::application::{
    DecoratorApplicationEffects, DecoratorApplicationFacts, DecoratorApplicationOperation,
    DecoratorTypeTransform, TransparentCallableReturn, apply_decorator_with,
    callable_paramspec_and_return_with, decorator_callable_kind_with, defer_decorator_call_with,
    map_decorator_union_with, propagate_decorator_kind_with, transparent_callable_decorator_with,
    wrap_decorator_type_with,
};
use crate::types::infer::{InferenceFlags, TypeInferenceBuilder};
use crate::types::known_instance::DeprecatedInstance;
use crate::types::property_provenance::{PropertyProvenanceFacts, with_accessor_definition_with};
use crate::types::set_theoretic::builder::controlled_union::{
    UnionFacts, add_in_place_with, try_build_with,
};
use crate::types::signatures::CallableSignature;
use crate::types::storage_quote::buffer_push_quote;
use crate::types::visitor::runtime::TypeSliceDeref;
use crate::types::{
    BoundTypeVarInstance, CallableBinding, CallableType, KnownClass, PropertyInstanceType,
    RecursiveType, RecursivelyDefined, Signature, Type, TypeAliasType, UnionBuilder, UnionType,
    union_like_with,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    async fn unavailable_decorator<T>(
        &self,
        operation: DecoratorApplicationOperation,
    ) -> RunResult<T> {
        self.unavailable(SourceOperation::DecoratorApplication(operation))
            .await
    }

    /// Interns a callable kind change while preserving its signatures and deprecation marker.
    /// Cloning owns overload storage and extras, and shares parameter/constraint arenas; fund the
    /// copied owner's possible final-handle cleanup before acquiring those handles.
    pub(super) async fn callable_value_with_kind(
        &self,
        signatures: &'db CallableSignature<'db>,
        kind: CallableTypeKind,
        deprecated: Option<OverloadLiteral<'db>>,
    ) -> RunResult<Type<'db>> {
        let count = self
            .local_with_fixed_transfers(
                8,
                3 * size_of::<Option<usize>>() + size_of::<RunResult<usize>>(),
                || signatures.overloads.len(),
            )
            .await?;
        // Each signature's quotes inspect fixed metadata and arena lengths. The two outer
        // scans are bounded by overload count, including empty and inline SmallVec storage.
        let quote_work = Self::checked(count.checked_mul(64).and_then(|n| n.checked_add(8)))?;
        let (work, bytes) = self
            .local_with_fixed_transfers(quote_work, 0, || {
                let clone_work = count.checked_mul(16).and_then(|n| n.checked_add(4));
                Ok::<_, RunError>((
                    Self::checked(
                        signatures
                            .retirement_work()
                            .and_then(|retirement| retirement.checked_add(clone_work?)),
                    )?,
                    Self::checked(signatures.clone_requested_bytes())?,
                ))
            })
            .await??;
        let owned = self
            .local_with_fixed_transfers(work, bytes, || signatures.clone())
            .await?;
        let callable = self
            .access
            .owned_callable_type(owned, kind, deprecated)
            .await?;
        self.local_with_fixed_transfers(2, 0, || Type::Callable(callable))
            .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> DecoratorApplicationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Union = UnionBuilder<'db>;

    async fn known_class(&self, class: ClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.known_call_class(self.db(), Type::ClassLiteral(class))
            .await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }

    async fn union_like(&self, ty: Type<'db>) -> RunResult<Option<UnionType<'db>>> {
        union_like_with(ty, self).await
    }

    async fn function_kind(&self, function: FunctionType<'db>) -> RunResult<CallableTypeKind> {
        function.callable_type_kind_with(self.db(), self).await
    }

    async fn callable_kind(&self, callable: CallableType<'db>) -> RunResult<CallableTypeKind> {
        self.field(
            callable
                .field_requests(self.access.endpoint().field_request_context())
                .kind(),
        )
        .await
    }

    async fn function_with_kind(
        &self,
        function: FunctionType<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<Type<'db>> {
        let function = self
            .descriptor_update_future(|| {
                FunctionTypeDescriptorEffects::with_kind(self, function, kind)
            })
            .await?
            .await?;
        self.descriptor_update_local(Some(1), Some(0), || Type::FunctionLiteral(function))
            .await
    }

    async fn callable_with_kind(
        &self,
        callable: CallableType<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<Type<'db>> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || callable.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.signatures())
            .await?;
        let signatures = self.field(request).await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.deprecated())
            .await?;
        let deprecated = self.field(request).await?;
        self.callable_value_with_kind(signatures, kind, deprecated)
            .await
    }

    async fn function_with_deprecated(
        &self,
        _function: FunctionType<'db>,
        _deprecated: DeprecatedInstance<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable_decorator(DecoratorApplicationOperation::FunctionDeprecation)
            .await
    }

    async fn overload_with_deprecated(
        &self,
        _overload: OverloadLiteral<'db>,
        _deprecated: DeprecatedInstance<'db>,
    ) -> RunResult<OverloadLiteral<'db>> {
        self.unavailable_decorator(DecoratorApplicationOperation::OverloadDeprecation)
            .await
    }

    async fn callable_with_deprecated(
        &self,
        callable: CallableType<'db>,
        overload: OverloadLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        let fields = callable.field_requests(self.access.endpoint().field_request_context());
        let signatures = self.field(fields.signatures()).await?;
        let kind = self.field(fields.kind()).await?;
        self.callable_value_with_kind(signatures, kind, Some(overload))
            .await
    }

    async fn upcast_callable(&self, ty: Type<'db>) -> RunResult<Option<CallableTypes<'db>>> {
        self.reachability_callables(&ProgramEnvironment::from_program(self.program), ty)
            .await
    }

    async fn next_callable(
        &self,
        cursor: &mut slice::Iter<'_, CallableType<'db>>,
    ) -> RunResult<Option<CallableType<'db>>> {
        self.local(1, 0, || cursor.next().copied()).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn next_union(
        &self,
        cursor: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(1, 0, || cursor.next().copied()).await
    }

    async fn new_union(&self) -> RunResult<Self::Union> {
        let env = ProgramEnvironment::from_program(self.program);
        self.local(size_of::<UnionBuilder<'db>>() * 2 + 1, 0, || {
            UnionBuilder::new(self.db(), &env)
        })
        .await
    }

    async fn union_add(&self, union: &mut Self::Union, ty: Type<'db>) -> RunResult<()> {
        add_in_place_with(union, ty, UnionFacts, self).await
    }

    async fn union_recursively_defined(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<RecursivelyDefined> {
        self.union_recursion_source(union).await
    }

    async fn finish_union(
        &self,
        mut union: Self::Union,
        recursively_defined: RecursivelyDefined,
    ) -> RunResult<Type<'db>> {
        self.local(2, 0, || {
            union.merge_recursively_defined(recursively_defined)
        })
        .await?;
        Ok(try_build_with(union, UnionFacts, self)
            .await?
            .unwrap_or(Type::Never))
    }

    async fn map_union(
        &self,
        union: UnionType<'db>,
        transform: DecoratorTypeTransform,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| {
            map_decorator_union_with(union, transform, DecoratorApplicationFacts, self)
        })
        .await?
        .await
    }

    async fn transform_type(
        &self,
        ty: Type<'db>,
        transform: DecoratorTypeTransform,
    ) -> RunResult<Option<Type<'db>>> {
        match transform {
            DecoratorTypeTransform::Wrap(kind) => {
                self.allocate_future(|| {
                    wrap_decorator_type_with(ty, kind, DecoratorApplicationFacts, self)
                })
                .await?
                .await
            }
            DecoratorTypeTransform::Propagate(kind) => {
                self.allocate_future(|| propagate_decorator_kind_with(ty, kind, self))
                    .await?
                    .await
            }
        }
    }

    async fn unfold(&self, _recursive: RecursiveType<'db>) -> RunResult<Option<Type<'db>>> {
        self.unavailable_decorator(DecoratorApplicationOperation::RecursiveUnfold)
            .await
    }

    async fn unbound_recursive(&self) -> RunResult<Option<Type<'db>>> {
        self.unavailable_decorator(DecoratorApplicationOperation::UnboundRecursiveVariable)
            .await
    }

    async fn alias_value(&self, _alias: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeAliasResolution).await
    }

    async fn propagatable_kind(&self, ty: Type<'db>) -> RunResult<Option<CallableTypeKind>> {
        self.allocate_future(|| decorator_callable_kind_with(ty, DecoratorApplicationFacts, self))
            .await?
            .await
    }

    async fn try_call(
        &self,
        decorator: Type<'db>,
        decorated: Type<'db>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        let bytes = Self::checked(CallArguments::capacity_bytes(1))?;
        let work = Self::checked(bytes.checked_mul(2).and_then(|work| work.checked_add(8)))?;
        // The single entry has an empty contextual-type map. Fund its storage and disposal
        // before constructing the arguments retained across the synthetic call.
        let arguments = self
            .local(work, bytes, || CallArguments::positional([decorated]))
            .await?;
        let env = ProgramEnvironment::from_program(self.program);
        self.synthetic_call(&env, decorator, &arguments, None).await
    }

    async fn return_type(&self, bindings: &Bindings<'db>) -> RunResult<Type<'db>> {
        let env = ProgramEnvironment::from_program(self.program);
        bindings.return_type_with(self.db(), &env, self).await
    }

    async fn no_type_check(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.local(1, 0, || {
            builder
                .inference_flags()
                .contains(InferenceFlags::IN_NO_TYPE_CHECK)
        })
        .await
    }

    async fn record_failed_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: &ast::Decorator,
        decorated: Type<'db>,
    ) -> RunResult<()> {
        let (len, capacity) = self
            .local(2, 0, || {
                (
                    builder.deferred_decorator_calls.len(),
                    builder.deferred_decorator_calls.capacity(),
                )
            })
            .await?;
        let quote = buffer_push_quote::<(ExpressionNodeKey, Type<'db>)>((len, capacity, true))
            .ok_or(RunError::Contract(
                "deferred decorator call storage quotation overflow",
            ))?;
        self.local(quote.work, quote.bytes, || {
            builder
                .deferred_decorator_calls
                .push(((&decorator.expression).into(), decorated));
        })
        .await
    }

    async fn defer_failed_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: &ast::Decorator,
        decorated: Type<'db>,
    ) -> RunResult<()> {
        defer_decorator_call_with(builder, decorator, decorated, self).await
    }

    async fn transparent_result(
        &self,
        bindings: &Bindings<'db>,
        decorated: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| {
            transparent_callable_decorator_with(
                bindings,
                decorated,
                DecoratorApplicationFacts,
                self,
            )
        })
        .await?
        .await
    }

    async fn single_binding<'a>(
        &self,
        bindings: &'a Bindings<'db>,
    ) -> RunResult<Option<&'a CallableBinding<'db>>> {
        self.local(5, 0, || bindings.single_element()).await
    }

    async fn single_matching_signature<'a>(
        &self,
        binding: &'a CallableBinding<'db>,
    ) -> RunResult<Option<&'a Signature<'db>>> {
        let count = binding.overloads().len();
        let work = self
            .local(Self::checked(count.checked_add(1))?, 0, || {
                binding
                    .overloads()
                    .iter()
                    .try_fold(4usize, |work, overload| {
                        work.checked_add(overload.errors().len())?.checked_add(2)
                    })
            })
            .await?;
        self.local(Self::checked(work)?, 0, || {
            binding
                .matching_overloads()
                .exactly_one()
                .ok()
                .map(|(_, overload)| &overload.signature)
        })
        .await
    }

    async fn bind_self(
        &self,
        _signature: &Signature<'db>,
        _bound_type: Type<'db>,
    ) -> RunResult<Signature<'db>> {
        self.unavailable_decorator(DecoratorApplicationOperation::SignatureBinding)
            .await
    }

    async fn callable_paramspec_and_return(
        &self,
        ty: Type<'db>,
    ) -> RunResult<Option<(BoundTypeVarInstance<'db>, TransparentCallableReturn<'db>)>> {
        self.allocate_future(|| {
            callable_paramspec_and_return_with(ty, DecoratorApplicationFacts, self)
        })
        .await?
        .await
    }

    async fn single_signature(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<Option<&'db Signature<'db>>> {
        let signatures = self
            .field(
                callable
                    .field_requests(self.access.endpoint().field_request_context())
                    .signatures(),
            )
            .await?;
        self.local(1, 0, || match signatures.overloads.as_slice() {
            [signature] => Some(signature),
            _ => None,
        })
        .await
    }

    async fn known_awaitable(&self, _ty: Type<'db>) -> RunResult<Option<Specialization<'db>>> {
        self.unavailable_decorator(DecoratorApplicationOperation::AwaitableSpecialization)
            .await
    }

    async fn single_type_argument(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let types = self
            .field_with_profile(
                specialization
                    .field_requests(self.access.endpoint().field_request_context())
                    .types(),
                &TypeSliceDeref,
            )
            .await?;
        self.local(1, 0, || match types.as_ref() {
            [inner] => Some(*inner),
            _ => None,
        })
        .await
    }

    async fn same_typevar(
        &self,
        left: BoundTypeVarInstance<'db>,
        right: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let context = self.access.endpoint().field_request_context();
        let left = self.field(left.identity_request(context)).await?;
        let right = self.field(right.identity_request(context)).await?;
        self.local(size_of_val(&left) * 2 + 1, 0, || left == right)
            .await
    }

    async fn function_callable(&self, function: FunctionType<'db>) -> RunResult<Type<'db>> {
        function
            .into_callable_type_with(self.db(), self)
            .await
            .map(Type::Callable)
    }

    async fn dynamic_return_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<bool> {
        self.is_lint_enabled_source(builder, &DYNAMIC_FUNCTION_DECORATOR_RETURN)
            .await
    }

    async fn equivalent_to_any(&self, _ty: Type<'db>) -> RunResult<bool> {
        self.unavailable_decorator(DecoratorApplicationOperation::DynamicReturnComparison)
            .await
    }

    async fn report_dynamic_return(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _decorator: &ast::Decorator,
        _decorated: Type<'db>,
        _bindings: &Bindings<'db>,
        _function: &ast::StmtFunctionDef,
        _inferred: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable_decorator(DecoratorApplicationOperation::DynamicReturnDiagnostic)
            .await
    }

    async fn apply(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        decorator: Type<'db>,
        decorated: Type<'db>,
        node: &ast::Decorator,
        function: Option<&ast::StmtFunctionDef>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| {
            apply_decorator_with(
                builder,
                decorator,
                decorated,
                node,
                function,
                DecoratorApplicationFacts,
                self,
            )
        })
        .await?
        .await
    }

    async fn property_accessor_definition(
        &self,
        property: PropertyInstanceType<'db>,
        decorator: Type<'db>,
        decorated: Type<'db>,
        definition: Definition<'db>,
    ) -> RunResult<Type<'db>> {
        with_accessor_definition_with(
            property,
            decorator,
            decorated,
            definition,
            PropertyProvenanceFacts,
            self,
        )
        .await
        .map(Type::PropertyInstance)
    }
}
