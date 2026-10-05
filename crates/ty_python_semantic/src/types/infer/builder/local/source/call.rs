//! Call classification and metadata use the shared source branches before parameter binding.

use std::convert::Infallible;

use salsa::execution_probe::{FieldReadProfile, FieldReturnMode, NativeValueQuote, TaskEndpoint};
use ty_python_core::ast_node_ref::AstNodeRef;

use super::*;
use crate::lint::LintMetadata;
use crate::types::BoundMethodType;
use crate::types::abstract_methods::AbstractMethods;
use crate::types::call::CallableBinding;
#[cfg(test)]
use crate::types::call::bind::constructor_matching::{ConstructorMatchingEffects, ConstructorMatchingOperation};
use crate::types::call::bind::deprecation::{
    DeprecationDependency, DeprecationEffects, DeprecationQuote,
};
use crate::types::call::bind::return_type::ReturnTypeEffects;
use crate::types::call::bind::source::initial_bindings_quote;
use crate::types::call::function_bindings::{
    FunctionBindingEffects, FunctionBindingSpecial, function_bindings_with,
};
use crate::types::call::invocation::{InvocationContext, InvocationEffects};
use crate::types::call::preparation::known_class::{
    KnownClassBindingFacts, known_class_bindings_with, wrapper_descriptor_bindings_with,
};
use crate::types::call::preparation::{
    BindingPreparationDependency, BindingPreparationEffects, BindingPreparationFacts,
    bindings_body_with,
};
use crate::types::class::protocol_status::static_is_protocol_with;
use crate::types::class::slots::layout::InstanceLayoutEffects;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::generics::enclosing_binding_contexts;
use crate::types::infer::builder::local::call::class_metadata::{
    ClassMetadataEffects, class_metadata_with,
};
use crate::types::infer::builder::source_definition::controlled::storage::ordered_merge;
use crate::types::infer::builder::{range, typeguard};
use crate::types::member_lookup::mro_dispatch::MroLookupEffects;
use crate::types::protocol_class::ProtocolClass;
use crate::types::set_theoretic::assembly::{TypeAssemblyEffects, TypeElements};
use crate::types::signatures::CallableSignature;
use crate::types::subclass_of::SubclassOfInner;
use crate::types::typed_dict::TypedDictType;
use crate::types::typevar::BindingContext;

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

struct AssignmentReferenceClone;

impl FieldReadProfile<Option<AstNodeRef<ast::StmtAssign>>> for AssignmentReferenceClone {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        _endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call Option<AstNodeRef<ast::StmtAssign>>,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        if mode != FieldReturnMode::Clone {
            return Err(RunError::Contract(
                "assignment reference field conversion is not a clone",
            ));
        }
        // AstNodeRef owns only scalar identity and debug metadata, so cloning cannot allocate.
        Ok(NativeValueQuote {
            work: size_of::<Option<AstNodeRef<ast::StmtAssign>>>() + 1,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Prepares and matches a local call, retaining its guard and actual enclosing binding contexts.
    /// The returned tree is ready for the separate argument-checking phase.
    pub(in crate::types::infer) async fn match_local_bindings(
        &self,
        env: &ProgramEnvironment<'db>,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        callable: Type<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<(Option<CallableRecursionGuard<'db>>, Bindings<'db>)> {
        let recursion_guard = self.constructor_matching_child(|| InvocationEffects::new_guard(self)).await?;
        let mut bindings = self.constructor_matching_child(|| InvocationEffects::prepare(
            self,
            InvocationContext { db: self.access.db(), env, arguments },
            callable,
            &recursion_guard,
        )).await?;
        // Parent scopes precede their children, bounding both the walk and its collected contexts.
        let scopes = Self::checked((scope.as_u32() as usize).checked_add(1))?;
        let work = Self::checked(scopes.checked_mul(8).and_then(|n| n.checked_add(4)))?;
        let bytes = Self::checked(scopes.checked_mul(3).and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_mul(size_of::<BindingContext<'db>>())))?;
        self.local_with_fixed_transfers(work, bytes, || {
            bindings.set_enclosing_binding_contexts(enclosing_binding_contexts(index, scope));
        }).await?;
        self.constructor_matching_child(|| bindings.match_parameters_with(self.access.db(), env, arguments, self)).await?;
        #[cfg(test)]
        self.before_matching(ConstructorMatchingOperation::ResultTransfer);
        self.local_with_fixed_transfers(3, 0, || {
            #[cfg(test)]
            self.after_matching(ConstructorMatchingOperation::ResultTransfer);
            (Some(recursion_guard), bindings)
        }).await
    }

    pub(in crate::types::infer::builder) async fn known_call_class(
        &self,
        db: &'db dyn crate::Db,
        ty: Type<'db>,
    ) -> RunResult<Option<KnownClass>> {
        let Some(class) = ty.as_class_literal().and_then(ClassLiteral::as_static) else {
            return Ok(None);
        };
        self.field(class.field_requests(db).known()).await
    }

    pub(in crate::types::infer::builder) async fn call_function_is_known(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        known: KnownFunction,
    ) -> RunResult<bool> {
        self.work(4).await?;
        let Some(function) = ty.as_function_literal() else {
            return Ok(false);
        };
        Ok(FunctionBindingEffects::known(self, builder.db(), function).await? == Some(known))
    }

    pub(in crate::types::infer::builder) async fn typed_dict_call_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
    ) -> RunResult<Option<TypingModule>> {
        self.work(1).await?;
        match ty {
            Type::SpecialForm(SpecialFormType::TypedDict(module)) => Ok(Some(module)),
            Type::Union(union) => {
                let elements = self
                    .field(union.field_requests(builder.db()).elements())
                    .await?;
                self.local(Self::checked(elements.len().checked_add(2))?, 0, || {
                    TypingModule::from_typed_dict_elements(elements)
                })
                .await
            }
            _ => Ok(None),
        }
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> call::CallEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn collection_initializer(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::ExprCall,
        ty: Type<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.work(16).await?;
        if !expression.arguments.is_empty() {
            return Ok(None);
        }
        let Some(indexed) = self
            .local(1, 0, || builder.index.try_expression(expression))
            .await?
        else {
            return Ok(None);
        };
        if self
            .field_with_profile(
                indexed.read_fields(builder.db()).assigned_to(),
                &AssignmentReferenceClone,
            )
            .await?
            .is_none()
        {
            return Ok(None);
        }
        let Some(name) = expression.func.as_name_expr() else {
            return Ok(None);
        };
        let Some(class) = self.known_call_class(builder.db(), ty).await? else {
            return Ok(None);
        };
        Ok(matches!(
            (name.id.as_str(), class),
            ("list", KnownClass::List) | ("set", KnownClass::Set) | ("dict", KnownClass::Dict)
        )
        .then_some(class))
    }

    async fn class_is_known(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        known: KnownClass,
    ) -> RunResult<bool> {
        self.work(4).await?;
        Ok(self.known_call_class(builder.db(), ty).await? == Some(known))
    }

    async fn function_is_known(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        known: KnownFunction,
    ) -> RunResult<bool> {
        self.call_function_is_known(builder, ty, known).await
    }

    async fn named_tuple_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<NamedTupleKind>> {
        self.work(4).await?;
        Ok(match ty {
            Type::SpecialForm(SpecialFormType::NamedTuple) => Some(NamedTupleKind::Typing),
            Type::FunctionLiteral(function) => {
                (FunctionBindingEffects::known(self, builder.db(), function).await?
                    == Some(KnownFunction::NamedTuple))
                .then_some(NamedTupleKind::Collections)
            }
            _ => None,
        })
    }

    async fn enum_base(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.work(4).await?;
        Ok(self
            .known_call_class(builder.db(), ty)
            .await?
            .filter(|class| enum_call::is_enum_functional_call_base(*class)))
    }

    async fn typed_dict_module(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<TypingModule>> {
        self.typed_dict_call_module(builder, ty).await
    }

    async fn is_notimplemented(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.local(8, 0, || {
            infallible(call::SynchronousCallEffects::is_notimplemented(
                &call::OrdinaryCallEffects,
                builder,
                ty,
            ))
        })
        .await
    }

    async fn class_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        if matches!(ty, Type::SubclassOf(subclass) if matches!(subclass.subclass_of(), SubclassOfInner::TypeVar(_)))
        {
            return self.unavailable(SourceOperation::CallMetadata).await;
        }
        self.local(4, 0, || {
            infallible(call::SynchronousCallEffects::class_type(
                &call::OrdinaryCallEffects,
                builder,
                ty,
            ))
        })
        .await
    }

    async fn is_typed_dict(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassType<'db>,
    ) -> RunResult<bool> {
        MroLookupEffects::is_typed_dict(self, class).await
    }

    async fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.local(
            Self::checked(builder.expressions.capacity().checked_add(2))?,
            0,
            || {
                infallible(call::SynchronousCallEffects::expression_type(
                    &call::OrdinaryCallEffects,
                    builder,
                    expression,
                ))
            },
        )
        .await
    }

    async fn metadata_checks(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
    ) -> RunResult<()> {
        call::metadata_with(builder, data, call::CallFacts, self).await
    }

    async fn function_in_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<bool> {
        self.work(16).await?;
        let file = self.function_file(function).await?;
        let python_file = self
            .field(file.read_fields(builder.db()).python_file())
            .await?;
        let file = self
            .field(python_file.read_fields(builder.db()).file())
            .await?;
        if file != builder.file() {
            return Ok(false);
        }
        let definition = function.definition_with(builder.db(), self).await?;
        Ok(self
            .field(definition.read_fields(builder.db()).scope_id())
            .await?
            == builder.scope())
    }

    async fn record_called_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<()> {
        let quote = ordered_merge::<FunctionType<'db>>(
            builder.called_functions.len(),
            builder.called_functions.capacity(),
            1,
        )
        .ok_or(RunError::Contract(
            "called-function storage quotation overflow",
        ))?;
        self.local(quote.work, quote.bytes, || {
            builder.called_functions.reserve_exact(1);
            infallible(call::SynchronousCallEffects::record_called_function(
                &call::OrdinaryCallEffects,
                builder,
                function,
            ));
        })
        .await
    }

    async fn staticmethod_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<bool> {
        self.work(16).await?;
        function
            .has_staticmethod_declaration_with(builder.db(), self)
            .await
    }

    async fn special_call(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _call: &ast::ExprCall,
        _ty: Type<'db>,
        _tcx: TypeContext<'db>,
        _kind: call::SpecialCall<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.unavailable(SourceOperation::CallSpecial).await
    }

    async fn optional_special_call(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _call: &ast::ExprCall,
        _tcx: TypeContext<'db>,
        _kind: call::OptionalSpecialCall,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.unavailable(SourceOperation::CallSpecial).await
    }

    async fn typed_dict_method(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _call: &ast::ExprCall,
        _typed_dict: TypedDictType<'db>,
        _attribute: &ast::ExprAttribute,
        _first: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.unavailable(SourceOperation::CallSpecial).await
    }

    async fn report_ineffective_final(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _call: &ast::ExprCall,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::CallMetadata).await
    }

    async fn bound_method_metadata(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _call: &ast::ExprCall,
        _method: BoundMethodType<'db>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::CallMetadata).await
    }

    async fn static_method_metadata(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _call: &ast::ExprCall,
        _function: FunctionType<'db>,
        _attribute: &ast::ExprAttribute,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::CallMetadata).await
    }

    async fn class_metadata(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        class: ClassType<'db>,
    ) -> Result<(), Self::Error> {
        class_metadata_with(builder, data, class, self).await
    }

    async fn match_bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<(Option<CallableRecursionGuard<'db>>, Bindings<'db>), Self::Error> {
        let scope = self
            .field(builder.scope().read_fields(builder.db()).file_scope_id())
            .await?;
        self.constructor_matching_child(|| self.match_local_bindings(
            builder.program_environment(), builder.index, scope, data.callable_type, arguments,
        )).await
    }

    async fn report_implicit_calls(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _data: &call::CallData<'db, '_>,
        bindings: &Bindings<'db>,
    ) -> Result<(), Self::Error> {
        let report = self
            .local(2, 0, || {
                bindings.has_implicit_dunder_new_is_possibly_unbound()
                    || bindings.has_implicit_dunder_init_is_possibly_unbound()
            })
            .await?;
        if report {
            self.unavailable(SourceOperation::CallMetadata).await
        } else {
            Ok(())
        }
    }

    async fn failed_call(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        call::completion::report_failed_with(builder, data, bindings, self).await?;
        bindings
            .return_type_with(builder.db(), builder.program_environment(), self)
            .await
    }

    async fn successful_checks(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        arguments: &CallArguments<'_, 'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        call::completion::successful_checks_with(builder, data, arguments, bindings, self).await
    }

    async fn receiver_constraints(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        arguments: &mut CallArguments<'_, 'db>,
    ) -> Result<(), Self::Error> {
        call::completion::receiver_constraints_with(builder, data, arguments, self).await
    }

    async fn refine_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        arguments: &CallArguments<'_, 'db>,
        _bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        self.work(2).await?;
        let refined = range::infer_builtin_range_instance_type_with(
            builder,
            data.callable_type,
            &data.call.arguments,
            arguments,
            self,
        )
        .await?;
        if refined.is_some() {
            self.unavailable(SourceOperation::CallRangeInference).await
        } else {
            Ok(())
        }
    }

    async fn return_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        bindings
            .return_type_with(builder.db(), builder.program_environment(), self)
            .await
    }

    async fn collection_return_matches(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
        _class: KnownClass,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::CallCollectionReturn)
            .await
    }

    async fn empty_collection(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _data: &call::CallData<'db, '_>,
        _class: KnownClass,
        _fallback: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.unavailable(SourceOperation::CallSpecial).await
    }

    async fn bind_type_guard(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &call::CallData<'db, '_>,
        bindings: &Bindings<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.work(2).await?;
        typeguard::bind_type_guard_return_type_with(
            builder.db(),
            builder.scope(),
            ty,
            bindings,
            &data.call.arguments,
            self,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn known(
        &self,
        db: &'db dyn crate::Db,
        function: FunctionType<'db>,
    ) -> RunResult<Option<KnownFunction>> {
        let literal = self.field(function.field_requests(db).literal()).await?;
        self.field(literal.last_definition.field_requests(db).known())
            .await
    }

    async fn special(
        &self,
        _db: &'db dyn crate::Db,
        _env: &crate::ProgramEnvironment<'db>,
        _callable_type: Type<'db>,
        _kind: FunctionBindingSpecial,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(SourceOperation::CallSpecial).await
    }

    async fn signature(
        &self,
        _db: &'db dyn crate::Db,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.function_signature(function).await
    }

    async fn bindings_from_signature(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.work(Self::checked(
            signature
                .overloads
                .len()
                .checked_mul(16)
                .and_then(|work| work.checked_add(1)),
        )?)
        .await?;
        let quote = initial_bindings_quote(signature)
            .ok_or(RunError::Contract("initial bindings quotation overflow"))?;
        self.local(1, size_of::<Option<Bindings<'db>>>(), || ())
            .await?;
        let mut owner = None;
        let action = || {
            owner = Some(CallableBinding::from_signature(callable_type, signature).into());
        };
        let bytes = Self::checked(
            quote
                .bytes
                .checked_add(size_of::<Bindings<'db>>())
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Bindings<'db>>>()))
                .and_then(|bytes| bytes.checked_add(size_of_val(&action))),
        )?;
        self.local(Self::checked(quote.work.checked_add(3))?, bytes, action)
            .await?;
        owner.ok_or(RunError::Contract("initial bindings were not constructed"))
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ReturnTypeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn local<T>(&self, work: Option<usize>, action: impl FnOnce() -> T) -> RunResult<T> {
        SourceEffects::local(self, Self::checked(work)?, 0, action).await
    }

    async fn constructor_return<T>(&self, _action: impl FnOnce() -> T) -> RunResult<T> {
        self.unavailable(SourceOperation::CallConstructorReturn)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeAssemblyEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn union<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn crate::Db,
        _env: &crate::ProgramEnvironment<'db>,
        _first: I::Item,
        _second: I::Item,
        _remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::CallReturnUnionBuilder)
            .await
    }

    async fn intersection<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn crate::Db,
        _env: &crate::ProgramEnvironment<'db>,
        _first: I::Item,
        _second: I::Item,
        _remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::CallReturnIntersectionBuilder)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DeprecationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn local<T>(
        &self,
        quote: Option<DeprecationQuote>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = quote.ok_or(RunError::Contract("deprecation quotation overflow"))?;
        SourceEffects::local(self, quote.work, quote.requested_bytes, action).await
    }

    async fn dependency<T>(
        &self,
        dependency: DeprecationDependency,
        _action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.unavailable(match dependency {
            DeprecationDependency::BoundMethodType => {
                SourceOperation::CallDeprecationBoundMethodType
            }
            DeprecationDependency::BoundMethodFunction => {
                SourceOperation::CallDeprecationBoundMethodFunction
            }
            DeprecationDependency::DownstreamConstructor => {
                SourceOperation::CallDeprecationDownstreamConstructor
            }
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> call::completion::CompletionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn decision<T, F: FnOnce() -> T>(&self, work: Option<usize>, action: F) -> RunResult<T> {
        let work = Self::checked(work.and_then(|work| work.checked_add(6)))?;
        let bytes = size_of::<F>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<Option<F>>()))
            .and_then(|bytes| bytes.checked_add(size_of::<T>().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>().checked_mul(2)?));
        self.local(work, Self::checked(bytes)?, action).await
    }

    async fn begin_checks(&self, bindings: &Bindings<'db>) -> RunResult<()> {
        let work = self
            .local(
                Self::checked(bindings.argument_context_root_len().checked_add(1))?,
                0,
                || bindings.direct_type_context_work(),
            )
            .await?;
        self.work(Self::checked(work.and_then(|work| work.checked_add(8)))?)
            .await
    }

    async fn lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> RunResult<bool> {
        self.is_lint_enabled_source(builder, lint).await
    }

    async fn static_class(
        &self,
        _db: &'db dyn crate::Db,
        class: ClassType<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let class = crate::types::class::code_generator::CodeGeneratorEffects::static_class_literal(
            self, class,
        )
        .await?;
        self.local(1, 0, || class.map(|(class, _)| class)).await
    }

    async fn code_generator(
        &self,
        _db: &'db dyn crate::Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::class::CodeGeneratorKind<'db>>> {
        crate::types::class::static_code_generator_with(class, self).await
    }

    async fn completion<T>(
        &self,
        dependency: call::completion::CompletionDependency,
        _action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.unavailable(match dependency {
            call::completion::CompletionDependency::DeprecationDiagnostic => {
                SourceOperation::CallDeprecationDiagnostic
            }
            call::completion::CompletionDependency::DiscardedExtraArguments => {
                SourceOperation::CallDiscardedExtraArguments
            }
            call::completion::CompletionDependency::KnownFunction => {
                SourceOperation::CallKnownFunctionCheck
            }
            call::completion::CompletionDependency::KnownClass => {
                SourceOperation::CallKnownClassCheck
            }
            call::completion::CompletionDependency::NeverReveal => SourceOperation::CallNeverReveal,
            call::completion::CompletionDependency::ReceiverConstraints => {
                SourceOperation::CallReceiverConstraints
            }
            call::completion::CompletionDependency::CallDiagnostic => {
                SourceOperation::CallDiagnostic
            }
        })
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> range::RangeInferenceEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn class_is_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        call::CallEffects::class_is_known(
            self,
            builder,
            Type::ClassLiteral(class),
            KnownClass::Range,
        )
        .await
    }

    async fn infer_range(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _arguments: &ast::Arguments,
        _call_arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::CallRangeInference).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> typeguard::TypeGuardReturnEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn bind_positive(
        &self,
        _db: &'db dyn crate::Db,
        _scope: ty_python_core::scope::ScopeId<'db>,
        _return_ty: Type<'db>,
        _bindings: &Bindings<'db>,
        _arguments: &ast::Arguments,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::CallTypeGuardBinding)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BindingPreparationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn guarded(
        &self,
        _db: &'db dyn crate::Db,
        _env: &crate::ProgramEnvironment<'db>,
        _ty: Type<'db>,
        _unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(SourceOperation::CallBindings).await
    }

    async fn body(
        &self,
        db: &'db dyn crate::Db,
        env: &crate::ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        bindings_body_with(
            db,
            env,
            ty,
            unknown_is_recovery,
            BindingPreparationFacts,
            self,
        )
        .await
    }

    async fn forward(
        &self,
        _db: &'db dyn crate::Db,
        _env: &crate::ProgramEnvironment<'db>,
        _ty: Type<'db>,
        _unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(SourceOperation::CallBindings).await
    }

    async fn function(
        &self,
        db: &'db dyn crate::Db,
        env: &crate::ProgramEnvironment<'db>,
        function: FunctionType<'db>,
    ) -> RunResult<Bindings<'db>> {
        function_bindings_with(db, env, function, self).await
    }

    async fn known_class(
        &self,
        _db: &'db dyn crate::Db,
        env: &crate::ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<Bindings<'db>>> {
        self.environment_program(env).await?;
        self.allocate_future(|| known_class_bindings_with(ty, class, KnownClassBindingFacts, self))
            .await?
            .await
    }

    async fn constructor(
        &self,
        _db: &'db dyn crate::Db,
        _env: &crate::ProgramEnvironment<'db>,
        _ty: Type<'db>,
        _class: ClassType<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(SourceOperation::CallBindings).await
    }

    async fn dependency(
        &self,
        _db: &'db dyn crate::Db,
        env: &crate::ProgramEnvironment<'db>,
        ty: Type<'db>,
        dependency: BindingPreparationDependency<'db>,
        _unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        match dependency {
            BindingPreparationDependency::WrapperDescriptor(wrapper) => {
                self.environment_program(env).await?;
                self.allocate_future(|| wrapper_descriptor_bindings_with(ty, wrapper, self))
                    .await?
                    .await
            }
            _ => self.unavailable(SourceOperation::CallBindings).await,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassMetadataEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn decision<T>(&self, work: usize, action: impl FnOnce() -> T) -> RunResult<T> {
        self.local(work, 0, action).await
    }

    async fn lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> RunResult<bool> {
        self.is_lint_enabled_source(builder, lint).await
    }

    async fn protocol_class(
        &self,
        _db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<Option<ProtocolClass<'db>>> {
        let literal = InstanceLayoutEffects::class_literal(self, class).await?;
        let file = self.class_file(literal).await?;
        self.check_file_program(file).await?;
        let ClassLiteral::Static(literal) = literal else {
            return self.local(1, 0, || None).await;
        };
        let is_protocol = static_is_protocol_with(literal, self).await?;
        self.local(2, 0, || {
            is_protocol.then(|| ProtocolClass::from_class(class))
        })
        .await
    }

    async fn abstract_methods(
        &self,
        _db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<AbstractMethods<'db>> {
        self.class_abstract_methods(class).await
    }

    async fn known(&self, db: &'db dyn Db, class: ClassType<'db>) -> RunResult<Option<KnownClass>> {
        let literal = InstanceLayoutEffects::class_literal(self, class).await?;
        let file = self.class_file(literal).await?;
        self.check_file_program(file).await?;
        let ClassLiteral::Static(literal) = literal else {
            return self.local(1, 0, || None).await;
        };
        self.field(literal.field_requests(db).known()).await
    }

    async fn diagnostic<T>(&self, _action: impl FnOnce() -> T) -> RunResult<T> {
        self.unavailable(SourceOperation::CallMetadata).await
    }
}
