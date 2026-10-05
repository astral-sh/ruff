//! Shared Call dispatch and completion, with source-specific operations kept explicit.

pub(super) mod class_metadata;
pub(super) mod completion;

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::super::*;
use super::arguments::{BorrowedArguments, OwnedArguments};
use crate::types::BoundMethodType;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::typed_dict::TypedDictType;

pub(super) struct CallData<'db, 'expr> {
    pub(super) call: &'expr ast::ExprCall,
    pub(super) callable_type: Type<'db>,
    pub(super) tcx: TypeContext<'db>,
    collection_initializer_class: Option<KnownClass>,
    class: Option<ClassType<'db>>,
}

pub(super) enum Start<'db, 'expr> {
    Complete(Type<'db>),
    Prepare(CallData<'db, 'expr>),
}

pub(super) enum Prepared<'db, 'expr> {
    Complete(Type<'db>),
    Arguments(
        CallData<'db, 'expr>,
        OwnedArguments<'expr, 'db>,
        Option<CallableRecursionGuard<'db>>,
    ),
}

pub(super) enum SpecialCall<'db> {
    BuiltinType,
    NewClass,
    NamedTuple(NamedTupleKind),
    TypedDict(TypingModule),
    TypeForm,
    TypedDictConstructor(ClassType<'db>),
    NotImplemented,
}

pub(super) enum OptionalSpecialCall {
    KeywordDict(bool),
    Enum(KnownClass),
}

pub(super) struct CallFacts;
pub(super) struct OrdinaryCallEffects;

shared_semantic_family! {
    #[synchronous(SynchronousCallEffects)]
    pub(super) trait CallEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn collection_initializer(&self, builder: &TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn class_is_known(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, known: KnownClass) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn function_is_known(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, known: KnownFunction) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn named_tuple_kind(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<NamedTupleKind>, Self::Error>;
        #[operation(child)]
        async fn enum_base(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn typed_dict_module(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<TypingModule>, Self::Error>;
        #[operation(child)]
        async fn is_notimplemented(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn class_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn is_typed_dict(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn special_call(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall, ty: Type<'db>, tcx: TypeContext<'db>, kind: SpecialCall<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn optional_special_call(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall, tcx: TypeContext<'db>, kind: OptionalSpecialCall) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn expression_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn typed_dict_method(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall, typed_dict: TypedDictType<'db>, attribute: &ast::ExprAttribute, first: &ast::Expr) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn metadata_checks(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn function_in_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn record_called_function(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn report_ineffective_final(&self, builder: &TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn staticmethod_declaration(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn bound_method_metadata(&self, builder: &TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall, method: BoundMethodType<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn static_method_metadata(&self, builder: &TypeInferenceBuilder<'db, 'ast>, call: &ast::ExprCall, function: FunctionType<'db>, attribute: &ast::ExprAttribute) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn class_metadata(&self, builder: &TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, class: ClassType<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn match_bindings(&self, builder: &TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, arguments: &CallArguments<'_, 'db>) -> Result<(Option<CallableRecursionGuard<'db>>, Bindings<'db>), Self::Error>;
        #[operation(child)]
        async fn report_implicit_calls(&self, builder: &TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, bindings: &Bindings<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn failed_call(&self, builder: &TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, bindings: &Bindings<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn successful_checks(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, arguments: &CallArguments<'_, 'db>, bindings: &mut Bindings<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn receiver_constraints(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, arguments: &mut CallArguments<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn refine_range(&self, builder: &TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, arguments: &CallArguments<'_, 'db>, bindings: &mut Bindings<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn return_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, bindings: &Bindings<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn collection_return_matches(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, class: KnownClass) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn empty_collection(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, class: KnownClass, fallback: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn bind_type_guard(&self, builder: &TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, bindings: &Bindings<'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl CallFacts {
        fn dict_initializer(&self, class: Option<KnownClass>) -> bool { class == Some(KnownClass::Dict) }
        fn is_type_form<'db>(&self, ty: Type<'db>) -> bool { ty == Type::SpecialForm(SpecialFormType::TypeForm) }
        fn attribute<'expr>(&self, call: &'expr ast::ExprCall) -> Option<&'expr ast::ExprAttribute> { call.func.as_attribute_expr() }
        fn typed_dict_method(&self, attribute: &ast::ExprAttribute) -> bool { matches!(attribute.attr.id.as_str(), "get" | "pop" | "setdefault") }
        fn first_argument<'expr>(&self, call: &'expr ast::ExprCall) -> Option<&'expr ast::Expr> { call.arguments.args.first() }
    }

    #[synchronous(start_sync)]
    #[capabilities(effects = CallEffects, facts = CallFacts)]
    #[passive_values(CallData, Start::Complete, Start::Prepare, OptionalSpecialCall::KeywordDict, OptionalSpecialCall::Enum, SpecialCall::BuiltinType, SpecialCall::NewClass, SpecialCall::NamedTuple, SpecialCall::TypedDict, SpecialCall::TypeForm, SpecialCall::NotImplemented, SpecialCall::TypedDictConstructor, KnownClass::Dict, KnownClass::Type, KnownFunction::NewClass)]
    pub(super) async fn start_with<'db, 'ast, 'expr, E: CallEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, call: &'expr ast::ExprCall, callable_type: Type<'db>, tcx: TypeContext<'db>, facts: CallFacts, effects: &E,
    ) -> Result<Start<'db, 'expr>, E::Error> {
        let collection_initializer_class = effects.collection_initializer(builder, call, callable_type).await?;
        if effects.class_is_known(builder, callable_type, KnownClass::Dict).await? {
            if let Some(ty) = effects.optional_special_call(builder, call, tcx, OptionalSpecialCall::KeywordDict(facts.dict_initializer(collection_initializer_class))).await? {
                return Ok(Start::Complete(ty));
            }
        }
        if effects.class_is_known(builder, callable_type, KnownClass::Type).await? {
            let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::BuiltinType).await?;
            return Ok(Start::Complete(ty));
        }
        if effects.function_is_known(builder, callable_type, KnownFunction::NewClass).await? {
            let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::NewClass).await?;
            return Ok(Start::Complete(ty));
        }
        if let Some(kind) = effects.named_tuple_kind(builder, callable_type).await? {
            let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::NamedTuple(kind)).await?;
            return Ok(Start::Complete(ty));
        }
        if let Some(base) = effects.enum_base(builder, callable_type).await? {
            if let Some(ty) = effects.optional_special_call(builder, call, tcx, OptionalSpecialCall::Enum(base)).await? {
                return Ok(Start::Complete(ty));
            }
        }
        if let Some(module) = effects.typed_dict_module(builder, callable_type).await? {
            let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::TypedDict(module)).await?;
            return Ok(Start::Complete(ty));
        }
        if facts.is_type_form(callable_type) {
            let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::TypeForm).await?;
            return Ok(Start::Complete(ty));
        }
        if effects.is_notimplemented(builder, callable_type).await? {
            let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::NotImplemented).await?;
            return Ok(Start::Complete(ty));
        }
        let class = effects.class_type(builder, callable_type).await?;
        if let Some(class) = class {
            if effects.is_typed_dict(builder, class).await? {
                let ty = effects.special_call(builder, call, callable_type, tcx, SpecialCall::TypedDictConstructor(class)).await?;
                return Ok(Start::Complete(ty));
            }
        }
        Ok(Start::Prepare(CallData { call, callable_type, tcx, collection_initializer_class, class }))
    }

    #[synchronous(prepared_sync)]
    #[capabilities(effects = CallEffects, facts = CallFacts)]
    #[passive_values(Prepared::Complete, Prepared::Arguments, OwnedArguments)]
    pub(super) async fn prepared_with<'db, 'ast, 'expr, E: CallEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, data: CallData<'db, 'expr>, arguments: CallArguments<'expr, 'db>, facts: CallFacts, effects: &E,
    ) -> Result<Prepared<'db, 'expr>, E::Error> {
        if let Some(attribute) = facts.attribute(data.call) {
            let value_type = effects.expression_type(builder, &attribute.value).await?;
            if let Type::TypedDict(typed_dict) = value_type {
                if facts.typed_dict_method(attribute) {
                    if let Some(first) = facts.first_argument(data.call) {
                        if let Some(ty) = effects.typed_dict_method(builder, data.call, typed_dict, attribute, first).await? {
                            return Ok(Prepared::Complete(ty));
                        }
                    }
                }
            }
        }
        effects.metadata_checks(builder, &data).await?;
        let (recursion_guard, bindings) = effects.match_bindings(builder, &data, &arguments).await?;
        effects.report_implicit_calls(builder, &data, &bindings).await?;
        Ok(Prepared::Arguments(data, OwnedArguments { arguments, bindings }, recursion_guard))
    }

    #[synchronous(metadata_sync)]
    #[capabilities(effects = CallEffects, facts = CallFacts)]
    #[passive_values(KnownFunction::Final)]
    pub(super) async fn metadata_with<'db, 'ast, E: CallEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, '_>, facts: CallFacts, effects: &E,
    ) -> Result<(), E::Error> {
        if let Type::FunctionLiteral(function) = data.callable_type {
            if effects.function_in_scope(builder, function).await? {
                effects.record_called_function(builder, function).await?;
            }
            if effects.function_is_known(builder, data.callable_type, KnownFunction::Final).await? {
                effects.report_ineffective_final(builder, data.call).await?;
            }
        }
        match data.callable_type {
            Type::BoundMethod(method) => effects.bound_method_metadata(builder, data.call, method).await?,
            Type::FunctionLiteral(function) => {
                if effects.staticmethod_declaration(builder, function).await? {
                    if let Some(attribute) = facts.attribute(data.call) {
                        effects.static_method_metadata(builder, data.call, function, attribute).await?;
                    }
                }
            }
            _ => {}
        }
        if let Some(class) = data.class {
            effects.class_metadata(builder, data, class).await?;
        }
        Ok(())
    }

    #[synchronous(finish_sync)]
    #[capabilities(effects = CallEffects)]
    #[passive_values()]
    pub(super) async fn finish_with<'db, 'ast, 'expr, E: CallEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, data: &CallData<'db, 'expr>, storage: BorrowedArguments<'_, 'expr, 'db>, result: Result<(), CallErrorKind>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if let Err(_) = result {
            return effects.failed_call(builder, data, storage.bindings).await;
        }
        effects.successful_checks(builder, data, storage.arguments, storage.bindings).await?;
        effects.receiver_constraints(builder, data, storage.arguments).await?;
        effects.refine_range(builder, data, storage.arguments, storage.bindings).await?;
        let return_ty = effects.return_type(builder, storage.bindings).await?;
        let return_ty = match data.collection_initializer_class {
            Some(class @ (KnownClass::List | KnownClass::Set)) => {
                if effects.collection_return_matches(builder, return_ty, class).await? {
                    effects.empty_collection(builder, data, class, return_ty).await?
                } else { return_ty }
            }
            _ => return_ty,
        };
        effects.bind_type_guard(builder, data, storage.bindings, return_ty).await
    }
}

impl<'db, 'ast> SynchronousCallEffects<'db, 'ast> for OrdinaryCallEffects {
    type Error = Infallible;
    fn collection_initializer(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
        ty: Type<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(builder.local_collection_initializer(call, ty))
    }
    fn class_is_known(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        known: KnownClass,
    ) -> Result<bool, Self::Error> {
        Ok(ty
            .as_class_literal()
            .is_some_and(|class| class.is_known(builder.db(), known)))
    }
    fn function_is_known(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        known: KnownFunction,
    ) -> Result<bool, Self::Error> {
        Ok(ty
            .as_function_literal()
            .is_some_and(|function| function.is_known(builder.db(), known)))
    }
    fn named_tuple_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<NamedTupleKind>, Self::Error> {
        Ok(NamedTupleKind::from_type(builder.db(), ty))
    }
    fn enum_base(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(enum_call::enum_functional_call_base(builder.db(), ty))
    }
    fn typed_dict_module(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<TypingModule>, Self::Error> {
        Ok(TypingModule::from_typed_dict_type(builder.db(), ty))
    }
    fn is_notimplemented(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(ty.is_notimplemented(builder.db()))
    }
    fn class_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(match ty {
            Type::ClassLiteral(class) => Some(ClassType::NonGeneric(class)),
            Type::GenericAlias(generic) => Some(ClassType::Generic(generic)),
            Type::SubclassOf(subclass) => subclass
                .subclass_of()
                .into_class(builder.db(), builder.program_environment()),
            _ => None,
        })
    }
    fn is_typed_dict(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.is_typed_dict(builder.db()))
    }
    fn special_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
        kind: SpecialCall<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(match kind {
            SpecialCall::BuiltinType => builder.infer_builtins_type_call(call, None),
            SpecialCall::NewClass => builder.infer_new_class_call(call, None),
            SpecialCall::NamedTuple(kind) => {
                builder.infer_namedtuple_call_expression(call, None, kind)
            }
            SpecialCall::TypedDict(module) => {
                builder.infer_typeddict_call_expression(call, None, module)
            }
            SpecialCall::TypeForm => builder.infer_type_form_call_expression(call),
            SpecialCall::TypedDictConstructor(class) => {
                builder.infer_typed_dict_constructor(ty, class, call, tcx)
            }
            SpecialCall::NotImplemented => {
                if let Some(diagnostic) = builder.context.report_lint(&CALL_NON_CALLABLE, call) {
                    let mut diagnostic =
                        diagnostic.into_diagnostic("`NotImplemented` is not callable");
                    diagnostic.annotate(
                        builder
                            .context
                            .secondary(&*call.func)
                            .message("Did you mean `NotImplementedError`?"),
                    );
                    diagnostic.set_concise_message(
                        "`NotImplemented` is not callable - did you mean `NotImplementedError`?",
                    );
                    autofix_with_notimplementederror(&builder.context, &mut diagnostic, &call.func);
                }
                Type::unknown()
            }
        })
    }
    fn optional_special_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
        tcx: TypeContext<'db>,
        kind: OptionalSpecialCall,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match kind {
            OptionalSpecialCall::KeywordDict(initializer) => builder.infer_keyword_only_dict_call(
                &call.func,
                &call.arguments,
                initializer.then_some(call.into()),
                tcx,
            ),
            OptionalSpecialCall::Enum(base) => builder.infer_enum_call_expression(call, None, base),
        })
    }
    fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.expression_type(expression))
    }
    fn typed_dict_method(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
        typed_dict: TypedDictType<'db>,
        attribute: &ast::ExprAttribute,
        first: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(builder.local_typed_dict_method(
            call,
            typed_dict,
            &attribute.value,
            attribute.attr.id.as_str(),
            first,
        ))
    }
    fn metadata_checks(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
    ) -> Result<(), Self::Error> {
        builder.local_call_metadata(data);
        Ok(())
    }
    fn function_in_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(builder.local_function_in_scope(function))
    }
    fn record_called_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> Result<(), Self::Error> {
        builder.called_functions.insert(function);
        Ok(())
    }
    fn report_ineffective_final(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
    ) -> Result<(), Self::Error> {
        builder.local_report_ineffective_final(call);
        Ok(())
    }
    fn staticmethod_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(function.has_staticmethod_declaration(builder.db()))
    }
    fn bound_method_metadata(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
        method: BoundMethodType<'db>,
    ) -> Result<(), Self::Error> {
        builder.local_bound_method_metadata(call, method);
        Ok(())
    }
    fn static_method_metadata(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        call: &ast::ExprCall,
        function: FunctionType<'db>,
        attribute: &ast::ExprAttribute,
    ) -> Result<(), Self::Error> {
        builder.local_static_method_metadata(call, function, attribute);
        Ok(())
    }
    fn class_metadata(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        class: ClassType<'db>,
    ) -> Result<(), Self::Error> {
        crate::types::signatures::effects::legacy_inline(class_metadata::class_metadata_with(
            builder, data, class, self,
        ));
        Ok(())
    }
    fn match_bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<(Option<CallableRecursionGuard<'db>>, Bindings<'db>), Self::Error> {
        Ok((
            None,
            builder
                .bindings_for_call(data.callable_type)
                .match_parameters(builder.db(), builder.program_environment(), arguments),
        ))
    }
    fn report_implicit_calls(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        bindings: &Bindings<'db>,
    ) -> Result<(), Self::Error> {
        report_missing_implicit_constructor_call(
            &builder.context,
            data.callable_type,
            data.call,
            bindings,
        );
        Ok(())
    }
    fn failed_call(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        crate::types::signatures::effects::legacy_inline(completion::report_failed_with(
            builder,
            data,
            bindings,
            &crate::types::function::LegacyFunctionIdentityEffects,
        ));
        Ok(bindings.return_type(builder.db(), builder.program_environment()))
    }
    fn successful_checks(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        arguments: &CallArguments<'_, 'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        crate::types::signatures::effects::legacy_inline(completion::successful_checks_with(
            builder,
            data,
            arguments,
            bindings,
            &crate::types::function::LegacyFunctionIdentityEffects,
        ));
        Ok(())
    }
    fn receiver_constraints(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        arguments: &mut CallArguments<'_, 'db>,
    ) -> Result<(), Self::Error> {
        crate::types::signatures::effects::legacy_inline(completion::receiver_constraints_with(
            builder,
            data,
            arguments,
            &crate::types::function::LegacyFunctionIdentityEffects,
        ));
        Ok(())
    }
    fn refine_range(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        arguments: &CallArguments<'_, 'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        if let Some(ty) = builder.infer_builtin_range_instance_type(
            data.callable_type,
            &data.call.arguments,
            arguments,
        ) {
            bindings.set_constructor_instance_type_in_place(builder.db(), ty);
        }
        Ok(())
    }
    fn return_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(bindings.return_type(builder.db(), builder.program_environment()))
    }
    fn collection_return_matches(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        class: KnownClass,
    ) -> Result<bool, Self::Error> {
        Ok(ty
            .class_specialization(builder.db(), builder.program_environment())
            .is_some_and(|(literal, _)| literal.is_known(builder.db(), class)))
    }
    fn empty_collection(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        class: KnownClass,
        fallback: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder
            .infer_empty_list_or_set_constructor(class, data.call, data.tcx)
            .unwrap_or(fallback))
    }
    fn bind_type_guard(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        data: &CallData<'db, '_>,
        bindings: &Bindings<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(typeguard::bind_type_guard_return_type(
            builder.db(),
            builder.scope(),
            ty,
            bindings,
            &data.call.arguments,
        ))
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    fn local_collection_initializer(
        &self,
        call_expression: &ast::ExprCall,
        callable_type: Type<'db>,
    ) -> Option<KnownClass> {
        let ast::ExprCall {
            func, arguments, ..
        } = call_expression;
        // Semantic indexing recognizes only bare empty constructor calls. Confirm that the name
        // still resolves to the corresponding builtin before using later collection constraints.
        if arguments.is_empty()
            && self
                .index
                .try_expression(call_expression)
                .and_then(|expression| expression.assigned_to(self.db()))
                .is_some()
            && let Some(name) = func.as_name_expr()
            && let Some(known_class) = callable_type
                .as_class_literal()
                .and_then(|class| class.known(self.db()))
            && matches!(
                (name.id.as_str(), known_class),
                ("list", KnownClass::List) | ("set", KnownClass::Set) | ("dict", KnownClass::Dict)
            )
        {
            Some(known_class)
        } else {
            None
        }
    }

    fn local_typed_dict_method(
        &mut self,
        call_expression: &ast::ExprCall,
        typed_dict_ty: TypedDictType<'db>,
        value: &ast::Expr,
        method_name: &str,
        first_arg: &ast::Expr,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let arguments = &call_expression.arguments;
        let Some(key) = (match first_arg {
            ast::Expr::StringLiteral(ast::ExprStringLiteral {
                value: key_literal, ..
            }) => Some(key_literal.to_str()),
            _ => self
                .speculate_without_diagnostics()
                .get_or_infer_expression(first_arg, TypeContext::default())
                .as_string_literal()
                .map(|key_literal| key_literal.value(self.db())),
        }) else {
            return None;
        };
        let items = typed_dict_ty.items(self.db());
        let is_declared = items.contains_key(key);

        if let Some(field) = typed_dict_ty.item(self.db(), key) {
            // Key exists - check if it's a `pop()` on a required field
            if is_declared && method_name == "pop" && field.is_required() {
                report_cannot_pop_required_field_on_typed_dict(
                    &self.context,
                    first_arg.into(),
                    Type::TypedDict(typed_dict_ty),
                    key,
                );
                return Some(Type::unknown());
            }

            if !is_declared
                && method_name == "get"
                && arguments.keywords.is_empty()
                && matches!(arguments.args.len(), 1 | 2)
            {
                let default_ty = if let Some(default) = arguments.args.get(1) {
                    self.get_or_infer_expression(default, TypeContext::new(Some(field.declared_ty)))
                } else {
                    Type::none(db, env)
                };
                return Some(UnionType::from_two_elements(
                    db,
                    env,
                    field.declared_ty,
                    default_ty,
                ));
            }

            if !is_declared && field.is_read_only() {
                let mutation = match method_name {
                    "pop"
                        if arguments.keywords.is_empty()
                            && matches!(arguments.args.len(), 1 | 2) =>
                    {
                        Some(("pop", "from"))
                    }
                    "setdefault" if arguments.keywords.is_empty() && arguments.args.len() == 2 => {
                        Some(("set default for", "on"))
                    }
                    _ => None,
                };
                if let Some((action, preposition)) = mutation {
                    if let Some(builder) =
                        self.context.report_lint(&INVALID_ARGUMENT_TYPE, first_arg)
                    {
                        builder.into_diagnostic(format_args!(
                            "Cannot {action} read-only extra item \
                                    \"{key}\" {preposition} TypedDict `{}`",
                            Type::TypedDict(typed_dict_ty).display(db, env),
                        ));
                    }
                    return Some(Type::unknown());
                }
            }

            // Unknown literal keys are concrete extra items, so mutating operations can
            // use their extra-items type even when arbitrary `str` keys are unsafe.
            if !is_declared && !field.is_read_only() {
                match method_name {
                    "pop"
                        if arguments.keywords.is_empty()
                            && matches!(arguments.args.len(), 1 | 2) =>
                    {
                        return Some(arguments.args.get(1).map_or(field.declared_ty, |default| {
                            UnionType::from_two_elements(
                                db,
                                env,
                                field.declared_ty,
                                self.get_or_infer_expression(
                                    default,
                                    TypeContext::new(Some(field.declared_ty)),
                                ),
                            )
                        }));
                    }
                    "setdefault" if arguments.keywords.is_empty() && arguments.args.len() == 2 => {
                        let default = &arguments.args[1];
                        let default_ty = self.get_or_infer_expression(
                            default,
                            TypeContext::new(Some(field.declared_ty)),
                        );
                        TypedDictKeyAssignment {
                            context: &self.context,
                            typed_dict: typed_dict_ty,
                            full_object_ty: None,
                            key,
                            value_ty: default_ty,
                            typed_dict_node: value.into(),
                            key_node: first_arg.into(),
                            value_node: default.into(),
                            assignment_kind: TypedDictAssignmentKind::Constructor,
                            emit_diagnostic: true,
                        }
                        .validate();
                        return Some(field.declared_ty);
                    }
                    _ => {}
                }
            }
        } else if method_name != "get" {
            // Key not found, report error with suggestion and return early
            let key_ty = Type::string_literal(self.db(), key);
            report_invalid_key_on_typed_dict(
                &self.context,
                first_arg.into(),
                first_arg.into(),
                Type::TypedDict(typed_dict_ty),
                None,
                key_ty,
                items,
            );
            // Return `Unknown` to prevent the overload system from generating its own error
            return Some(Type::unknown());
        }

        None
    }

    fn local_call_metadata(&mut self, data: &CallData<'db, '_>) {
        let result = metadata_sync(self, data, CallFacts, &OrdinaryCallEffects);
        match result {
            Ok(()) => {}
            Err(error) => match error {},
        }
    }

    fn local_function_in_scope(&self, function: FunctionType<'db>) -> bool {
        // Read the definition only for functions from this file: looking it up reads the
        // semantic index and would otherwise add a cross-module AST dependency.
        function.file(self.db()) == self.file()
            && function.definition(self.db()).scope(self.db()) == self.scope()
    }

    fn local_report_ineffective_final(&self, call_expression: &ast::ExprCall) {
        // Type checkers cannot interpret `final()` as a call rather than a decorator.
        if let Some(builder) = self
            .context
            .report_lint(&INEFFECTIVE_FINAL, call_expression)
        {
            let mut diagnostic = builder.into_diagnostic(
                "Type checkers will not prevent subclassing \
                when `final()` is called as a function",
            );
            diagnostic.info("Use `@final` as a decorator on a class or method instead");
        }
    }

    fn local_bound_method_metadata(
        &self,
        call_expression: &ast::ExprCall,
        bound_method: BoundMethodType<'db>,
    ) {
        let db = self.db();
        if let Some(function) = bound_method.function(db)
            && let Some(class) = bound_method.self_instance(db).to_class_type(db)
            && bound_method.class_method(db)
            && function.as_abstract_method(db, class).is_some()
            && function.has_trivial_body(db)
        {
            report_call_to_abstract_method(&self.context, call_expression, function, "classmethod");
        }
    }

    fn local_static_method_metadata(
        &self,
        call_expression: &ast::ExprCall,
        function: FunctionType<'db>,
        attribute: &ast::ExprAttribute,
    ) {
        let db = self.db();
        let value_type = self.expression_type(&attribute.value);
        if let Some(class) = value_type.to_class_type(db)
            && function.as_abstract_method(db, class).is_some()
            && function.has_trivial_body(db)
        {
            report_call_to_abstract_method(
                &self.context,
                call_expression,
                function,
                "staticmethod",
            );
        }
    }

    fn local_receiver_collection_constraints(
        &mut self,
        data: &CallData<'db, '_>,
        call_arguments: &mut CallArguments<'_, 'db>,
        attribute: &ast::ExprAttribute,
        value_type: Type<'db>,
        collection_def: ty_python_core::definition::Definition<'db>,
    ) {
        let db = self.db();
        let env = self.program_environment();
        let call_expression_tcx = data.tcx;
        let arguments = &data.call.arguments;
        // Record the constraints for the receiver of a bound method call, if the receiver is an
        // unannotated collection initializer.
        if let Some((collection_literal, _)) = value_type.class_specialization(db, env) {
            let identity_instance =
                Type::instance(db, env, collection_literal.identity_specialization(db));
            let collection_generic_context = collection_literal.generic_context(db);
            let mut identity_bindings = self
                .infer_attribute_load_impl(attribute, identity_instance)
                .unwrap_or_else(|recovery_ty| recovery_ty)
                .inner_type()
                .bindings(db, env)
                .match_parameters(db, env, &call_arguments)
                // Perform inference against the type variables on the receiver's generic context.
                .with_generic_context(self.db(), collection_generic_context);

            let call_result = self
                .speculate_without_diagnostics()
                .infer_and_check_argument_types(
                    ArgumentsIter::from_ast(arguments),
                    call_arguments,
                    // TODO: The argument types have already been inferred and stored in `call_arguments`.
                    // However, `receiver` would have been inferred to be a collection with `Divergent`
                    // element types, meaning the type context for a given argument, by which the inferred
                    // type is keyed, may not be the same as the type context we get here. It is not immediately
                    // clear how to retrieve those types, and so we just re-infer the argument expressions
                    // for simplicity.
                    &mut |builder, (_, expr, tcx)| builder.infer_expression(expr, tcx),
                    &mut identity_bindings,
                    call_expression_tcx,
                );

            if call_result.is_ok() {
                let db = self.db();
                for call_specialization in identity_bindings
                    .iter_flat()
                    .flat_map(CallableBinding::matching_overloads)
                    .filter_map(|(_, identity_overload)| {
                        identity_overload.partial_specialization(db, env)
                    })
                {
                    // Record the constraints on the receiver's generic context formed by
                    // the arguments to this bound method call.
                    let Some(constraints) = self.collection_use_constraint_from_specialization(
                        identity_instance,
                        collection_generic_context,
                        call_specialization,
                    ) else {
                        continue;
                    };

                    self.collection_use_constraints
                        .entry(collection_def)
                        .or_default()
                        .insert(constraints);
                }
            }
        }
    }

    pub(super) fn local_validate_positional_splat(&mut self, value: &ast::Expr) {
        let db = self.db();
        let env = self.program_environment();
        let iterable_type = self.expression_type(value);
        if let Err(err) = iterable_type.try_iterate(db, env) {
            err.report_diagnostic(&self.context, iterable_type, value.into());
        }
    }

    pub(super) fn local_validate_keyword_splat(&mut self, value: &ast::Expr) {
        let db = self.db();
        let env = self.program_environment();
        let mapping_type = self.expression_type(value);
        if mapping_type.as_paramspec_typevar(db).is_some()
            || mapping_type.unpack_keys_and_items(db, env).is_some()
        {
            return;
        }
        let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, value) else {
            return;
        };
        builder
            .into_diagnostic("Argument expression after ** must be a mapping type")
            .set_primary_annotation_message(format_args!(
                "Found `{}`",
                mapping_type.display(db, env)
            ));
    }
}

fn report_missing_implicit_constructor_call<'db>(
    context: &InferContext<'db, '_>,
    callable_type: Type<'db>,
    call_expression: &ast::ExprCall,
    bindings: &Bindings<'db>,
) {
    let db = context.db();
    let env = context.program_environment();
    if bindings.has_implicit_dunder_new_is_possibly_unbound() {
        if let Some(builder) = context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, call_expression)
        {
            builder.into_diagnostic(format_args!(
                "Method `__new__` on type `{}` may be missing.",
                callable_type.display(db, env),
            ));
        }
    }

    if bindings.has_implicit_dunder_init_is_possibly_unbound() {
        if let Some(builder) = context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, call_expression)
        {
            builder.into_diagnostic(format_args!(
                "Method `__init__` on type `{}` may be missing.",
                callable_type.display(db, env),
            ));
        }
    }
}
