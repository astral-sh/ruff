use std::convert::Infallible;

use itertools::{Either, Itertools};
use ruff_python_ast::{self as ast, ArgOrKeyword, ExprContext};
use ruff_text_size::Ranged;
use ty_module_resolver::file_to_module;

use super::TypeInferenceBuilder;
use crate::place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers};
use crate::types::call::CallErrorKind;
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, INVALID_KEY,
    INVALID_TYPE_ARGUMENTS, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE, POSSIBLY_MISSING_IMPLICIT_CALL,
    TypedDictDeleteErrorKind, report_cannot_delete_typed_dict_key,
    report_invalid_arguments_to_annotated, report_not_subscriptable,
};
use crate::types::generics::GenericContext;
use crate::types::infer::builder::annotation_expression::PEP613Policy;
use crate::types::infer::builder::{ArgExpr, ArgumentsIter, MultiInferenceGuard};
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::special_form::AliasSpec;
use crate::types::subscript::LegacyGenericOrigin;
use crate::types::typed_dict::{
    TypedDictAssignmentKind, TypedDictExtraItems, TypedDictKeyAssignment,
};
use crate::types::{
    CallArguments, CallDunderError, CallableBinding, ClassLiteral, CycleDetector, DisplaySettings,
    DynamicType, InternedType, KnownClass, KnownInstanceType, LintDiagnosticGuard,
    MemberLookupPolicy, Parameter, Parameters, SpecialFormType, StaticClassLiteral, Type,
    TypeAliasType, TypeAndQualifiers, TypeContext, UnionType, UnionTypeInstance,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::FileScopeId;

pub(super) mod legacy_generic;
pub(in crate::types::infer) mod specialization;

use legacy_generic::{OrdinaryLegacyGenericEffects, infer_legacy_generic_subscript_sync};

pub(in crate::types::infer) enum SubscriptStart<'db, 'expr> {
    Complete(Result<Type<'db>, Type<'db>>),
    Slice(SubscriptPending<'db, 'expr>),
    ClassSpecialization {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    },
}

pub(in crate::types::infer) struct SubscriptPending<'db, 'expr> {
    subscript: &'expr ast::ExprSubscript,
    value_ty: Type<'db>,
    assigned: Option<Type<'db>>,
    constraint_keys: Vec<(FileScopeId, ConstraintKey)>,
}

impl<'expr> SubscriptPending<'_, 'expr> {
    pub(super) fn slice(&self) -> &'expr ast::Expr {
        &self.subscript.slice
    }
}

#[derive(Clone, Copy)]
pub(super) struct OrdinarySubscriptEffects;
pub(in crate::types::infer) struct SubscriptFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSubscriptEffects)]
    pub(in crate::types::infer) trait SubscriptEffects<'db, 'ast> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn expected_keys(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn store_expected(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, expected: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn empty_constraints(&self) -> Result<Vec<(FileScopeId, ConstraintKey)>, Self::Error>;
        #[operation(local)]
        async fn place_expression(&self, subscript: &ast::ExprSubscript) -> Result<Option<PlaceExpr>, Self::Error>;
        #[operation(source)]
        async fn assigned_place(&self, builder: &TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, place: PlaceExpr) -> Result<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>), Self::Error>;
        #[operation(child)]
        async fn implicit_alias(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, value_ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn class_is_tuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn class_is_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn class_generic_context(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn tuple_class_specialization(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn type_class_specialization(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn receiver_tail(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, value_ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn expression_types(&self, builder: &TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, value_ty: Type<'db>, slice_ty: Type<'db>) -> Result<Result<Type<'db>, Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn narrow(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, ty: Type<'db>, constraints: &[(FileScopeId, ConstraintKey)]) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl SubscriptFacts {
        fn implicit_alias<'db>(&self, ty: Type<'db>) -> bool { ty.is_generic_alias() }
        fn unknown<'db>(&self, ty: Type<'db>) -> bool { ty.is_unknown() }
        fn slice<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> &'expr ast::Expr { &subscript.slice }
        pub(in crate::types::infer) fn legacy_origin<'db>(&self, ty: Type<'db>) -> Option<LegacyGenericOrigin> {
            match ty {
                Type::SpecialForm(SpecialFormType::Generic) => Some(LegacyGenericOrigin::Generic),
                Type::SpecialForm(SpecialFormType::Protocol) => Some(LegacyGenericOrigin::Protocol),
                _ => None,
            }
        }
        fn legacy<'db>(&self, ty: Type<'db>) -> bool { self.legacy_origin(ty).is_some() }
        fn assigned<'db>(&self, place: PlaceAndQualifiers<'db>) -> Option<Type<'db>> {
            match place.place {
                Place::Defined(DefinedPlace { ty, definedness: Definedness::AlwaysDefined, .. }) => Some(ty),
                _ => None,
            }
        }
        fn needs_expected_keys<'db>(&self, ty: Type<'db>) -> bool {
            matches!(ty, Type::TypedDict(_) | Type::Union(_) | Type::Intersection(_) | Type::TypeAlias(_) | Type::Recursive(_))
        }
        fn class<'db>(&self, ty: Type<'db>) -> Option<ClassLiteral<'db>> { ty.as_class_literal() }
        fn static_class<'db>(&self, class: ClassLiteral<'db>) -> Option<StaticClassLiteral<'db>> { class.as_static() }
    }

    #[synchronous(subscript_after_receiver_step_sync)]
    #[capabilities(effects = SubscriptEffects, facts = SubscriptFacts)]
    #[passive_values(SubscriptStart::Complete, SubscriptStart::Slice, SubscriptStart::ClassSpecialization, SubscriptPending)]
    async fn subscript_after_receiver_step_with<'db, 'ast, 'expr, E: SubscriptEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, facts: SubscriptFacts, effects: &E,
    ) -> Result<SubscriptStart<'db, 'expr>, E::Error> {
        effects.checkpoint().await?;
        // If we have an implicit type alias like `MyList = list[T]`, and if `MyList` is being
        // used in another implicit type alias like `Numbers = MyList[int]`, then we infer the
        // right hand side as a value expression, and need to handle the specialization here.
        if facts.implicit_alias(value_ty) {
            return Ok(SubscriptStart::Complete(Ok(effects.implicit_alias(builder, subscript, value_ty).await?)));
        }
        if facts.needs_expected_keys(value_ty)
            && let Some(expected) = effects.expected_keys(builder, value_ty).await?
        {
            effects.store_expected(builder, facts.slice(subscript), expected).await?;
        }
        let empty_constraints = effects.empty_constraints().await?;
        // If `value` is a valid reference, we attempt type narrowing by assignment.
        let (assigned, constraint_keys) = if !facts.unknown(value_ty)
            && let Some(place) = effects.place_expression(subscript).await?
        {
            let (place, keys) = effects.assigned_place(builder, subscript, place).await?;
            (facts.assigned(place), keys)
        } else {
            (None, empty_constraints)
        };
        if let Some(ty) = assigned {
            // Even if we can obtain the subscript type based on the assignments, we still perform default type inference
            // (to store the expression type and to report errors).
            return Ok(SubscriptStart::Slice(SubscriptPending { subscript, value_ty, assigned: Some(ty), constraint_keys }));
        }
        if let Some(class) = facts.class(value_ty) {
            // HACK ALERT: If we are subscripting a generic class, short-circuit the rest of the
            // subscript inference logic and treat this as an explicit specialization.
            // TODO: Move this logic into a custom callable, and update `find_name_in_mro` to return
            // this callable as the `__class_getitem__` method on `type`. That probably requires
            // updating all of the subscript logic below to use custom callables for all of the _other_
            // special cases, too.
            if effects.class_is_tuple(builder, class).await? {
                return Ok(SubscriptStart::Complete(Ok(effects.tuple_class_specialization(builder, subscript).await?)));
            } else if effects.class_is_type(builder, class).await? {
                return Ok(SubscriptStart::Complete(Ok(effects.type_class_specialization(builder, subscript).await?)));
            }
            if let Some(generic_context) = effects.class_generic_context(builder, class).await?
                && let Some(class) = facts.static_class(class)
            {
                return Ok(SubscriptStart::ClassSpecialization { subscript, value_ty, class, generic_context });
            }
        }
        if !facts.legacy(value_ty)
            && let Some(ty) = effects.receiver_tail(builder, subscript, value_ty).await?
        {
            return Ok(SubscriptStart::Complete(Ok(ty)));
        }
        Ok(SubscriptStart::Slice(SubscriptPending { subscript, value_ty, assigned: None, constraint_keys }))
    }

    #[synchronous(subscript_after_slice_sync)]
    #[capabilities(effects = SubscriptEffects)]
    #[passive_values(Err)]
    pub(in crate::types::infer) async fn subscript_after_slice_with<'db, 'ast, 'expr, E: SubscriptEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, pending: SubscriptPending<'db, 'expr>, slice_ty: Type<'db>, effects: &E,
    ) -> Result<Result<Type<'db>, Type<'db>>, E::Error> {
        let result = effects.expression_types(builder, pending.subscript, pending.value_ty, slice_ty).await?;
        if let Some(ty) = pending.assigned {
            return Ok(match result { Ok(_) => Ok(ty), Err(_) => Err(ty) });
        }
        Ok(match result {
            Ok(ty) => Ok(effects.narrow(builder, pending.subscript, ty, &pending.constraint_keys).await?),
            Err(ty) => Err(effects.narrow(builder, pending.subscript, ty, &pending.constraint_keys).await?),
        })
    }
}

pub(in crate::types::infer) async fn subscript_after_receiver_with<
    'db,
    'ast,
    'expr,
    E: SubscriptEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    subscript: &'expr ast::ExprSubscript,
    value_ty: Type<'db>,
    effects: &E,
) -> Result<SubscriptStart<'db, 'expr>, E::Error> {
    subscript_after_receiver_step_with(builder, subscript, value_ty, SubscriptFacts, effects).await
}

pub(super) fn subscript_after_receiver_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousSubscriptEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    subscript: &'expr ast::ExprSubscript,
    value_ty: Type<'db>,
    effects: &E,
) -> Result<SubscriptStart<'db, 'expr>, E::Error> {
    subscript_after_receiver_step_sync(builder, subscript, value_ty, SubscriptFacts, effects)
}

impl<'db, 'ast> SynchronousSubscriptEffects<'db, 'ast> for OrdinarySubscriptEffects {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn expected_keys(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(builder.typed_dict_key_expected_type_impl(ty))
    }
    fn store_expected(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        expected: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.store_expected_type(slice, expected);
        Ok(())
    }
    fn empty_constraints(&self) -> Result<Vec<(FileScopeId, ConstraintKey)>, Infallible> {
        Ok(Vec::new())
    }
    fn place_expression(
        &self,
        subscript: &ast::ExprSubscript,
    ) -> Result<Option<PlaceExpr>, Infallible> {
        Ok(PlaceExpr::try_from_expr(subscript))
    }
    fn assigned_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        place: PlaceExpr,
    ) -> Result<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>), Infallible> {
        Ok(builder.infer_place_load(place, ast::ExprRef::Subscript(subscript)))
    }
    fn implicit_alias(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_explicit_type_alias_specialization(subscript, value_ty, false))
    }
    fn receiver_tail(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(builder.infer_subscript_special_receiver(value_ty, subscript))
    }
    fn class_is_tuple(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(class.is_tuple(builder.db()))
    }
    fn class_is_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(class.is_known(builder.db(), KnownClass::Type))
    }
    fn class_generic_context(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(class.generic_context(builder.db()))
    }
    fn tuple_class_specialization(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_tuple_type_expression(subscript, super::local::tuple_annotation::ResultMode::Class))
    }
    fn type_class_specialization(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
    ) -> Result<Type<'db>, Infallible> {
        let argument_ty = builder.infer_type_expression(&subscript.slice);
        Ok(Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
            InternedType::new(builder.db(), argument_ty),
        )))
    }
    fn expression_types(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
    ) -> Result<Result<Type<'db>, Type<'db>>, Infallible> {
        Ok(builder.infer_subscript_expression_types(
            subscript,
            value_ty,
            slice_ty,
            ExprContext::Load,
        ))
    }
    fn narrow(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.narrow_expr_with_applicable_constraints(subscript, ty, constraints))
    }
}

/// Given a string literal or a union of string literals, return an iterator over the contained
/// strings, or `None` if the type is neither.
fn string_literal_values<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<impl Iterator<Item = &'db str> + 'db> {
    if let Some(literal) = ty.as_string_literal() {
        Some(Either::Left(std::iter::once(literal.value(db))))
    } else {
        let elements = ty.as_union()?.elements(db);
        elements
            .iter()
            .all(|ty| ty.as_string_literal().is_some())
            .then(|| {
                Either::Right(
                    elements
                        .iter()
                        .filter_map(|ty| ty.as_string_literal().map(|lit| lit.value(db))),
                )
            })
    }
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(super) fn typed_dict_key_expected_type(&self, ty: Type<'db>) -> Option<Type<'db>> {
        if SubscriptFacts.needs_expected_keys(ty) {
            self.typed_dict_key_expected_type_impl(ty)
        } else {
            None
        }
    }

    fn typed_dict_key_expected_type_impl(&self, ty: Type<'db>) -> Option<Type<'db>> {
        struct TypedDictKeyExpectedType;
        type TypedDictKeyExpectedTypeVisitor<'db> =
            CycleDetector<'db, TypedDictKeyExpectedType, Type<'db>, Option<Type<'db>>, 3>;

        fn imp<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            visitor: &TypedDictKeyExpectedTypeVisitor<'db>,
        ) -> Option<Type<'db>> {
            match ty {
                Type::TypedDict(typed_dict) => {
                    if typed_dict.explicit_extra_items(db).is_some() {
                        return Some(KnownClass::Str.to_instance(db, env));
                    }
                    let keys = typed_dict
                        .items(db)
                        .keys()
                        .map(|key| Type::string_literal(db, key))
                        .collect_vec();
                    (!keys.is_empty()).then(|| UnionType::from_elements(db, env, keys))
                }
                Type::Union(union) => {
                    let keys = union
                        .elements(db)
                        .iter()
                        .filter_map(|element| imp(db, env, *element, visitor))
                        .collect_vec();
                    (!keys.is_empty()).then(|| UnionType::from_elements(db, env, keys))
                }
                Type::Intersection(intersection) => {
                    let keys = intersection
                        .positive(db)
                        .iter()
                        .filter_map(|element| imp(db, env, *element, visitor))
                        .collect_vec();
                    (!keys.is_empty()).then(|| UnionType::from_elements(db, env, keys))
                }
                Type::TypeAlias(_) | Type::Recursive(_) => {
                    visitor.visit(db, ty, || imp(db, env, ty.resolve_type_alias(db), visitor))
                }
                _ => None,
            }
        }
        let db = self.db();

        imp(
            db,
            self.program_environment(),
            ty,
            &TypedDictKeyExpectedTypeVisitor::default(),
        )
    }

    fn store_typed_dict_key_expected_type(&mut self, slice: &ast::Expr, value_ty: Type<'db>) {
        if let Some(expected_key_ty) = self.typed_dict_key_expected_type(value_ty) {
            self.store_expected_type(slice, expected_key_ty);
        }
    }

    pub(super) fn infer_subscript_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
    ) -> Type<'db> {
        let ast::ExprSubscript {
            value,
            slice,
            range: _,
            node_index: _,
            ctx,
        } = subscript;

        match ctx {
            ExprContext::Load => self
                .infer_subscript_load(subscript)
                .unwrap_or_else(|recovery_ty| recovery_ty),
            ExprContext::Store => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                self.store_typed_dict_key_expected_type(slice, value_ty);
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let _ = self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                Type::Never
            }
            ExprContext::Del => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                self.store_typed_dict_key_expected_type(slice, value_ty);
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                self.validate_subscript_deletion(subscript, value_ty, slice_ty);
                Type::Never
            }
            ExprContext::Invalid => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let _ = self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                Type::unknown()
            }
        }
    }

    /// Infer a subscript load, returning its inferred type when the subscription succeeds.
    ///
    /// If the subscription fails, report the error and return the type that should be used to
    /// continue inference. This recovery type may be `Unknown` or, for example, the return type of
    /// `__getitem__` when its arguments are invalid. Keeping it separate from a successful result
    /// lets augmented assignments check their right-hand side without attempting a failed store.
    pub(super) fn infer_subscript_load(
        &mut self,
        subscript: &ast::ExprSubscript,
    ) -> Result<Type<'db>, Type<'db>> {
        let value_ty = self.infer_expression(&subscript.value, TypeContext::default());

        let start = match subscript_after_receiver_sync(
            self,
            subscript,
            value_ty,
            &OrdinarySubscriptEffects,
        ) {
            Ok(start) => start,
            Err(never) => match never {},
        };
        match start {
            SubscriptStart::Complete(result) => result,
            SubscriptStart::ClassSpecialization {
                subscript,
                value_ty,
                class,
                generic_context,
            } => Ok(self.infer_explicit_class_specialization(
                subscript,
                value_ty,
                class,
                generic_context,
            )),
            SubscriptStart::Slice(pending) => {
                let slice_ty = self.infer_expression(pending.slice(), TypeContext::default());
                match subscript_after_slice_sync(self, pending, slice_ty, &OrdinarySubscriptEffects)
                {
                    Ok(result) => result,
                    Err(never) => match never {},
                }
            }
        }
    }

    fn infer_subscript_special_receiver(
        &mut self,
        value_ty: Type<'db>,
        subscript: &ast::ExprSubscript,
    ) -> Option<Type<'db>> {
        let env = self.program_environment();
        let db = self.db();
        let slice = &subscript.slice;

        match value_ty {
            Type::ClassLiteral(_) => {}
            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias)) => {
                if let Some(generic_context) = type_alias.generic_context(db) {
                    return Some(self.infer_explicit_type_alias_type_specialization(
                        subscript,
                        value_ty,
                        type_alias,
                        generic_context,
                    ));
                }
            }
            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::Tuple => {
                    return Some(self.infer_tuple_type_expression(subscript, super::local::tuple_annotation::ResultMode::Class));
                }
                SpecialFormType::Literal => match self.infer_literal_parameter_type(slice) {
                    Ok(result) => {
                        return Some(Type::KnownInstance(KnownInstanceType::Literal(
                            InternedType::new(db, result),
                        )));
                    }
                    Err(nodes) => {
                        for node in nodes {
                            let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, node)
                            else {
                                continue;
                            };
                            builder.into_diagnostic(
                                "Type arguments for `Literal` must be `None`, \
                                a literal value (int, bool, str, or bytes), \
                                or an enum member",
                            );
                        }
                        return Some(Type::unknown());
                    }
                },
                SpecialFormType::Annotated => {
                    return Some(
                        self.parse_subscription_of_annotated_special_form(
                            subscript,
                            AnnotatedExprContext::TypeExpression,
                        )
                        .inner_type(),
                    );
                }
                SpecialFormType::Optional => {
                    if matches!(**slice, ast::Expr::Tuple(_))
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                    {
                        builder.into_diagnostic(format_args!(
                            "`typing.Optional` requires exactly one argument"
                        ));
                    }

                    let ty = self.infer_type_expression(slice);

                    // `Optional[None]` is equivalent to `None`:
                    if ty.is_none(db) {
                        return Some(ty);
                    }
                    return Some(Type::KnownInstance(KnownInstanceType::UnionType(
                        UnionTypeInstance::new(
                            db,
                            None,
                            Ok(UnionType::from_two_elements(
                                db,
                                env,
                                ty,
                                Type::none(db, env),
                            )),
                        ),
                    )));
                }
                SpecialFormType::Union => match **slice {
                    ast::Expr::Tuple(ref tuple) => {
                        let elements = tuple.iter().map(|elt| self.infer_type_expression(elt));

                        let union_type = Type::KnownInstance(KnownInstanceType::UnionType(
                            UnionTypeInstance::new(
                                db,
                                None,
                                Ok(UnionType::from_elements(db, env, elements)),
                            ),
                        ));

                        if tuple.is_empty()
                            && let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                        {
                            builder.into_diagnostic(
                                "`typing.Union` requires at least one type argument",
                            );
                        }

                        return Some(union_type);
                    }
                    _ => {
                        return Some(self.infer_expression(slice, TypeContext::default()));
                    }
                },
                SpecialFormType::Type => {
                    // Similar to the branch above that handles `type[…]`, handle `typing.Type[…]`
                    let argument_ty = self.infer_type_expression(slice);
                    return Some(Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                        InternedType::new(db, argument_ty),
                    )));
                }
                SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => {
                    let callable = self
                        .infer_callable_type(subscript)
                        .as_callable()
                        .expect("always returns Type::Callable");

                    return Some(Type::KnownInstance(KnownInstanceType::Callable(callable)));
                }
                SpecialFormType::Unpack => {
                    self.store_type_expression_flags(
                        ast::ExprRef::from(subscript),
                        TypeExpressionFlags::UNPACK,
                    );

                    let previously_in_unpack_type_argument = self
                        .context
                        .inference_flags
                        .replace(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, true);
                    let inner_ty = self.infer_type_expression(slice);
                    self.context.inference_flags.set(
                        InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
                        previously_in_unpack_type_argument,
                    );

                    return Some(
                        if matches!(
                            inner_ty,
                            Type::TypeVar(typevar) if typevar.is_typevartuple(db)
                        ) || inner_ty.exact_tuple_instance_spec(db).is_some()
                        {
                            inner_ty
                        } else {
                            self.store_type_expression_flags(
                                ast::ExprRef::from(subscript),
                                TypeExpressionFlags::INVALID_UNPACK,
                            );
                            Type::unknown()
                        },
                    );
                }
                SpecialFormType::LegacyStdlibAlias(alias) => {
                    let AliasSpec {
                        class,
                        expected_argument_number,
                    } = alias.alias_spec();

                    let args = if let ast::Expr::Tuple(t) = &**slice {
                        &*t.elts
                    } else {
                        std::slice::from_ref(&**slice)
                    };

                    if args.len() != expected_argument_number
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                    {
                        let noun = if expected_argument_number == 1 {
                            "argument"
                        } else {
                            "arguments"
                        };
                        builder.into_diagnostic(format_args!(
                            "`typing.{name}` requires exactly \
                                {expected_argument_number} {noun}, got {got}",
                            name = special_form.name(),
                            got = args.len()
                        ));
                    }

                    let arg_types: Vec<_> = args
                        .iter()
                        .map(|arg| self.infer_type_expression(arg))
                        .collect();

                    return Some(
                        class
                            .to_specialized_class_type(db, env, arg_types)
                            .map(Type::from)
                            .unwrap_or_else(Type::unknown),
                    );
                }
                _ => {}
            },

            Type::KnownInstance(
                KnownInstanceType::UnionType(_)
                | KnownInstanceType::Annotated(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::TypeGenericAlias(_),
            ) => {
                return Some(
                    self.infer_explicit_type_alias_specialization(subscript, value_ty, false),
                );
            }
            Type::Dynamic(DynamicType::Unknown) => {
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    db,
                    env,
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(db, env, variables);
                return Some(Type::Dynamic(DynamicType::UnknownGeneric(generic_context)));
            }
            _ => {}
        }

        None
    }

    pub(super) fn infer_explicit_class_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    ) -> Type<'db> {
        super::local::class_specialization(
            self,
            subscript,
            value_ty,
            generic_class,
            generic_context,
        )
    }

    pub(super) fn infer_explicit_type_alias_type_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_type_alias: TypeAliasType<'db>,
        generic_context: GenericContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        if generic_type_alias.specialization(db).is_some() {
            if !self.in_string_annotation() {
                self.infer_expression(&subscript.slice, TypeContext::default());
            }
            if let Some(builder) = self.context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                let mut diagnostic =
                    builder.into_diagnostic("Cannot specialize non-generic type alias");
                diagnostic.set_primary_annotation_message("Double specialization is not allowed");
            }
            return Type::unknown();
        }

        let specialize = &|types: &[Option<Type<'db>>]| {
            let type_alias = generic_type_alias.apply_specialization(db, |_| {
                generic_context.specialize_partial(db, types.iter().copied())
            });

            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias))
        };

        self.infer_explicit_callable_specialization(
            subscript,
            value_ty,
            generic_context,
            specialize,
        )
    }

    pub(super) fn infer_explicit_callable_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_context: GenericContext<'db>,
        specialize: &dyn Fn(&[Option<Type<'db>>]) -> Type<'db>,
    ) -> Type<'db> {
        super::local::specialization(self, subscript, value_ty, generic_context, specialize)
    }

    /// Infer the type of the expression that represents an explicit specialization of a
    /// `ParamSpec` type variable.
    fn infer_paramspec_explicit_specialization_value(
        &mut self,
        expr: &ast::Expr,
        exactly_one_paramspec: bool,
    ) -> Result<Type<'db>, ()> {
        let db = self.db();

        match expr {
            ast::Expr::EllipsisLiteral(_) => {
                return Ok(Type::paramspec_value_callable(
                    db,
                    Parameters::gradual_form(),
                ));
            }

            ast::Expr::Tuple(_) if !exactly_one_paramspec => {
                // Tuple expression is only allowed when the generic context contains only one
                // `ParamSpec` type variable and no other type variables.
            }

            ast::Expr::Tuple(ast::ExprTuple { elts, .. })
            | ast::Expr::List(ast::ExprList { elts, .. }) => {
                let mut parameter_types = Vec::with_capacity(elts.len());

                // Whether to infer `Todo` for the parameters
                let mut return_todo = false;

                let previously_allowed_paramspec = self
                    .context
                    .inference_flags
                    .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
                for param in elts {
                    let param_type = self.infer_type_expression(param);
                    // This is similar to what we currently do for inferring tuple type expression.
                    // We currently infer `Todo` for the parameters to avoid invalid diagnostics
                    // when trying to check for assignability or any other relation. For example,
                    // `*tuple[int, str]`, `Unpack[]`, etc. are not yet supported.
                    return_todo |= param_type.is_todo()
                        && matches!(param, ast::Expr::Starred(_) | ast::Expr::Subscript(_));
                    parameter_types.push(param_type);
                }
                self.context.inference_flags.set(
                    InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                    previously_allowed_paramspec,
                );

                let parameters = if return_todo {
                    // TODO: `Unpack`
                    Parameters::todo()
                } else {
                    Parameters::from_annotation(
                        db,
                        parameter_types.iter().map(|param_type| {
                            Parameter::positional_only(None).with_annotated_type(*param_type)
                        }),
                    )
                };

                return Ok(Type::paramspec_value_callable(db, parameters));
            }

            ast::Expr::Subscript(subscript) => {
                let value_ty = self.infer_expression(&subscript.value, TypeContext::default());

                if matches!(value_ty, Type::SpecialForm(SpecialFormType::Concatenate)) {
                    return Ok(Type::paramspec_value_callable(
                        db,
                        self.infer_concatenate_special_form(subscript),
                    ));
                }

                // Non-Concatenate subscript: fall back to todo
                return Ok(Type::paramspec_value_callable(db, Parameters::todo()));
            }

            ast::Expr::Name(name) => {
                if name.is_invalid() {
                    return Err(());
                }

                let previous_concatenate_context = self
                    .context
                    .inference_flags
                    .replace(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT, true);
                let param_type = self.infer_type_expression(expr);
                self.context.inference_flags.set(
                    InferenceFlags::IN_VALID_CONCATENATE_CONTEXT,
                    previous_concatenate_context,
                );

                match param_type {
                    Type::TypeVar(typevar) if typevar.is_paramspec(db) => {
                        return Ok(param_type);
                    }

                    Type::KnownInstance(KnownInstanceType::TypeVar(typevar))
                        if typevar.is_paramspec(db) =>
                    {
                        if let Some(diagnostic_builder) =
                            self.context.report_lint(&INVALID_TYPE_ARGUMENTS, expr)
                        {
                            diagnostic_builder.into_diagnostic(format_args!(
                                "ParamSpec `{}` is unbound",
                                typevar.name(db)
                            ));
                        }
                        return Err(());
                    }

                    // This is to handle the following case:
                    //
                    // ```python
                    // from typing import ParamSpec
                    //
                    // class Foo[**P]: ...
                    //
                    // Foo[ParamSpec]  # P: (ParamSpec, /)
                    // ```
                    Type::NominalInstance(nominal)
                        if nominal.has_known_class(db, KnownClass::ParamSpec) =>
                    {
                        return Ok(Type::paramspec_value_callable(
                            db,
                            Parameters::from_annotation(
                                db,
                                [
                                    Parameter::positional_only(None)
                                        .with_annotated_type(param_type),
                                ],
                            ),
                        ));
                    }

                    _ if exactly_one_paramspec => {
                        // Square brackets are optional when `ParamSpec` is the only type variable
                        // being specialized. This means that a single name expression represents a
                        // parameter list with a single parameter. For example,
                        //
                        // ```python
                        // class OnlyParamSpec[**P]: ...
                        //
                        // OnlyParamSpec[int]  # P: (int, /)
                        // ```
                        let parameters =
                            if param_type.is_todo() {
                                Parameters::todo()
                            } else if param_type.is_dynamic() && param_type != Type::any() {
                                // If we ended up with an `Unknown` type here, it almost certainly means
                                // that we already emitted an error elsewhere. Fallback to the more lenient
                                // type.
                                Parameters::unknown()
                            } else {
                                Parameters::from_annotation(
                                    db,
                                    [Parameter::positional_only(None)
                                        .with_annotated_type(param_type)],
                                )
                            };
                        return Ok(Type::paramspec_value_callable(db, parameters));
                    }

                    // This is specifically to handle a case where there are more than one type
                    // variables and at least one of them is a `ParamSpec` which is specialized
                    // using `typing.Any`. This isn't explicitly allowed in the spec, but both mypy
                    // and Pyright allows this and the ecosystem report suggested there are usages
                    // of this in the wild e.g., `staticmethod[Any, Any]`. For example,
                    //
                    // ```python
                    // class Foo[**P, T]: ...
                    //
                    // Foo[Any, int]  # P: (Any, /), T: int
                    // ```
                    Type::Dynamic(DynamicType::Any) => {
                        return Ok(Type::paramspec_value_callable(
                            db,
                            Parameters::gradual_form(),
                        ));
                    }

                    // If we ended up with an `Unknown` type here, it almost certainly means
                    // that we already emitted an error elsewhere
                    Type::Dynamic(_) => {
                        return Ok(Type::paramspec_value_callable(db, Parameters::unknown()));
                    }

                    _ => {}
                }
            }

            _ => {}
        }

        if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, expr) {
            builder.into_diagnostic(
                "Type argument for `ParamSpec` must be either \
                    a list of types, `ParamSpec`, `Concatenate`, or `...`",
            );
        }

        Err(())
    }

    /// Infer a subscription and report failures while preserving their recovery types.
    pub(super) fn infer_subscript_expression_types(
        &self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
        expr_context: ExprContext,
    ) -> Result<Type<'db>, Type<'db>> {
        let env = self.program_environment();
        let db = self.db();

        if let Some(origin) = SubscriptFacts.legacy_origin(value_ty) {
            return match infer_legacy_generic_subscript_sync(
                self,
                subscript,
                slice_ty,
                origin,
                &OrdinaryLegacyGenericEffects { db },
            ) {
                Ok(result) => result,
                Err(never) => match never {},
            };
        }

        // Special typing forms for which subscriptions are context-dependent are parsed here,
        // outside of `Type::subscript`, which is a pure function that doesn't depend on the
        // semantic index or any context-dependent state.
        let subscript_result = match value_ty {
            Type::SpecialForm(SpecialFormType::Concatenate) => {
                // TODO: Add proper support for `Concatenate`
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    db,
                    env,
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(db, env, variables);
                Ok(Type::Dynamic(DynamicType::UnknownGeneric(generic_context)))
            }
            _ => value_ty.subscript(db, env, slice_ty, expr_context),
        };

        subscript_result.map_err(|error| {
            error.report_diagnostics(&self.context, subscript);
            error.result_type()
        })
    }

    pub(super) fn infer_slice_expression(&mut self, slice: &ast::ExprSlice) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprSlice {
            range: _,
            node_index: _,
            lower,
            upper,
            step,
        } = slice;

        let ty_lower = self.infer_optional_expression(lower.as_deref(), TypeContext::default());
        let ty_upper = self.infer_optional_expression(upper.as_deref(), TypeContext::default());
        let ty_step = self.infer_optional_expression(step.as_deref(), TypeContext::default());

        KnownClass::Slice.to_specialized_instance(
            db,
            env,
            &[
                ty_lower.unwrap_or_else(|| Type::none(db, env)),
                ty_upper.unwrap_or_else(|| Type::none(db, env)),
                ty_step.unwrap_or_else(|| Type::none(db, env)),
            ],
        )
    }

    /// Validate a subscript assignment of the form `object[key] = rhs_value`.
    pub(super) fn validate_subscript_assignment(
        &mut self,
        target: &ast::ExprSubscript,
        rhs_value: &ast::Expr,
        object_ty: Type<'db>,
        infer_slice_ty: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
        infer_rhs_value: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
    ) -> bool {
        let env = self.program_environment();
        let ast::ExprSubscript {
            range: _,
            node_index: _,
            value: object,
            slice,
            ctx: _,
        } = target;

        let db = self.db();

        self.store_typed_dict_key_expected_type(slice, object_ty);

        let is_valid_assignment = self.validate_subscript_assignment_impl(
            target,
            None,
            object_ty,
            infer_slice_ty,
            rhs_value,
            infer_rhs_value,
            true,
        );

        // Record the constraints for the object of the subscript assignment, if the object is an
        // unannotated collection initializer.
        if is_valid_assignment
            && let Some(collection_def) = self.index.unannotated_collection_initializer(object)
            && let Some((class_literal, _)) = object_ty.class_specialization(db, env)
        {
            let identity_instance =
                Type::instance(db, env, class_literal.identity_specialization(db));
            let collection_generic_context = class_literal.generic_context(db);

            let ast_arguments = [
                ArgOrKeyword::Arg(&target.slice),
                ArgOrKeyword::Arg(rhs_value),
            ];

            let mut call_arguments = CallArguments::positional([Type::unknown(), Type::unknown()]);

            if let Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                ..
            }) = identity_instance
                .member_lookup_with_policy(
                    db,
                    env,
                    "__setitem__",
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
            {
                let mut identity_bindings = dunder_callable
                    .bindings(db, env)
                    .match_parameters(db, env, &call_arguments)
                    // Perform inference against the type variables on the receiver's generic context.
                    .with_generic_context(db, collection_generic_context);

                let call_result = self
                    .speculate_without_diagnostics()
                    .infer_and_check_argument_types(
                        ArgumentsIter::synthesized(&ast_arguments),
                        &mut call_arguments,
                        &mut |builder, (_, expr, tcx)| {
                            // TODO: The argument types have already been inferred and stored in `call_arguments`.
                            // However, `object` would have been inferred to a be a collection with `Divergent`
                            // element types, meaning the type context for a given argument, by which the inferred
                            // type is keyed, may not be the same as the type context we get here. It is not immediately
                            // clear how to retrieve those types, and so we just re-infer the argument expressions
                            // for simplicity.
                            builder.infer_maybe_standalone_expression(expr, tcx)
                        },
                        &mut identity_bindings,
                        TypeContext::default(),
                    );

                if call_result.is_ok() && boundness == Definedness::AlwaysDefined {
                    for call_specialization in identity_bindings
                        .iter_flat()
                        .flat_map(CallableBinding::matching_overloads)
                        .filter_map(|(_, identity_overload)| {
                            identity_overload.merged_specialization(db)
                        })
                    {
                        // Record the constraints on the receiver's generic context formed by
                        // the arguments to this dunder call.
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

        is_valid_assignment
    }

    #[expect(clippy::too_many_arguments)]
    fn validate_subscript_assignment_impl(
        &mut self,
        target: &ast::ExprSubscript,
        full_object_ty: Option<Type<'db>>,
        object_ty: Type<'db>,
        infer_slice_ty: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
        rhs_value_node: &ast::Expr,
        infer_rhs_value: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
        emit_diagnostic: bool,
    ) -> bool {
        let env = self.program_environment();
        let db = self.db();

        let attach_original_type_info = |diagnostic: &mut LintDiagnosticGuard| {
            if let Some(full_object_ty) = full_object_ty {
                diagnostic.info(format_args!(
                    "The full type of the subscripted object is `{}`",
                    full_object_ty.display(db, env)
                ));
            }
        };

        // Aliases must use the same union and TypedDict checks as their underlying types.
        match object_ty.resolve_type_alias(db) {
            Type::Union(union) => {
                let mut infer_slice_ty = MultiInferenceGuard::new(infer_slice_ty);
                let mut infer_rhs_value = MultiInferenceGuard::new(infer_rhs_value);

                // Perform loud inference without type context, as there may be multiple
                // equally applicable type contexts for each union member.
                infer_slice_ty.infer_loud(self, TypeContext::default());
                infer_rhs_value.infer_loud(self, TypeContext::default());

                // Note that we use a loop here instead of .all(…) to avoid short-circuiting.
                // We need to keep iterating to emit all diagnostics.
                let mut valid = true;
                for element_ty in union.elements(db) {
                    valid &= self.validate_subscript_assignment_impl(
                        target,
                        full_object_ty.or(Some(object_ty)),
                        *element_ty,
                        &mut |builder, tcx| infer_slice_ty.infer_silent(builder, tcx),
                        rhs_value_node,
                        &mut |builder, tcx| infer_rhs_value.infer_silent(builder, tcx),
                        emit_diagnostic,
                    );
                }

                valid
            }

            Type::Intersection(intersection) => {
                let mut infer_slice_ty = MultiInferenceGuard::new(infer_slice_ty);
                let mut infer_rhs_value = MultiInferenceGuard::new(infer_rhs_value);

                let mut check_positive_elements = |emit_diagnostic_and_short_circuit| {
                    let mut valid = false;
                    for element_ty in intersection.positive(db) {
                        valid |= self.validate_subscript_assignment_impl(
                            target,
                            full_object_ty.or(Some(object_ty)),
                            *element_ty,
                            &mut |builder, tcx| infer_slice_ty.infer_silent(builder, tcx),
                            rhs_value_node,
                            &mut |builder, tcx| infer_rhs_value.infer_silent(builder, tcx),
                            emit_diagnostic_and_short_circuit,
                        );

                        if valid || emit_diagnostic_and_short_circuit {
                            // Otherwise, perform loud inference with the narrowed type context, or the
                            // type context of the first failing element.
                            infer_slice_ty.infer_loud(self, infer_slice_ty.last_tcx());
                            infer_rhs_value.infer_loud(self, infer_rhs_value.last_tcx());
                            break;
                        }
                    }

                    valid
                };

                // Perform an initial check of all elements. If the assignment is valid
                // for at least one element, we do not emit any diagnostics. Otherwise,
                // we re-run the check and emit a diagnostic on the first failing element.
                let valid = check_positive_elements(false);
                if !valid {
                    check_positive_elements(true);
                }

                valid
            }

            Type::EnumComplement(complement) => self.validate_subscript_assignment_impl(
                target,
                full_object_ty,
                complement.remaining_literal_union(db, env),
                infer_slice_ty,
                rhs_value_node,
                infer_rhs_value,
                emit_diagnostic,
            ),

            Type::TypedDict(typed_dict) => {
                // As an optimization, prevent calling `__setitem__` on (unions of) large `TypedDict`s, and
                // validate the assignment ourselves. This also allows us to emit better diagnostics.

                let mut valid = true;
                let slice_ty = infer_slice_ty(self, TypeContext::default());
                let Some(keys) = string_literal_values(db, slice_ty) else {
                    // Check if the key has a valid type. We only allow string literals, a union of string literals,
                    // or a dynamic type like `Any`. We can do this by checking assignability to `LiteralString`,
                    // but we need to exclude `LiteralString` itself. This check would technically allow weird key
                    // types like `LiteralString & Any` to pass, but it does not need to be perfect. We would just
                    // fail to provide the "can only be subscripted with a string literal key" hint in that case.

                    if slice_ty.is_dynamic() {
                        return true;
                    }

                    if slice_ty.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
                        && let Some(expected_ty) = typed_dict.arbitrary_key_mutation_type(db, env)
                    {
                        let rhs_value_ty =
                            infer_rhs_value(self, TypeContext::new(Some(expected_ty)));
                        if rhs_value_ty.is_assignable_to(db, env, expected_ty) {
                            return true;
                        }

                        if emit_diagnostic
                            && let Some(builder) = self
                                .context
                                .report_lint(&INVALID_ASSIGNMENT, rhs_value_node)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot assign value of type `{}` to key of type `{}` \
                                on TypedDict `{}`",
                                rhs_value_ty.display(db, env),
                                slice_ty.display(db, env),
                                object_ty.display(db, env),
                            ));
                            diagnostic.set_primary_annotation_message(format_args!(
                                "Expected value assignable to `{}`",
                                expected_ty.display(db, env)
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                        return false;
                    }

                    let rhs_value_ty = infer_rhs_value(self, TypeContext::default());
                    let assigned_d = rhs_value_ty.display(db, env);
                    let value_d = object_ty.display(db, env);

                    if slice_ty.is_assignable_to(db, env, Type::literal_string())
                        && !slice_ty.is_equivalent_to(db, env, Type::literal_string())
                    {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_ASSIGNMENT, target.slice.as_ref())
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot assign value of type `{assigned_d}` to key of type `{}` \
                                on TypedDict `{value_d}`",
                                slice_ty.display(db, env)
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                    } else {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_KEY, target.slice.as_ref())
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "TypedDict `{value_d}` can only be subscripted \
                                with a string literal key, got key of type `{}`.",
                                slice_ty.display(db, env)
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                    }

                    return false;
                };

                // We may need to infer the value multiple times for distinct keys.
                let mut key_count = 0;
                let mut infer_rhs_value = MultiInferenceGuard::new(infer_rhs_value);

                for key in keys {
                    // Infer the value with type context.
                    let item = typed_dict.item(db, key);
                    let value_ty = infer_rhs_value.infer_silent(
                        self,
                        TypeContext::new(item.as_ref().map(|item| item.declared_ty)),
                    );

                    if item.is_some() {
                        key_count += 1;
                    }
                    valid &= TypedDictKeyAssignment {
                        context: &self.context,
                        typed_dict,
                        full_object_ty,
                        key,
                        value_ty,
                        typed_dict_node: target.value.as_ref().into(),
                        key_node: target.slice.as_ref().into(),
                        value_node: rhs_value_node.into(),
                        assignment_kind: TypedDictAssignmentKind::Subscript,
                        emit_diagnostic,
                    }
                    .validate();
                }

                // Perform loud inference with type context if there is a single key.
                if key_count == 1 {
                    infer_rhs_value.infer_loud(self, infer_rhs_value.last_tcx());
                } else {
                    infer_rhs_value.infer_loud(self, TypeContext::default());
                }

                valid
            }

            _ => {
                let ast_arguments = [
                    ArgOrKeyword::Arg(&target.slice),
                    ArgOrKeyword::Arg(rhs_value_node),
                ];

                let mut call_arguments =
                    CallArguments::positional([Type::unknown(), Type::unknown()]);

                let mut infer_argument_ty =
                    |builder: &mut Self, (argument_index, _, tcx): ArgExpr<'db, '_>| {
                        match argument_index {
                            0 => infer_slice_ty(builder, tcx),
                            1 => infer_rhs_value(builder, tcx),
                            _ => unreachable!(),
                        }
                    };

                let Err(call_dunder_err) = self.infer_and_try_call_dunder(
                    object_ty,
                    "__setitem__",
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    ArgumentsIter::synthesized(&ast_arguments),
                    &mut call_arguments,
                    &mut infer_argument_ty,
                    TypeContext::default(),
                ) else {
                    return true;
                };

                match call_dunder_err {
                    CallDunderError::PossiblyUnbound { .. } => {
                        if emit_diagnostic
                            && let Some(builder) = self
                                .context
                                .report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, target)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Method `__setitem__` of type `{}` may be missing",
                                object_ty.display(db, env),
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                        false
                    }
                    CallDunderError::CallError(call_error_kind, bindings, _) => {
                        let slice_ty = bindings.type_for_argument(&call_arguments, 0);
                        let rhs_value_ty = bindings.type_for_argument(&call_arguments, 1);

                        match call_error_kind {
                            CallErrorKind::NotCallable => {
                                if emit_diagnostic
                                    && let Some(builder) =
                                        self.context.report_lint(&CALL_NON_CALLABLE, target)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Method `__setitem__` of type `{}` is not callable \
                                             on object of type `{}`",
                                        bindings.callable_type().display(db, env),
                                        object_ty.display(db, env),
                                    ));
                                    attach_original_type_info(&mut diagnostic);
                                }
                            }
                            CallErrorKind::BindingError => {
                                if let Some(typed_dict) = object_ty.as_typed_dict() {
                                    if let Some(key) = slice_ty.as_string_literal() {
                                        let key = key.value(db);
                                        TypedDictKeyAssignment {
                                            context: &self.context,
                                            typed_dict,
                                            full_object_ty,
                                            key,
                                            value_ty: rhs_value_ty,
                                            typed_dict_node: target.value.as_ref().into(),
                                            key_node: target.slice.as_ref().into(),
                                            value_node: rhs_value_node.into(),
                                            assignment_kind: TypedDictAssignmentKind::Subscript,
                                            emit_diagnostic: true,
                                        }
                                        .validate();
                                    }
                                } else {
                                    if emit_diagnostic
                                        && let Some(builder) = self.context.report_lint(
                                            &INVALID_ASSIGNMENT,
                                            target.range.cover(rhs_value_node.range()),
                                        )
                                    {
                                        let settings =
                                            DisplaySettings::from_possibly_ambiguous_types(
                                                db,
                                                env,
                                                [rhs_value_ty, object_ty, slice_ty],
                                            );
                                        let assigned_d =
                                            rhs_value_ty.display_with(db, env, settings.clone());
                                        let object_d =
                                            object_ty.display_with(db, env, settings.clone());

                                        let mut diagnostic = builder.into_diagnostic(format_args!(
                                            "Invalid subscript assignment with key of type `{}` \
                                            and value of type `{assigned_d}` \
                                            on object of type `{object_d}`",
                                            slice_ty.display_with(db, env, settings),
                                        ));

                                        // Special diagnostic for dictionaries
                                        if let Some([expected_key_ty, expected_value_ty]) =
                                            object_ty
                                                .known_specialization(db, env, KnownClass::Dict)
                                                .map(|s| s.types(db))
                                        {
                                            if !slice_ty.is_assignable_to(db, env, *expected_key_ty)
                                            {
                                                diagnostic.annotate(
                                                    self.context
                                                        .secondary(target.slice.as_ref())
                                                        .message(format_args!(
                                                            "Expected key of type `{}`, got `{}`",
                                                            expected_key_ty.display(db, env),
                                                            slice_ty.display(db, env),
                                                        )),
                                                );
                                            }

                                            if !rhs_value_ty.is_assignable_to(
                                                db,
                                                env,
                                                *expected_value_ty,
                                            ) {
                                                diagnostic.annotate(
                                                    self.context.secondary(rhs_value_node).message(
                                                        format_args!(
                                                            "Expected value of type `{}`, got `{}`",
                                                            expected_value_ty.display(db, env),
                                                            rhs_value_ty.display(db, env),
                                                        ),
                                                    ),
                                                );
                                            }
                                        }

                                        attach_original_type_info(&mut diagnostic);
                                    }
                                }
                            }
                            CallErrorKind::PossiblyNotCallable => {
                                if emit_diagnostic
                                    && let Some(builder) =
                                        self.context.report_lint(&CALL_NON_CALLABLE, target)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Method `__setitem__` of type `{}` may not be callable \
                                        on object of type `{}`",
                                        bindings.callable_type().display(db, env),
                                        object_ty.display(db, env),
                                    ));
                                    attach_original_type_info(&mut diagnostic);
                                }
                            }
                        }
                        false
                    }
                    CallDunderError::MethodNotAvailable => {
                        if emit_diagnostic
                            && let Some(builder) =
                                self.context.report_lint(&INVALID_ASSIGNMENT, target)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot assign to a subscript on an object of type `{}`",
                                object_ty.display(db, env),
                            ));
                            attach_original_type_info(&mut diagnostic);

                            // If it's a user-defined class, suggest adding a `__setitem__` method.
                            if object_ty
                                .as_nominal_instance()
                                .and_then(|instance| {
                                    instance.class(db, env).static_class_literal(db)
                                })
                                .and_then(|(class_literal, _)| {
                                    let file = class_literal.program_file(db);
                                    file_to_module(db, file.resolver_file(db))
                                })
                                .and_then(|module| module.search_path(db))
                                .is_some_and(ty_module_resolver::SearchPath::is_first_party)
                            {
                                diagnostic.help(format_args!(
                                    "Consider adding a `__setitem__` method to `{}`.",
                                    object_ty.display(db, env),
                                ));
                            } else {
                                diagnostic.info(format_args!(
                                    "`{}` does not have a `__setitem__` method.",
                                    object_ty.display(db, env),
                                ));
                            }
                        }
                        false
                    }
                }
            }
        }
    }

    /// Validate a subscript deletion of the form `del object[key]`.
    fn validate_subscript_deletion(
        &self,
        target: &ast::ExprSubscript,
        object_ty: Type<'db>,
        slice_ty: Type<'db>,
    ) {
        self.validate_subscript_deletion_impl(target, None, object_ty, slice_ty);
    }

    fn validate_subscript_deletion_impl(
        &self,
        target: &'ast ast::ExprSubscript,
        full_object_ty: Option<Type<'db>>,
        object_ty: Type<'db>,
        slice_ty: Type<'db>,
    ) {
        let env = self.program_environment();
        let db = self.db();

        let attach_original_type_info = |diagnostic: &mut LintDiagnosticGuard| {
            if let Some(full_object_ty) = full_object_ty {
                diagnostic.info(format_args!(
                    "The full type of the subscripted object is `{}`",
                    full_object_ty.display(db, env)
                ));
            }
        };

        match object_ty {
            Type::Union(union) => {
                for element_ty in union.elements(db) {
                    self.validate_subscript_deletion_impl(
                        target,
                        full_object_ty.or(Some(object_ty)),
                        *element_ty,
                        slice_ty,
                    );
                }
            }

            Type::Intersection(intersection) => {
                // Check if any positive element supports deletion
                let positive = intersection.positive(db);
                let mut any_valid = false;
                for element_ty in positive {
                    if self.can_delete_subscript(*element_ty, slice_ty) {
                        any_valid = true;
                        break;
                    }
                }

                // If none are valid, emit a diagnostic for the first failing element
                if !any_valid && let Some(element_ty) = positive.first() {
                    self.validate_subscript_deletion_impl(
                        target,
                        full_object_ty.or(Some(object_ty)),
                        *element_ty,
                        slice_ty,
                    );
                }
            }

            Type::EnumComplement(complement) => self.validate_subscript_deletion_impl(
                target,
                full_object_ty,
                complement.remaining_literal_union(db, env),
                slice_ty,
            ),

            _ => {
                if let Type::TypedDict(typed_dict) = object_ty {
                    // Known undeclared keys can only refer to explicit extra items, so they can be
                    // deleted whenever those items are mutable. An arbitrary string key could
                    // instead refer to any declared field, so deletion is only safe when all
                    // possible fields are optional and mutable.
                    let can_delete_extra_literals = typed_dict
                        .explicit_extra_items(db)
                        .is_some_and(|extra_items| !extra_items.is_read_only())
                        && string_literal_values(db, slice_ty).is_some_and(|mut literals| {
                            literals.all(|literal| !typed_dict.items(db).contains_key(literal))
                        });
                    let can_delete_arbitrary_key =
                        slice_ty.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
                            && typed_dict.supports_arbitrary_key_deletion(db);
                    if can_delete_extra_literals || can_delete_arbitrary_key {
                        return;
                    }
                }

                let Err(err) = object_ty.try_call_dunder(
                    db,
                    env,
                    "__delitem__",
                    CallArguments::positional([slice_ty]),
                    TypeContext::default(),
                ) else {
                    return;
                };

                match err {
                    CallDunderError::PossiblyUnbound { .. } => {
                        if let Some(builder) = self
                            .context
                            .report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, target)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Method `__delitem__` of type `{}` may be missing",
                                object_ty.display(db, env),
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                    }
                    CallDunderError::CallError(call_error_kind, bindings, _) => {
                        match call_error_kind {
                            CallErrorKind::NotCallable => {
                                if let Some(builder) =
                                    self.context.report_lint(&CALL_NON_CALLABLE, target)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Method `__delitem__` of type `{}` \
                                        is not callable on object of type `{}`",
                                        bindings.callable_type().display(db, env),
                                        object_ty.display(db, env),
                                    ));
                                    attach_original_type_info(&mut diagnostic);
                                }
                            }
                            CallErrorKind::BindingError => {
                                // For deletions of string literal keys on `TypedDict`, provide
                                // a more detailed diagnostic.
                                if let Some(typed_dict) = object_ty.as_typed_dict() {
                                    if let Some(string_literal) = slice_ty.as_string_literal() {
                                        let key = string_literal.value(db);
                                        let items = typed_dict.items(db);

                                        if let Some(field) = items.get(key) {
                                            // Key exists but is required (i.e., can't be deleted).
                                            report_cannot_delete_typed_dict_key(
                                                &self.context,
                                                (&*target.slice).into(),
                                                typed_dict,
                                                key,
                                                Some(field),
                                                TypedDictDeleteErrorKind::RequiredKey,
                                            );
                                        } else if typed_dict
                                            .explicit_extra_items(db)
                                            .is_some_and(TypedDictExtraItems::is_read_only)
                                        {
                                            report_cannot_delete_typed_dict_key(
                                                &self.context,
                                                (&*target.slice).into(),
                                                typed_dict,
                                                key,
                                                None,
                                                TypedDictDeleteErrorKind::ReadOnlyExtraItem,
                                            );
                                        } else {
                                            // Key doesn't exist.
                                            report_cannot_delete_typed_dict_key(
                                                &self.context,
                                                (&*target.slice).into(),
                                                typed_dict,
                                                key,
                                                None,
                                                TypedDictDeleteErrorKind::UnknownKey,
                                            );
                                        }
                                    } else {
                                        // Non-string-literal key on `TypedDict`.
                                        if let Some(builder) =
                                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, target)
                                        {
                                            let mut diagnostic =
                                                builder.into_diagnostic(format_args!(
                                                    "Method `__delitem__` of type `{}` \
                                                    cannot be called with key of type \
                                                    `{}` on object of type `{}`",
                                                    bindings.callable_type().display(db, env),
                                                    slice_ty.display(db, env),
                                                    object_ty.display(db, env),
                                                ));
                                            attach_original_type_info(&mut diagnostic);
                                        }
                                    }
                                } else {
                                    // Non-`TypedDict` object
                                    if let Some(builder) =
                                        self.context.report_lint(&INVALID_ARGUMENT_TYPE, target)
                                    {
                                        let mut diagnostic = builder.into_diagnostic(format_args!(
                                            "Method `__delitem__` of type `{}` cannot \
                                            be called with key of type `{}` on \
                                            object of type `{}`",
                                            bindings.callable_type().display(db, env),
                                            slice_ty.display(db, env),
                                            object_ty.display(db, env),
                                        ));
                                        attach_original_type_info(&mut diagnostic);
                                    }
                                }
                            }
                            CallErrorKind::PossiblyNotCallable => {
                                if let Some(builder) =
                                    self.context.report_lint(&CALL_NON_CALLABLE, target)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Method `__delitem__` of type `{}` may not be \
                                        callable on object of type `{}`",
                                        bindings.callable_type().display(db, env),
                                        object_ty.display(db, env),
                                    ));
                                    attach_original_type_info(&mut diagnostic);
                                }
                            }
                        }
                    }
                    CallDunderError::MethodNotAvailable => {
                        report_not_subscriptable(&self.context, target, object_ty, "__delitem__");
                    }
                }
            }
        }
    }

    /// Check if a type supports subscript deletion (has `__delitem__`).
    fn can_delete_subscript(&self, object_ty: Type<'db>, slice_ty: Type<'db>) -> bool {
        let db = self.db();
        object_ty
            .try_call_dunder(
                db,
                self.program_environment(),
                "__delitem__",
                CallArguments::positional([slice_ty]),
                TypeContext::default(),
            )
            .is_ok()
    }

    pub(super) fn parse_subscription_of_annotated_special_form(
        &mut self,
        subscript: &ast::ExprSubscript,
        subscript_context: AnnotatedExprContext,
    ) -> TypeAndQualifiers<'db> {
        let slice = &*subscript.slice;
        let ast::Expr::Tuple(ast::ExprTuple {
            elts: arguments, ..
        }) = slice
        else {
            report_invalid_arguments_to_annotated(&self.context, subscript);
            return subscript_context.infer(self, slice);
        };

        if arguments.len() < 2 {
            report_invalid_arguments_to_annotated(&self.context, subscript);
        }

        let Some(first_argument) = arguments.first() else {
            self.infer_expression(slice, TypeContext::default());
            return TypeAndQualifiers::declared(Type::unknown());
        };

        let previous_in_type_alias = self
            .context
            .inference_flags
            .replace(InferenceFlags::IN_TYPE_ALIAS, false);
        for metadata_element in &arguments[1..] {
            self.infer_expression(metadata_element, TypeContext::default());
        }
        self.context
            .inference_flags
            .set(InferenceFlags::IN_TYPE_ALIAS, previous_in_type_alias);

        subscript_context.infer(self, first_argument)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnnotatedExprContext {
    TypeExpression,
    AnnotationExpression,
}

impl AnnotatedExprContext {
    fn infer<'db>(
        self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        argument: &ast::Expr,
    ) -> TypeAndQualifiers<'db> {
        match self {
            AnnotatedExprContext::TypeExpression => {
                let inner = builder.infer_type_expression(argument);
                let outer = Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
                    builder.db(),
                    inner,
                )));
                TypeAndQualifiers::declared(outer)
            }
            AnnotatedExprContext::AnnotationExpression => {
                let inner =
                    builder.infer_annotation_expression_impl(argument, PEP613Policy::Disallowed);
                let outer = Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
                    builder.db(),
                    inner.inner_type(),
                )));
                TypeAndQualifiers::declared(outer).with_qualifier(inner.qualifiers())
            }
        }
    }
}
