use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;
use ty_python_core::scope::FileScopeId;

use super::TypeInferenceBuilder;
use crate::types::generics::{GenericContext, bind_typevar};
use crate::types::infer::TypeExpressionFlags;
use crate::types::subscript::{LegacyGenericOrigin, SubscriptError, SubscriptErrorKind};
use crate::types::tuple::{Tuple, TupleSpec, VariableSegment};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, KnownClass, KnownInstanceType, NominalInstanceType, SpecialFormType,
    Type, any_over_type, todo_type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types::infer) enum LegacyGenericContextError<'db> {
    /// It's invalid to subscript `Generic` or `Protocol` with this type.
    InvalidArgument(Type<'db>),
    /// It's invalid to subscript `Generic` or `Protocol` with a variadic tuple type.
    /// We should emit a diagnostic for this, but we don't yet.
    VariadicTupleArguments,
    /// It's valid to subscribe `Generic` or `Protocol` with this type,
    /// but the type is not yet supported.
    NotYetSupported,
    /// A duplicate typevar was provided.
    DuplicateTypevar(&'db str),
    /// A `TypeVarTuple` was provided but not unpacked.
    ///
    /// The generic context is available when the argument is a bound `TypeVarTuple` and is used
    /// to avoid cascading errors during recovery.
    TypeVarTupleMustBeUnpacked(Option<GenericContext<'db>>),
}

impl<'db> LegacyGenericContextError<'db> {
    const fn into_type(self) -> Type<'db> {
        match self {
            Self::InvalidArgument(_)
            | Self::VariadicTupleArguments
            | Self::DuplicateTypevar(_)
            | Self::TypeVarTupleMustBeUnpacked(_) => Type::unknown(),
            Self::NotYetSupported => todo_type!("ParamSpecs and TypeVarTuples"),
        }
    }
}

pub(in crate::types::infer) struct LegacyArguments<'db> {
    scalar: Option<Type<'db>>,
    tuple: Option<&'db TupleSpec<'db>>,
    next: usize,
}

impl<'db> LegacyArguments<'db> {
    pub(in crate::types::infer) fn next(&mut self) -> Option<Type<'db>> {
        let Some(tuple) = self.tuple else {
            return self.scalar.take();
        };
        let result = match tuple {
            Tuple::Fixed(elements) => elements.elements_slice().get(self.next).copied(),
            Tuple::Variable(variable) => {
                let prefix = variable.prefix_elements();
                if let Some(ty) = prefix.get(self.next) {
                    Some(*ty)
                } else if self.next == prefix.len() {
                    variable.variable().typevartuple().map(Type::TypeVar)
                } else {
                    variable
                        .suffix_elements()
                        .get(self.next - prefix.len() - 1)
                        .copied()
                }
            }
        };
        if result.is_some() {
            self.next += 1;
        }
        result
    }
}

pub(in crate::types::infer) struct AstArguments<'expr> {
    arguments: &'expr [ast::Expr],
    next: usize,
}

impl<'expr> AstArguments<'expr> {
    pub(in crate::types::infer) fn next(&mut self) -> Option<&'expr ast::Expr> {
        let argument = self.arguments.get(self.next)?;
        self.next += 1;
        Some(argument)
    }
}

enum UnpackCandidate<'expr> {
    None,
    Starred(&'expr ast::Expr),
    Subscript(&'expr ast::ExprSubscript),
}

struct LegacyGenericFacts;

pub(super) struct OrdinaryLegacyGenericEffects<'db> {
    pub(super) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLegacyGenericEffects)]
    pub(in crate::types::infer) trait LegacyGenericEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn exact_tuple(&self, ty: Type<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, arguments: &mut LegacyArguments<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_validated(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn insert(&self, variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>, bound: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn bind(&self, db: &'db dyn Db, index: &'db SemanticIndex<'db>, scope: FileScopeId, context: Option<Definition<'db>>, typevar: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn bound_is_typevartuple(&self, bound: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn typevar_name(&self, typevar: TypeVarInstance<'db>) -> Result<&'db str, Self::Error>;
        #[operation(source)]
        async fn bound_name(&self, bound: BoundTypeVarInstance<'db>) -> Result<&'db str, Self::Error>;
        #[operation(child)]
        async fn nominal_is_typevartuple(&self, nominal: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn contains_typevartuple(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn context(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ast<'expr>(&self, arguments: &mut AstArguments<'expr>) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn invalid_unpack(&self, builder: &TypeInferenceBuilder<'db, '_>, argument: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn expression_type(&self, builder: &TypeInferenceBuilder<'db, '_>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn unpacked(&self, builder: &TypeInferenceBuilder<'db, '_>, argument: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn report(&self, builder: &TypeInferenceBuilder<'db, '_>, subscript: &ast::ExprSubscript, error: SubscriptError<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn validate(&self, builder: &TypeInferenceBuilder<'db, '_>, slice_ty: Type<'db>) -> Result<Result<GenericContext<'db>, LegacyGenericContextError<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl LegacyGenericFacts {
        fn arguments<'db>(&self, ty: Type<'db>, tuple: Option<&'db TupleSpec<'db>>) -> Result<LegacyArguments<'db>, LegacyGenericContextError<'db>> {
            if let Some(Tuple::Variable(variable)) = tuple
                && !matches!(variable.variable(), VariableSegment::TypeVarTuple(_))
            {
                return Err(LegacyGenericContextError::VariadicTupleArguments);
            }
            Ok(LegacyArguments { scalar: Some(ty), tuple, next: 0 })
        }
        fn ast_arguments<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> AstArguments<'expr> {
            let arguments = if let ast::Expr::Tuple(tuple) = subscript.slice.as_ref() {
                &*tuple.elts
            } else {
                std::slice::from_ref(subscript.slice.as_ref())
            };
            AstArguments { arguments, next: 0 }
        }
        fn unpack_candidate<'expr>(&self, argument: &'expr ast::Expr) -> UnpackCandidate<'expr> {
            match argument {
                ast::Expr::Starred(starred) => UnpackCandidate::Starred(&starred.value),
                ast::Expr::Subscript(subscript) => UnpackCandidate::Subscript(subscript),
                _ => UnpackCandidate::None,
            }
        }
        fn receiver<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> &'expr ast::Expr { &subscript.value }
        fn slice<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> &'expr ast::Expr { &subscript.slice }
        fn is_unpack<'db>(&self, ty: Type<'db>) -> bool { ty == Type::SpecialForm(SpecialFormType::Unpack) }
        fn tuple_is_unpacked<'db>(&self, tuple: Option<&'db TupleSpec<'db>>) -> bool {
            matches!(tuple, Some(Tuple::Variable(variable)) if variable.variable().typevartuple().is_some())
        }
        fn multiple_error<'db>(&self, origin: LegacyGenericOrigin) -> SubscriptError<'db> {
            SubscriptError::new(Type::unknown(), SubscriptErrorKind::MultipleTypeVarTuples { origin })
        }
        fn unpack_error<'db>(&self, origin: LegacyGenericOrigin) -> SubscriptError<'db> {
            SubscriptError::new(Type::unknown(), SubscriptErrorKind::InvalidLegacyGenericArgument { origin, argument_ty: Type::SpecialForm(SpecialFormType::Unpack) })
        }
        fn wrap<'db>(&self, origin: LegacyGenericOrigin, context: GenericContext<'db>) -> Type<'db> {
            Type::KnownInstance(match origin {
                LegacyGenericOrigin::Generic => KnownInstanceType::SubscriptedGeneric(context),
                LegacyGenericOrigin::Protocol => KnownInstanceType::SubscriptedProtocol(context),
            })
        }
        fn result<'db>(&self, origin: LegacyGenericOrigin, result: Result<GenericContext<'db>, LegacyGenericContextError<'db>>) -> Result<Type<'db>, SubscriptError<'db>> {
            match result {
                Ok(context) => Ok(self.wrap(origin, context)),
                Err(LegacyGenericContextError::InvalidArgument(argument_ty)) => Err(SubscriptError::new(Type::unknown(), SubscriptErrorKind::InvalidLegacyGenericArgument { origin, argument_ty })),
                Err(LegacyGenericContextError::DuplicateTypevar(typevar_name)) => Err(SubscriptError::new(Type::unknown(), SubscriptErrorKind::DuplicateTypevar { origin, typevar_name })),
                Err(LegacyGenericContextError::TypeVarTupleMustBeUnpacked(context)) => Err(SubscriptError::new(context.map_or(Type::unknown(), |context| self.wrap(origin, context)), SubscriptErrorKind::TypeVarTupleNotUnpacked { origin })),
                Err(error @ (LegacyGenericContextError::NotYetSupported | LegacyGenericContextError::VariadicTupleArguments)) => Ok(error.into_type()),
            }
        }
    }

    /// Parse the type arguments to `Generic[...]` or `Protocol[...]` and validate
    /// that each argument is a type variable.
    #[synchronous(legacy_generic_class_context_step_sync)]
    #[capabilities(effects = LegacyGenericEffects, facts = LegacyGenericFacts)]
    #[passive_values(LegacyGenericContextError::InvalidArgument, LegacyGenericContextError::DuplicateTypevar, LegacyGenericContextError::TypeVarTupleMustBeUnpacked, LegacyGenericContextError::NotYetSupported, Err)]
    #[expect(clippy::too_many_arguments)]
    async fn legacy_generic_class_context_step_with<'db, E: LegacyGenericEffects<'db>>(
        db: &'db dyn Db, env: &ProgramEnvironment<'db>, index: &'db SemanticIndex<'db>, file_scope_id: FileScopeId,
        typevar_binding_context: Option<Definition<'db>>, typevars: Type<'db>, facts: LegacyGenericFacts, effects: &E,
    ) -> Result<Result<GenericContext<'db>, LegacyGenericContextError<'db>>, E::Error> {
        effects.checkpoint().await?;
        let tuple = effects.exact_tuple(typevars).await?;
        let mut arguments = match facts.arguments(typevars, tuple) {
            Ok(arguments) => arguments,
            Err(error) => return Ok(Err(error)),
        };
        let mut validated_typevars = effects.new_validated().await?;
        #[cursor_loop]
        while let Some(argument_ty) = effects.next_type(&mut arguments).await? {
            if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = argument_ty {
                let Some(bound) = effects.bind(db, index, file_scope_id, typevar_binding_context, typevar).await? else {
                    return Ok(Err(LegacyGenericContextError::InvalidArgument(argument_ty)));
                };
                if effects.bound_is_typevartuple(bound).await? {
                    effects.insert(&mut validated_typevars, bound).await?;
                    let context = effects.context(db, env, validated_typevars).await?;
                    return Ok(Err(LegacyGenericContextError::TypeVarTupleMustBeUnpacked(Some(context))));
                }
                if !effects.insert(&mut validated_typevars, bound).await? {
                    return Ok(Err(LegacyGenericContextError::DuplicateTypevar(effects.typevar_name(typevar).await?)));
                }
            } else if let Type::TypeVar(bound) = argument_ty
                && effects.bound_is_typevartuple(bound).await?
            {
                if !effects.insert(&mut validated_typevars, bound).await? {
                    return Ok(Err(LegacyGenericContextError::DuplicateTypevar(effects.bound_name(bound).await?)));
                }
            } else if let Type::NominalInstance(instance) = argument_ty
                && effects.nominal_is_typevartuple(instance).await?
            {
                return Ok(Err(LegacyGenericContextError::TypeVarTupleMustBeUnpacked(None)));
            } else if effects.contains_typevartuple(env, argument_ty).await? {
                return Ok(Err(LegacyGenericContextError::NotYetSupported));
            } else {
                return Ok(Err(LegacyGenericContextError::InvalidArgument(argument_ty)));
            }
        }
        Ok(Ok(effects.context(db, env, validated_typevars).await?))
    }

    #[synchronous(is_unpacked_typevartuple_sync)]
    #[capabilities(effects = LegacyGenericEffects, facts = LegacyGenericFacts)]
    #[passive_values()]
    async fn is_unpacked_typevartuple_with<'db, E: LegacyGenericEffects<'db>>(
        builder: &TypeInferenceBuilder<'db, '_>, argument: &ast::Expr, facts: LegacyGenericFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        let operand = match facts.unpack_candidate(argument) {
            UnpackCandidate::None => return Ok(false),
            UnpackCandidate::Starred(operand) => operand,
            UnpackCandidate::Subscript(subscript) => {
                let receiver = effects.expression_type(builder, facts.receiver(subscript)).await?;
                if !facts.is_unpack(receiver) { return Ok(false); }
                facts.slice(subscript)
            }
        };
        let argument_ty = effects.expression_type(builder, argument).await?;
        let operand_ty = effects.expression_type(builder, operand).await?;
        if let Type::TypeVar(typevar) = argument_ty
            && effects.bound_is_typevartuple(typevar).await?
        {
            return Ok(true);
        }
        if facts.tuple_is_unpacked(effects.exact_tuple(argument_ty).await?) { return Ok(true); }
        if let Type::NominalInstance(instance) = operand_ty {
            return effects.nominal_is_typevartuple(instance).await;
        }
        Ok(false)
    }

    /// Validate the type arguments to `Generic[...]` or `Protocol[...]`, returning
    /// either its inferred type or a reported error's recovery type.
    #[synchronous(infer_legacy_generic_subscript_step_sync)]
    #[capabilities(effects = LegacyGenericEffects, facts = LegacyGenericFacts)]
    #[passive_values(Err)]
    async fn infer_legacy_generic_subscript_step_with<'db, E: LegacyGenericEffects<'db>>(
        builder: &TypeInferenceBuilder<'db, '_>, subscript: &ast::ExprSubscript, slice_ty: Type<'db>, origin: LegacyGenericOrigin,
        facts: LegacyGenericFacts, effects: &E,
    ) -> Result<Result<Type<'db>, Type<'db>>, E::Error> {
        let mut arguments = facts.ast_arguments(subscript);
        #[passive_state]
        let mut invalid_unpack = false;
        #[cursor_loop]
        while let Some(argument) = effects.next_ast(&mut arguments).await? {
            if effects.invalid_unpack(builder, argument).await? {
                invalid_unpack = true;
                break;
            }
        }
        // A tuple type can preserve only one variable segment, so count unpacked
        // `TypeVarTuple`s before the argument tuple is lowered to its type.
        let mut arguments = facts.ast_arguments(subscript);
        #[passive_state]
        let mut unpacked_seen = false;
        #[cursor_loop]
        while let Some(argument) = effects.next_ast(&mut arguments).await? {
            if effects.unpacked(builder, argument).await? {
                if unpacked_seen {
                    return Ok(Err(effects.report(builder, subscript, facts.multiple_error(origin)).await?));
                }
                unpacked_seen = true;
            }
        }
        if invalid_unpack {
            return Ok(Err(effects.report(builder, subscript, facts.unpack_error(origin)).await?));
        }
        let result = effects.validate(builder, slice_ty).await?;
        Ok(match facts.result(origin, result) {
            Ok(ty) => Ok(ty),
            Err(error) => Err(effects.report(builder, subscript, error).await?),
        })
    }
}

pub(in crate::types::infer) async fn legacy_generic_class_context_with<
    'db,
    E: LegacyGenericEffects<'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    index: &'db SemanticIndex<'db>,
    file_scope_id: FileScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    typevars: Type<'db>,
    effects: &E,
) -> Result<Result<GenericContext<'db>, LegacyGenericContextError<'db>>, E::Error> {
    legacy_generic_class_context_step_with(
        db,
        env,
        index,
        file_scope_id,
        typevar_binding_context,
        typevars,
        LegacyGenericFacts,
        effects,
    )
    .await
}

pub(in crate::types::infer) async fn infer_legacy_generic_subscript_with<
    'db,
    E: LegacyGenericEffects<'db>,
>(
    builder: &TypeInferenceBuilder<'db, '_>,
    subscript: &ast::ExprSubscript,
    slice_ty: Type<'db>,
    origin: LegacyGenericOrigin,
    effects: &E,
) -> Result<Result<Type<'db>, Type<'db>>, E::Error> {
    infer_legacy_generic_subscript_step_with(
        builder,
        subscript,
        slice_ty,
        origin,
        LegacyGenericFacts,
        effects,
    )
    .await
}

pub(super) fn infer_legacy_generic_subscript_sync<'db, E: SynchronousLegacyGenericEffects<'db>>(
    builder: &TypeInferenceBuilder<'db, '_>,
    subscript: &ast::ExprSubscript,
    slice_ty: Type<'db>,
    origin: LegacyGenericOrigin,
    effects: &E,
) -> Result<Result<Type<'db>, Type<'db>>, E::Error> {
    infer_legacy_generic_subscript_step_sync(
        builder,
        subscript,
        slice_ty,
        origin,
        LegacyGenericFacts,
        effects,
    )
}

pub(in crate::types::infer) async fn unpacked_typevartuple_with<
    'db,
    E: LegacyGenericEffects<'db>,
>(
    builder: &TypeInferenceBuilder<'db, '_>,
    argument: &ast::Expr,
    effects: &E,
) -> Result<bool, E::Error> {
    is_unpacked_typevartuple_with(builder, argument, LegacyGenericFacts, effects).await
}

impl<'db> SynchronousLegacyGenericEffects<'db> for OrdinaryLegacyGenericEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn exact_tuple(&self, ty: Type<'db>) -> Result<Option<&'db TupleSpec<'db>>, Infallible> {
        Ok(
            match ty
                .as_nominal_instance()
                .map(|instance| instance.visitor_kind())
            {
                Some(crate::types::instance::NominalVisitorKind::Tuple(tuple)) => {
                    Some(tuple.tuple(self.db))
                }
                _ => None,
            },
        )
    }
    fn next_type(
        &self,
        arguments: &mut LegacyArguments<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(arguments.next())
    }
    fn new_validated(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(FxOrderSet::default())
    }
    fn insert(
        &self,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(variables.insert(bound))
    }
    fn bind(
        &self,
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        scope: FileScopeId,
        context: Option<Definition<'db>>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(bind_typevar(db, index, scope, context, typevar))
    }
    fn bound_is_typevartuple(&self, bound: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(bound.is_typevartuple(self.db))
    }
    fn typevar_name(&self, typevar: TypeVarInstance<'db>) -> Result<&'db str, Infallible> {
        Ok(typevar.name(self.db))
    }
    fn bound_name(&self, bound: BoundTypeVarInstance<'db>) -> Result<&'db str, Infallible> {
        Ok(bound.name(self.db))
    }
    fn nominal_is_typevartuple(
        &self,
        nominal: NominalInstanceType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(matches!(
            nominal.known_class(self.db),
            Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple)
        ))
    }
    fn contains_typevartuple(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(any_over_type(
            self.db,
            env,
            ty,
            true,
            |inner_ty| match inner_ty {
                Type::NominalInstance(nominal) => matches!(
                    nominal.known_class(self.db),
                    Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple)
                ),
                _ => false,
            },
        ))
    }
    fn context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(GenericContext::from_typevar_instances(db, env, variables))
    }
    fn next_ast<'expr>(
        &self,
        arguments: &mut AstArguments<'expr>,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(arguments.next())
    }
    fn invalid_unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        argument: &ast::Expr,
    ) -> Result<bool, Infallible> {
        Ok(builder
            .type_expression_flags(argument)
            .contains(TypeExpressionFlags::INVALID_UNPACK))
    }
    fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.expression_type(expression))
    }
    fn unpacked(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        argument: &ast::Expr,
    ) -> Result<bool, Infallible> {
        is_unpacked_typevartuple_sync(builder, argument, LegacyGenericFacts, self)
    }
    fn report(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        subscript: &ast::ExprSubscript,
        error: SubscriptError<'db>,
    ) -> Result<Type<'db>, Infallible> {
        error.report_diagnostics(&builder.context, subscript);
        Ok(error.result_type())
    }
    fn validate(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        slice_ty: Type<'db>,
    ) -> Result<Result<GenericContext<'db>, LegacyGenericContextError<'db>>, Infallible> {
        legacy_generic_class_context_step_sync(
            self.db,
            builder.program_environment(),
            builder.index,
            builder.scope().file_scope_id(self.db),
            builder.typevar_binding_context,
            slice_ty,
            LegacyGenericFacts,
            self,
        )
    }
}
