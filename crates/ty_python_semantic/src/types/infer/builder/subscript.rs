use itertools::{Either, EitherOrBoth, Itertools};
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, ArgOrKeyword, ExprContext};
use ruff_text_size::Ranged;
use ty_module_resolver::file_to_module;

use super::TypeInferenceBuilder;
use crate::place::{DefinedPlace, Definedness, Place};
use crate::types::call::CallErrorKind;
use crate::types::call::bind::CallableDescription;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, INVALID_KEY,
    INVALID_TYPE_ARGUMENTS, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE, POSSIBLY_MISSING_IMPLICIT_CALL,
    TypedDictDeleteErrorKind, report_cannot_delete_typed_dict_key,
    report_invalid_arguments_to_annotated, report_invalid_key_on_typed_dict,
    report_not_subscriptable,
};
use crate::types::generics::{GenericContext, InferableTypeVars, bind_typevar};
use crate::types::infer::builder::annotation_expression::PEP613Policy;
use crate::types::infer::builder::{ArgExpr, ArgumentsIter, MultiInferenceGuard};
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::special_form::AliasSpec;
use crate::types::subscript::{LegacyGenericOrigin, SubscriptError, SubscriptErrorKind};
use crate::types::tuple::{Tuple, TupleType};
use crate::types::typed_dict::{TypedDictAssignmentKind, TypedDictKeyAssignment};
use crate::types::{
    BoundTypeVarInstance, CallArguments, CallDunderError, CycleDetector, DynamicType, InternedType,
    KnownClass, KnownInstanceType, LintDiagnosticGuard, Parameter, Parameters, SpecialFormType,
    StaticClassLiteral, Type, TypeAliasType, TypeAndQualifiers, TypeContext,
    TypeVarBoundOrConstraints, UnionType, UnionTypeInstance, any_over_type, todo_type,
};
use crate::{Db, FxOrderSet};
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;
use ty_python_core::place::{PlaceExpr, PlaceExprRef};
use ty_python_core::scope::FileScopeId;

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(super) fn typed_dict_key_expected_type(&self, ty: Type<'db>) -> Option<Type<'db>> {
        struct TypedDictKeyExpectedType;
        type TypedDictKeyExpectedTypeVisitor<'db> =
            CycleDetector<TypedDictKeyExpectedType, Type<'db>, Option<Type<'db>>>;

        fn imp<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            visitor: &TypedDictKeyExpectedTypeVisitor<'db>,
        ) -> Option<Type<'db>> {
            match ty {
                Type::TypedDict(typed_dict) => {
                    let keys = typed_dict
                        .items(db)
                        .keys()
                        .map(|key| Type::string_literal(db, key.as_str()))
                        .collect_vec();
                    (!keys.is_empty()).then(|| UnionType::from_elements(db, keys))
                }
                Type::Union(union) => {
                    let keys = union
                        .elements(db)
                        .iter()
                        .filter_map(|element| imp(db, *element, visitor))
                        .collect_vec();
                    (!keys.is_empty()).then(|| UnionType::from_elements(db, keys))
                }
                Type::Intersection(intersection) => {
                    let keys = intersection
                        .positive(db)
                        .iter()
                        .filter_map(|element| imp(db, *element, visitor))
                        .collect_vec();
                    (!keys.is_empty()).then(|| UnionType::from_elements(db, keys))
                }
                Type::TypeAlias(alias) => {
                    visitor.visit(ty, || imp(db, alias.value_type(db), visitor))
                }
                _ => None,
            }
        }

        imp(self.db(), ty, &TypedDictKeyExpectedTypeVisitor::default())
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
            ExprContext::Load => self.infer_subscript_load(subscript),
            ExprContext::Store => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                self.store_typed_dict_key_expected_type(slice, value_ty);
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
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
                self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                Type::unknown()
            }
        }
    }

    pub(super) fn infer_subscript_load(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        let value_ty = self.infer_expression(&subscript.value, TypeContext::default());

        // If we have an implicit type alias like `MyList = list[T]`, and if `MyList` is being
        // used in another implicit type alias like `Numbers = MyList[int]`, then we infer the
        // right hand side as a value expression, and need to handle the specialization here.
        if value_ty.is_generic_alias() {
            return self.infer_explicit_type_alias_specialization(subscript, value_ty, false);
        }

        self.infer_subscript_load_impl(value_ty, subscript)
    }

    pub(super) fn infer_subscript_load_impl(
        &mut self,
        value_ty: Type<'db>,
        subscript: &ast::ExprSubscript,
    ) -> Type<'db> {
        let db = self.db();

        let ast::ExprSubscript {
            range: _,
            node_index: _,
            value: _,
            slice,
            ctx,
        } = subscript;

        self.store_typed_dict_key_expected_type(slice, value_ty);

        let mut constraint_keys = vec![];

        // If `value` is a valid reference, we attempt type narrowing by assignment.
        if !value_ty.is_unknown() {
            if let Some(expr) = PlaceExpr::try_from_expr(subscript) {
                let (place, keys) = self.infer_place_load(
                    PlaceExprRef::from(&expr),
                    ast::ExprRef::Subscript(subscript),
                );
                constraint_keys.extend(keys);
                if let Place::Defined(DefinedPlace {
                    ty,
                    definedness: Definedness::AlwaysDefined,
                    ..
                }) = place.place
                {
                    // Even if we can obtain the subscript type based on the assignments, we still perform default type inference
                    // (to store the expression type and to report errors).
                    let slice_ty = self.infer_expression(slice, TypeContext::default());
                    self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                    return ty;
                }
            }
        }

        let tuple_generic_alias = |db: &'db dyn Db, tuple: Option<TupleType<'db>>| {
            let tuple = tuple.unwrap_or_else(|| TupleType::homogeneous(db, Type::unknown()));
            Type::from(tuple.to_class_type(db))
        };

        match value_ty {
            Type::ClassLiteral(class) => {
                // HACK ALERT: If we are subscripting a generic class, short-circuit the rest of the
                // subscript inference logic and treat this as an explicit specialization.
                // TODO: Move this logic into a custom callable, and update `find_name_in_mro` to return
                // this callable as the `__class_getitem__` method on `type`. That probably requires
                // updating all of the subscript logic below to use custom callables for all of the _other_
                // special cases, too.
                if class.is_tuple(db) {
                    return tuple_generic_alias(db, self.infer_tuple_type_expression(subscript));
                } else if class.is_known(db, KnownClass::Type) {
                    let argument_ty = self.infer_type_expression(slice);
                    return Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                        InternedType::new(db, argument_ty),
                    ));
                }

                if let Some(generic_context) = class.generic_context(db)
                    && let Some(class) = class.as_static()
                {
                    return self.infer_explicit_class_specialization(
                        subscript,
                        value_ty,
                        class,
                        generic_context,
                    );
                }
            }
            Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::ManualPEP695(
                _,
            ))) => {
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    db,
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(db, variables);
                return Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
            }
            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias)) => {
                if let Some(generic_context) = type_alias.generic_context(db) {
                    return self.infer_explicit_type_alias_type_specialization(
                        subscript,
                        value_ty,
                        type_alias,
                        generic_context,
                    );
                }
            }
            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::Tuple => {
                    return tuple_generic_alias(db, self.infer_tuple_type_expression(subscript));
                }
                SpecialFormType::Literal => match self.infer_literal_parameter_type(slice) {
                    Ok(result) => {
                        return Type::KnownInstance(KnownInstanceType::Literal(InternedType::new(
                            db, result,
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
                            a literal value (int, bool, str, or bytes), or an enum member",
                            );
                        }
                        return Type::unknown();
                    }
                },
                SpecialFormType::Annotated => {
                    return self
                        .parse_subscription_of_annotated_special_form(
                            subscript,
                            AnnotatedExprContext::TypeExpression,
                        )
                        .inner_type();
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
                        return ty;
                    }

                    return Type::KnownInstance(KnownInstanceType::UnionType(
                        UnionTypeInstance::new(
                            db,
                            None,
                            Ok(UnionType::from_two_elements(db, ty, Type::none(db))),
                        ),
                    ));
                }
                SpecialFormType::Union => match **slice {
                    ast::Expr::Tuple(ref tuple) => {
                        let elements = tuple.iter().map(|elt| self.infer_type_expression(elt));

                        let union_type = Type::KnownInstance(KnownInstanceType::UnionType(
                            UnionTypeInstance::new(
                                db,
                                None,
                                Ok(UnionType::from_elements(db, elements)),
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

                        return union_type;
                    }
                    _ => {
                        return self.infer_expression(slice, TypeContext::default());
                    }
                },
                SpecialFormType::Type => {
                    // Similar to the branch above that handles `type[…]`, handle `typing.Type[…]`
                    let argument_ty = self.infer_type_expression(slice);
                    return Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                        InternedType::new(db, argument_ty),
                    ));
                }
                SpecialFormType::Callable => {
                    let callable = self
                        .infer_callable_type(subscript)
                        .as_callable()
                        .expect("always returns Type::Callable");

                    return Type::KnownInstance(KnownInstanceType::Callable(callable));
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

                    return class
                        .to_specialized_class_type(db, arg_types)
                        .map(Type::from)
                        .unwrap_or_else(Type::unknown);
                }
                _ => {}
            },

            Type::KnownInstance(
                KnownInstanceType::UnionType(_)
                | KnownInstanceType::Annotated(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::TypeGenericAlias(_),
            ) => {
                return self.infer_explicit_type_alias_specialization(subscript, value_ty, false);
            }
            Type::Dynamic(DynamicType::Unknown) => {
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    db,
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(db, variables);
                return Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
            }
            _ => {}
        }

        let slice_ty = self.infer_expression(slice, TypeContext::default());
        let result_ty = self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
        self.narrow_expr_with_applicable_constraints(subscript, result_ty, &constraint_keys)
    }

    pub(super) fn infer_explicit_class_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let specialize = &|types: &[Option<Type<'db>>]| {
            Type::from(generic_class.apply_specialization(db, |_| {
                generic_context.specialize_partial(db, types.iter().copied())
            }))
        };

        self.infer_explicit_callable_specialization(
            subscript,
            value_ty,
            generic_context,
            specialize,
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
        let previously_allowed_paramspec = self
            .context
            .inference_flags
            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
        let result = self.infer_explicit_callable_specialization_impl(
            subscript,
            value_ty,
            generic_context,
            specialize,
        );
        self.context.inference_flags.set(
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
            previously_allowed_paramspec,
        );
        result
    }

    pub(super) fn infer_explicit_callable_specialization_impl(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        generic_context: GenericContext<'db>,
        specialize: &dyn Fn(&[Option<Type<'db>>]) -> Type<'db>,
    ) -> Type<'db> {
        enum ExplicitSpecializationError {
            InvalidParamSpec,
            ParamSpecForTypeVar,
            UnsatisfiedBound,
            UnsatisfiedConstraints,
            /// These two errors override the errors above, causing all specializations to be `Unknown`.
            MissingTypeVars,
            TooManyArguments,
            /// This error overrides the errors above, causing the type itself to be `Unknown`.
            NonGeneric,
        }

        fn add_typevar_definition<'db>(
            db: &'db dyn Db,
            diagnostic: &mut Diagnostic,
            typevar: BoundTypeVarInstance<'db>,
        ) {
            let Some(definition) = typevar.typevar(db).definition(db) else {
                return;
            };
            let file = definition.file(db);
            let module = parsed_module(db, file).load(db);
            let range = definition.focus_range(db, &module).range();
            diagnostic.annotate(
                Annotation::secondary(Span::from(file).with_range(range))
                    .message("Type variable defined here"),
            );
        }

        /// A type argument after expanding any allowed `Unpack[tuple[...]]` syntax.
        struct TypeArgument<'ast, 'db> {
            /// The source expression used for diagnostics and deferred inference.
            node: &'ast ast::Expr,
            /// The already-inferred type, if this argument did not need deferred inference.
            ty: Option<Type<'db>>,
            /// The index of the original source argument before any `Unpack` expansion.
            source_index: usize,
        }

        let db = self.db();
        let constraints = ConstraintSetBuilder::new();
        let slice_node = subscript.slice.as_ref();

        let exactly_one_paramspec = generic_context.exactly_one_paramspec(db);
        let (type_arguments, store_inferred_type_arguments) = match slice_node {
            ast::Expr::Tuple(tuple) => {
                if exactly_one_paramspec && !tuple.elts.is_empty() {
                    (std::slice::from_ref(slice_node), false)
                } else {
                    (tuple.elts.as_slice(), true)
                }
            }
            _ => (std::slice::from_ref(slice_node), false),
        };
        let mut inferred_type_arguments = vec![None; type_arguments.len()];

        let typevars = generic_context.variables(db).collect::<Vec<_>>();
        let typevars_len = typevars.len();

        let mut expanded_type_arguments = Vec::with_capacity(type_arguments.len());

        for (source_index, expr) in type_arguments.iter().enumerate() {
            let typevar = typevars.get(expanded_type_arguments.len()).copied();
            if exactly_one_paramspec || typevar.is_some_and(|typevar| typevar.is_paramspec(db)) {
                expanded_type_arguments.push(TypeArgument {
                    node: expr,
                    ty: None,
                    source_index,
                });
                continue;
            }

            let provided_type = if typevars_len == 0 {
                // If there are no typevars at all, this is not a generic type,
                // so we should not infer excess arguments as type expressions.
                // For example, `list[int][0]` — the `0` is not a type expression.
                self.infer_expression(expr, TypeContext::default())
            } else {
                let previously_in_valid_unpack_context = self
                    .context
                    .inference_flags
                    .replace(InferenceFlags::IN_VALID_UNPACK_CONTEXT, true);
                let provided_type = self.infer_type_expression(expr);
                self.context.inference_flags.set(
                    InferenceFlags::IN_VALID_UNPACK_CONTEXT,
                    previously_in_valid_unpack_context,
                );
                provided_type
            };

            inferred_type_arguments[source_index] = Some(provided_type);

            let is_unpack = self
                .type_expression_flags(expr)
                .contains(TypeExpressionFlags::UNPACK);

            if is_unpack
                && let Some(tuple) = provided_type.exact_tuple_instance_spec(db)
                && let Tuple::Fixed(tuple) = tuple.as_ref()
                && expanded_type_arguments.len() <= typevars_len
                && typevars[expanded_type_arguments.len()
                    ..usize::min(
                        expanded_type_arguments.len() + tuple.elements_slice().len(),
                        typevars_len,
                    )]
                    .iter()
                    .all(|typevar| !typevar.is_paramspec(db))
            {
                // Expand `Foo[Unpack[tuple[int, str]]]` to `Foo[int, str]`. ParamSpec arguments
                // must still use their dedicated inference path.
                expanded_type_arguments.extend(tuple.iter_all_elements().map(|ty| TypeArgument {
                    node: expr,
                    ty: Some(ty),
                    source_index,
                }));
            } else {
                if is_unpack
                    && !self
                        .inference_flags()
                        .contains(InferenceFlags::IN_KWARG_ANNOTATION)
                    && !matches!(
                        value_ty,
                        Type::GenericAlias(alias)
                            if alias
                                .specialization(db)
                                .types(db)
                                .contains(&Type::Dynamic(DynamicType::TodoTypeVarTuple))
                    )
                    && let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, expr)
                {
                    builder.into_diagnostic(
                        "`Unpack` can only be used with a fixed tuple type in this context",
                    );
                }

                expanded_type_arguments.push(TypeArgument {
                    node: expr,
                    ty: Some(provided_type),
                    source_index,
                });
            }
        }

        let mut specialization_types = Vec::with_capacity(typevars_len);
        let mut typevar_with_defaults = 0;
        let mut missing_typevars = vec![];
        let mut first_excess_type_argument_index = None;

        let mut error: Option<ExplicitSpecializationError> = None;

        for (index, item) in typevars
            .iter()
            .copied()
            .zip_longest(expanded_type_arguments.iter())
            .enumerate()
        {
            match item {
                EitherOrBoth::Both(typevar, type_argument) => {
                    if typevar.default_type(db).is_some() {
                        typevar_with_defaults += 1;
                    }

                    let provided_type = if typevar.is_paramspec(db) {
                        let provided_type = self
                            .infer_paramspec_explicit_specialization_value(
                                type_argument.node,
                                exactly_one_paramspec,
                            )
                            .unwrap_or_else(|()| {
                                error = Some(ExplicitSpecializationError::InvalidParamSpec);
                                Type::paramspec_value_callable(db, Parameters::unknown())
                            });
                        inferred_type_arguments[type_argument.source_index] = Some(provided_type);
                        provided_type
                    } else {
                        type_argument.ty.unwrap_or_else(|| {
                            let previously_in_valid_unpack_context = self
                                .context
                                .inference_flags
                                .replace(InferenceFlags::IN_VALID_UNPACK_CONTEXT, true);
                            let provided_type = self.infer_type_expression(type_argument.node);
                            self.context.inference_flags.set(
                                InferenceFlags::IN_VALID_UNPACK_CONTEXT,
                                previously_in_valid_unpack_context,
                            );
                            inferred_type_arguments[type_argument.source_index] =
                                Some(provided_type);
                            provided_type
                        })
                    };

                    // A ParamSpec cannot be used to specialize a regular TypeVar.
                    if !typevar.is_paramspec(db)
                        && let Type::TypeVar(tv) = provided_type
                        && tv.is_paramspec(db)
                    {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_TYPE_ARGUMENTS, type_argument.node)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "ParamSpec `{}` cannot be used to specialize \
                                    type variable `{}`",
                                tv.typevar(db).name(db),
                                typevar.name(db),
                            ));
                            for (kind, var) in [("ParamSpec", tv), ("Type variable", typevar)] {
                                let Some(definition) = var.typevar(db).definition(db) else {
                                    continue;
                                };
                                let file = definition.file(db);
                                let module = parsed_module(db, file).load(db);
                                let range = definition.focus_range(db, &module).range();
                                diagnostic.annotate(
                                    Annotation::secondary(Span::from(file).with_range(range))
                                        .message(format_args!(
                                            "{kind} `{}` defined here",
                                            var.name(db)
                                        )),
                                );
                            }
                        }
                        error = Some(ExplicitSpecializationError::ParamSpecForTypeVar);
                        specialization_types.push(Some(Type::unknown()));
                        continue;
                    }

                    // TODO consider just accepting the given specialization without checking
                    // against bounds/constraints, but recording the expression for deferred
                    // checking at end of scope. This would avoid a lot of cycles caused by eagerly
                    // doing assignment checks here.
                    match typevar.typevar(db).bound_or_constraints(db) {
                        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                            if provided_type
                                .when_assignable_to(
                                    db,
                                    bound,
                                    &constraints,
                                    InferableTypeVars::None,
                                )
                                .is_never_satisfied(db)
                            {
                                if let Some(builder) = self
                                    .context
                                    .report_lint(&INVALID_TYPE_ARGUMENTS, type_argument.node)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Type `{}` is not assignable to upper bound `{}` \
                                            of type variable `{}`",
                                        provided_type.display(db),
                                        bound.display(db),
                                        typevar.identity(db).display(db),
                                    ));
                                    add_typevar_definition(db, &mut diagnostic, typevar);
                                }
                                error = Some(ExplicitSpecializationError::UnsatisfiedBound);
                                specialization_types.push(Some(Type::unknown()));
                            } else {
                                specialization_types.push(Some(provided_type));
                            }
                        }
                        Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                            // TODO: this is wrong, the given specialization needs to be assignable
                            // to _at least one_ of the individual constraints, not to the union of
                            // all of them. `int | str` is not a valid specialization of a typevar
                            // constrained to `(int, str)`.
                            if provided_type
                                .when_assignable_to(
                                    db,
                                    typevar_constraints.as_type(db),
                                    &constraints,
                                    InferableTypeVars::None,
                                )
                                .is_never_satisfied(db)
                            {
                                if let Some(builder) = self
                                    .context
                                    .report_lint(&INVALID_TYPE_ARGUMENTS, type_argument.node)
                                {
                                    let mut diagnostic = builder.into_diagnostic(format_args!(
                                        "Type `{}` does not satisfy constraints `{}` \
                                            of type variable `{}`",
                                        provided_type.display(db),
                                        typevar_constraints
                                            .elements(db)
                                            .iter()
                                            .map(|c| c.display(db))
                                            .format("`, `"),
                                        typevar.identity(db).display(db),
                                    ));
                                    add_typevar_definition(db, &mut diagnostic, typevar);
                                }
                                error = Some(ExplicitSpecializationError::UnsatisfiedConstraints);
                                specialization_types.push(Some(Type::unknown()));
                            } else {
                                specialization_types.push(Some(provided_type));
                            }
                        }
                        None => {
                            specialization_types.push(Some(provided_type));
                        }
                    }
                }
                EitherOrBoth::Left(typevar) => {
                    if typevar.default_type(db).is_none() {
                        // This is an error case, so no need to push into the specialization types.
                        missing_typevars.push(typevar);
                    } else {
                        typevar_with_defaults += 1;
                        specialization_types.push(None);
                    }
                }
                EitherOrBoth::Right(_) => {
                    first_excess_type_argument_index.get_or_insert(index);
                }
            }
        }

        if !missing_typevars.is_empty() {
            if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, subscript) {
                let description = CallableDescription::new(db, value_ty);
                let s = if missing_typevars.len() > 1 { "s" } else { "" };
                builder.into_diagnostic(format_args!(
                    "No type argument{s} provided for required type variable{s} `{}`{}",
                    missing_typevars
                        .iter()
                        .map(|tv| tv.typevar(db).name(db))
                        .format("`, `"),
                    description
                        .map(|description| format!(" of {description}"))
                        .unwrap_or_default()
                ));
            }
            error = Some(ExplicitSpecializationError::MissingTypeVars);
        }

        if let Some(first_excess_type_argument_index) = first_excess_type_argument_index {
            if let Type::GenericAlias(alias) = value_ty
                && alias
                    .specialization(db)
                    .types(db)
                    .contains(&Type::Dynamic(DynamicType::TodoTypeVarTuple))
            {
                // Avoid false-positive errors when specializing a class
                // that's generic over a legacy TypeVarTuple
            } else if typevars_len == 0 {
                // Type parameter list cannot be empty, so if we reach here, `value_ty` is not a generic type.
                if let Some(builder) = self.context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Cannot subscript non-generic type `{}`",
                        value_ty.display(db)
                    ));
                    let already_specialized = match value_ty {
                        Type::GenericAlias(_) => true,
                        Type::KnownInstance(KnownInstanceType::UnionType(union)) => union
                            .value_expression_types(db)
                            .is_ok_and(|mut tys| tys.any(|ty| ty.is_generic_alias())),
                        _ => false,
                    };
                    if already_specialized {
                        diagnostic.annotate(
                            self.context
                                .secondary(&*subscript.value)
                                .message("Type is already specialized"),
                        );
                    }
                }
                error = Some(ExplicitSpecializationError::NonGeneric);
            } else {
                let node = expanded_type_arguments[first_excess_type_argument_index].node;
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                    let description = CallableDescription::new(db, value_ty);
                    builder.into_diagnostic(format_args!(
                        "Too many type arguments{}: expected {}, got {}",
                        description
                            .map(|description| format!(" to {description}"))
                            .unwrap_or_default(),
                        if typevar_with_defaults == 0 {
                            format!("{typevars_len}")
                        } else {
                            format!(
                                "between {} and {}",
                                typevars_len - typevar_with_defaults,
                                typevars_len
                            )
                        },
                        expanded_type_arguments.len(),
                    ));
                }
                error = Some(ExplicitSpecializationError::TooManyArguments);
            }
        }

        if store_inferred_type_arguments {
            self.store_expression_type(
                slice_node,
                Type::heterogeneous_tuple(
                    db,
                    inferred_type_arguments
                        .into_iter()
                        .map(|ty| ty.unwrap_or(Type::unknown())),
                ),
            );
        }

        match error {
            Some(ExplicitSpecializationError::NonGeneric) => Type::unknown(),
            Some(
                ExplicitSpecializationError::MissingTypeVars
                | ExplicitSpecializationError::TooManyArguments,
            ) => {
                let unknowns = generic_context
                    .variables(db)
                    .map(|typevar| {
                        Some(if typevar.is_paramspec(db) {
                            Type::paramspec_value_callable(db, Parameters::unknown())
                        } else {
                            Type::unknown()
                        })
                    })
                    .collect::<Vec<_>>();
                specialize(&unknowns)
            }
            Some(
                ExplicitSpecializationError::UnsatisfiedBound
                | ExplicitSpecializationError::UnsatisfiedConstraints
                | ExplicitSpecializationError::InvalidParamSpec
                | ExplicitSpecializationError::ParamSpecForTypeVar,
            )
            | None => specialize(&specialization_types),
        }
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
                    Parameters::new(
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
                            Parameters::new(
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
                                Parameters::new(
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

    pub(super) fn infer_subscript_expression_types(
        &self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
        expr_context: ExprContext,
    ) -> Type<'db> {
        let db = self.db();

        // Special typing forms for which subscriptions are context-dependent are parsed here,
        // outside of `Type::subscript`, which is a pure function that doesn't depend on the
        // semantic index or any context-dependent state.
        let subscript_result = match value_ty {
            Type::SpecialForm(SpecialFormType::Generic) => infer_legacy_generic_subscript(
                db,
                self.index,
                self.scope().file_scope_id(db),
                self.typevar_binding_context,
                slice_ty,
                LegacyGenericOrigin::Generic,
                KnownInstanceType::SubscriptedGeneric,
            ),
            Type::SpecialForm(SpecialFormType::Protocol) => infer_legacy_generic_subscript(
                db,
                self.index,
                self.scope().file_scope_id(db),
                self.typevar_binding_context,
                slice_ty,
                LegacyGenericOrigin::Protocol,
                KnownInstanceType::SubscriptedProtocol,
            ),
            Type::SpecialForm(SpecialFormType::Concatenate) => {
                // TODO: Add proper support for `Concatenate`
                let mut variables = FxOrderSet::default();
                slice_ty.bind_and_find_all_legacy_typevars(
                    db,
                    self.typevar_binding_context,
                    &mut variables,
                );
                let generic_context = GenericContext::from_typevar_instances(db, variables);
                Ok(Type::Dynamic(DynamicType::UnknownGeneric(generic_context)))
            }
            _ => value_ty.subscript(db, slice_ty, expr_context),
        };

        subscript_result.unwrap_or_else(|e| {
            e.report_diagnostics(&self.context, subscript);
            e.result_type()
        })
    }

    pub(super) fn infer_slice_expression(&mut self, slice: &ast::ExprSlice) -> Type<'db> {
        enum SliceArg<'db> {
            Arg(Type<'db>),
            Unsupported,
        }

        let db = self.db();

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

        let type_to_slice_argument = |ty: Option<Type<'db>>| match ty {
            Some(ty @ Type::LiteralValue(literal)) if literal.is_int() || literal.is_bool() => {
                SliceArg::Arg(ty)
            }
            Some(ty @ Type::NominalInstance(instance))
                if instance.has_known_class(db, KnownClass::NoneType) =>
            {
                SliceArg::Arg(ty)
            }
            None => SliceArg::Arg(Type::none(db)),
            _ => SliceArg::Unsupported,
        };

        match (
            type_to_slice_argument(ty_lower),
            type_to_slice_argument(ty_upper),
            type_to_slice_argument(ty_step),
        ) {
            (SliceArg::Arg(lower), SliceArg::Arg(upper), SliceArg::Arg(step)) => {
                KnownClass::Slice.to_specialized_instance(db, &[lower, upper, step])
            }
            _ => KnownClass::Slice.to_instance(db),
        }
    }

    /// Validate a subscript assignment of the form `object[key] = rhs_value`.
    pub(super) fn validate_subscript_assignment(
        &mut self,
        target: &ast::ExprSubscript,
        rhs_value: &ast::Expr,
        infer_rhs_value: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
    ) -> bool {
        let ast::ExprSubscript {
            range: _,
            node_index: _,
            value: object,
            slice,
            ctx: _,
        } = target;

        let object_ty = self.infer_expression(object, TypeContext::default());
        self.store_typed_dict_key_expected_type(slice, object_ty);
        let mut infer_slice_ty = |builder: &mut Self, tcx| builder.infer_expression(slice, tcx);

        self.validate_subscript_assignment_impl(
            target,
            None,
            object_ty,
            &mut infer_slice_ty,
            rhs_value,
            infer_rhs_value,
            true,
        )
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
        /// Given a string literal or a union of string literals, return an iterator over the contained
        /// strings, or `None`, if the type is neither.
        fn key_literals<'db>(
            db: &'db dyn Db,
            slice_ty: Type<'db>,
        ) -> Option<impl Iterator<Item = &'db str> + 'db> {
            if let Some(literal) = slice_ty.as_string_literal() {
                Some(Either::Left(std::iter::once(literal.value(db))))
            } else {
                slice_ty.as_union().map(|union| {
                    Either::Right(
                        union
                            .elements(db)
                            .iter()
                            .filter_map(|ty| ty.as_string_literal().map(|lit| lit.value(db))),
                    )
                })
            }
        }

        let db = self.db();

        let attach_original_type_info = |diagnostic: &mut LintDiagnosticGuard| {
            if let Some(full_object_ty) = full_object_ty {
                diagnostic.info(format_args!(
                    "The full type of the subscripted object is `{}`",
                    full_object_ty.display(db)
                ));
            }
        };

        match object_ty {
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

            Type::TypedDict(typed_dict) => {
                // As an optimization, prevent calling `__setitem__` on (unions of) large `TypedDict`s, and
                // validate the assignment ourselves. This also allows us to emit better diagnostics.

                let mut valid = true;
                let slice_ty = infer_slice_ty(self, TypeContext::default());
                let Some(keys) = key_literals(db, slice_ty) else {
                    let rhs_value_ty = infer_rhs_value(self, TypeContext::default());

                    // Check if the key has a valid type. We only allow string literals, a union of string literals,
                    // or a dynamic type like `Any`. We can do this by checking assignability to `LiteralString`,
                    // but we need to exclude `LiteralString` itself. This check would technically allow weird key
                    // types like `LiteralString & Any` to pass, but it does not need to be perfect. We would just
                    // fail to provide the "can only be subscripted with a string literal key" hint in that case.

                    if slice_ty.is_dynamic() {
                        return true;
                    }

                    let assigned_d = rhs_value_ty.display(db);
                    let value_d = object_ty.display(db);

                    if slice_ty.is_assignable_to(db, Type::literal_string())
                        && !slice_ty.is_equivalent_to(db, Type::literal_string())
                    {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_ASSIGNMENT, target.slice.as_ref())
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot assign value of type `{assigned_d}` to key of type `{}` on TypedDict `{value_d}`",
                                slice_ty.display(db)
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                    } else {
                        if let Some(builder) = self
                            .context
                            .report_lint(&INVALID_KEY, target.slice.as_ref())
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "TypedDict `{value_d}` can only be subscripted with a string literal key, got key of type `{}`.",
                                slice_ty.display(db)
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
                    let items = typed_dict.items(db);

                    // Check if the key exists on the `TypedDict`
                    let Some((_, item)) = items.iter().find(|(name, _)| *name == key) else {
                        if emit_diagnostic {
                            report_invalid_key_on_typed_dict(
                                &self.context,
                                target.value.as_ref().into(),
                                target.slice.as_ref().into(),
                                object_ty,
                                full_object_ty,
                                Type::string_literal(db, key),
                                items,
                            );
                        }

                        valid = false;
                        continue;
                    };

                    // Infer the value with type context.
                    let value_ty = infer_rhs_value
                        .infer_silent(self, TypeContext::new(Some(item.declared_ty)));

                    key_count += 1;
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
                    db,
                    object_ty,
                    "__setitem__",
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
                                object_ty.display(db),
                            ));
                            attach_original_type_info(&mut diagnostic);
                        }
                        false
                    }
                    CallDunderError::CallError(call_error_kind, bindings) => {
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
                                        bindings.callable_type().display(db),
                                        object_ty.display(db),
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
                                        let assigned_d = rhs_value_ty.display(db);
                                        let object_d = object_ty.display(db);

                                        let mut diagnostic = builder.into_diagnostic(format_args!(
                                                    "Invalid subscript assignment with key of type `{}` and value of \
                                                     type `{assigned_d}` on object of type `{object_d}`",
                                                    slice_ty.display(db),
                                                ));

                                        // Special diagnostic for dictionaries
                                        if let Some([expected_key_ty, expected_value_ty]) =
                                            object_ty
                                                .known_specialization(db, KnownClass::Dict)
                                                .map(|s| s.types(db))
                                        {
                                            if !slice_ty.is_assignable_to(db, *expected_key_ty) {
                                                diagnostic.annotate(
                                                    self.context
                                                        .secondary(target.slice.as_ref())
                                                        .message(format_args!(
                                                            "Expected key of type `{}`, got `{}`",
                                                            expected_key_ty.display(db),
                                                            slice_ty.display(db),
                                                        )),
                                                );
                                            }

                                            if !rhs_value_ty
                                                .is_assignable_to(db, *expected_value_ty)
                                            {
                                                diagnostic.annotate(
                                                    self.context.secondary(rhs_value_node).message(
                                                        format_args!(
                                                            "Expected value of type `{}`, got `{}`",
                                                            expected_value_ty.display(db),
                                                            rhs_value_ty.display(db),
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
                                            "Method `__setitem__` of type `{}` may not be callable on object of type `{}`",
                                            bindings.callable_type().display(db),
                                            object_ty.display(db),
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
                                object_ty.display(db),
                            ));
                            attach_original_type_info(&mut diagnostic);

                            // If it's a user-defined class, suggest adding a `__setitem__` method.
                            if object_ty
                                .as_nominal_instance()
                                .and_then(|instance| instance.class(db).static_class_literal(db))
                                .and_then(|(class_literal, _)| {
                                    file_to_module(db, class_literal.file(db))
                                })
                                .and_then(|module| module.search_path(db))
                                .is_some_and(ty_module_resolver::SearchPath::is_first_party)
                            {
                                diagnostic.help(format_args!(
                                    "Consider adding a `__setitem__` method to `{}`.",
                                    object_ty.display(db),
                                ));
                            } else {
                                diagnostic.info(format_args!(
                                    "`{}` does not have a `__setitem__` method.",
                                    object_ty.display(db),
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
        let db = self.db();

        let attach_original_type_info = |diagnostic: &mut LintDiagnosticGuard| {
            if let Some(full_object_ty) = full_object_ty {
                diagnostic.info(format_args!(
                    "The full type of the subscripted object is `{}`",
                    full_object_ty.display(db)
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
                let mut any_valid = false;
                for element_ty in intersection.positive(db) {
                    if self.can_delete_subscript(*element_ty, slice_ty) {
                        any_valid = true;
                        break;
                    }
                }

                // If none are valid, emit a diagnostic for the first failing element
                if !any_valid && let Some(element_ty) = intersection.positive(db).first() {
                    self.validate_subscript_deletion_impl(
                        target,
                        full_object_ty.or(Some(object_ty)),
                        *element_ty,
                        slice_ty,
                    );
                }
            }

            _ => {
                match object_ty.try_call_dunder(
                    db,
                    "__delitem__",
                    CallArguments::positional([slice_ty]),
                    TypeContext::default(),
                ) {
                    Ok(_) => {}
                    Err(err) => match err {
                        CallDunderError::PossiblyUnbound { .. } => {
                            if let Some(builder) = self
                                .context
                                .report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, target)
                            {
                                let mut diagnostic = builder.into_diagnostic(format_args!(
                                    "Method `__delitem__` of type `{}` may be missing",
                                    object_ty.display(db),
                                ));
                                attach_original_type_info(&mut diagnostic);
                            }
                        }
                        CallDunderError::CallError(call_error_kind, bindings) => {
                            match call_error_kind {
                                CallErrorKind::NotCallable => {
                                    if let Some(builder) =
                                        self.context.report_lint(&CALL_NON_CALLABLE, target)
                                    {
                                        let mut diagnostic = builder.into_diagnostic(format_args!(
                                            "Method `__delitem__` of type `{}` is not callable \
                                             on object of type `{}`",
                                            bindings.callable_type().display(db),
                                            object_ty.display(db),
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
                                            if let Some(builder) = self
                                                .context
                                                .report_lint(&INVALID_ARGUMENT_TYPE, target)
                                            {
                                                let mut diagnostic = builder.into_diagnostic(format_args!(
                                                    "Method `__delitem__` of type `{}` cannot be called \
                                                     with key of type `{}` on object of type `{}`",
                                                    bindings.callable_type().display(db),
                                                    slice_ty.display(db),
                                                    object_ty.display(db),
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
                                                "Method `__delitem__` of type `{}` cannot be called \
                                                 with key of type `{}` on object of type `{}`",
                                                bindings.callable_type().display(db),
                                                slice_ty.display(db),
                                                object_ty.display(db),
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
                                            "Method `__delitem__` of type `{}` may not be callable \
                                             on object of type `{}`",
                                            bindings.callable_type().display(db),
                                            object_ty.display(db),
                                        ));
                                        attach_original_type_info(&mut diagnostic);
                                    }
                                }
                            }
                        }
                        CallDunderError::MethodNotAvailable => {
                            report_not_subscriptable(
                                &self.context,
                                target,
                                object_ty,
                                "__delitem__",
                            );
                        }
                    },
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

        for metadata_element in &arguments[1..] {
            self.infer_expression(metadata_element, TypeContext::default());
        }

        subscript_context.infer(self, first_argument)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyGenericContextError<'db> {
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
    TypeVarTupleMustBeUnpacked,
}

impl<'db> LegacyGenericContextError<'db> {
    const fn into_type(self) -> Type<'db> {
        match self {
            LegacyGenericContextError::InvalidArgument(_)
            | LegacyGenericContextError::VariadicTupleArguments
            | LegacyGenericContextError::DuplicateTypevar(_)
            | LegacyGenericContextError::TypeVarTupleMustBeUnpacked => Type::unknown(),
            LegacyGenericContextError::NotYetSupported => {
                todo_type!("ParamSpecs and TypeVarTuples")
            }
        }
    }
}

/// Validate the type arguments to `Generic[...]` or `Protocol[...]`, returning
/// either the resulting [`GenericContext`] or a [`SubscriptError`].
fn infer_legacy_generic_subscript<'db>(
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    file_scope_id: FileScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    slice_ty: Type<'db>,
    origin: LegacyGenericOrigin,
    wrap_ok: impl FnOnce(GenericContext<'db>) -> KnownInstanceType<'db>,
) -> Result<Type<'db>, SubscriptError<'db>> {
    match legacy_generic_class_context(db, index, file_scope_id, typevar_binding_context, slice_ty)
    {
        Ok(context) => Ok(Type::KnownInstance(wrap_ok(context))),
        Err(LegacyGenericContextError::InvalidArgument(argument_ty)) => Err(SubscriptError::new(
            Type::unknown(),
            SubscriptErrorKind::InvalidLegacyGenericArgument {
                origin,
                argument_ty,
            },
        )),
        Err(LegacyGenericContextError::DuplicateTypevar(typevar_name)) => Err(SubscriptError::new(
            Type::unknown(),
            SubscriptErrorKind::DuplicateTypevar {
                origin,
                typevar_name,
            },
        )),
        Err(LegacyGenericContextError::TypeVarTupleMustBeUnpacked) => Err(SubscriptError::new(
            Type::unknown(),
            SubscriptErrorKind::TypeVarTupleNotUnpacked { origin },
        )),
        Err(
            error @ (LegacyGenericContextError::NotYetSupported
            | LegacyGenericContextError::VariadicTupleArguments),
        ) => Ok(error.into_type()),
    }
}

/// Parse the type arguments to `Generic[...]` or `Protocol[...]` and validate
/// that each argument is a type variable.
fn legacy_generic_class_context<'db>(
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    file_scope_id: FileScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    typevars: Type<'db>,
) -> Result<GenericContext<'db>, LegacyGenericContextError<'db>> {
    let typevars_class_tuple_spec = typevars.exact_tuple_instance_spec(db);

    let typevars = if let Some(tuple_spec) = typevars_class_tuple_spec.as_deref() {
        match tuple_spec {
            Tuple::Fixed(typevars) => typevars.elements_slice(),
            Tuple::Variable(_) => {
                return Err(LegacyGenericContextError::VariadicTupleArguments);
            }
        }
    } else {
        std::slice::from_ref(&typevars)
    };

    let mut validated_typevars = FxOrderSet::default();
    for ty in typevars {
        let argument_ty = *ty;
        if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = argument_ty {
            let bound = bind_typevar(db, index, file_scope_id, typevar_binding_context, typevar)
                .ok_or(LegacyGenericContextError::InvalidArgument(argument_ty))?;
            if !validated_typevars.insert(bound) {
                return Err(LegacyGenericContextError::DuplicateTypevar(
                    typevar.name(db),
                ));
            }
        } else if let Type::NominalInstance(instance) = argument_ty
            && instance.has_known_class(db, KnownClass::TypeVarTuple)
        {
            return Err(LegacyGenericContextError::TypeVarTupleMustBeUnpacked);
        } else if any_over_type(db, argument_ty, true, |inner_ty| match inner_ty {
            Type::Dynamic(DynamicType::TodoUnpack | DynamicType::TodoStarredExpression) => true,
            Type::NominalInstance(nominal) => nominal.has_known_class(db, KnownClass::TypeVarTuple),
            _ => false,
        }) {
            return Err(LegacyGenericContextError::NotYetSupported);
        } else {
            return Err(LegacyGenericContextError::InvalidArgument(argument_ty));
        }
    }
    Ok(GenericContext::from_typevar_instances(
        db,
        validated_typevars,
    ))
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
