use itertools::{EitherOrBoth, Itertools};
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, ExprContext};
use ruff_text_size::Ranged;

use super::TypeInferenceBuilder;
use crate::place::{DefinedPlace, Definedness, Place};
use crate::semantic_index::SemanticIndex;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::place::{PlaceExpr, PlaceExprRef};
use crate::semantic_index::scope::FileScopeId;
use crate::types::call::bind::CallableDescription;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::diagnostic::{
    INVALID_TYPE_ARGUMENTS, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE,
    report_invalid_arguments_to_annotated,
};
use crate::types::generics::{GenericContext, InferableTypeVars, bind_typevar};
use crate::types::infer::InferenceFlags;
use crate::types::special_form::AliasSpec;
use crate::types::subscript::{LegacyGenericOrigin, SubscriptError, SubscriptErrorKind};
use crate::types::tuple::{Tuple, TupleType};
use crate::types::{
    BoundTypeVarInstance, DynamicType, InternedType, KnownClass, KnownInstanceType, Parameters,
    SpecialFormType, StaticClassLiteral, Type, TypeAliasType, TypeContext,
    TypeVarBoundOrConstraints, UnionType, UnionTypeInstance, any_over_type, todo_type,
};
use crate::{Db, FxOrderSet};

impl<'db> TypeInferenceBuilder<'db, '_> {
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
                let slice_ty = self.infer_expression(slice, TypeContext::default());
                self.infer_subscript_expression_types(subscript, value_ty, slice_ty, *ctx);
                Type::Never
            }
            ExprContext::Del => {
                let value_ty = self.infer_expression(value, TypeContext::default());
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
                    let ast::Expr::Tuple(ast::ExprTuple {
                        elts: ref arguments,
                        ..
                    }) = **slice
                    else {
                        report_invalid_arguments_to_annotated(&self.context, subscript);

                        return self.infer_expression(slice, TypeContext::default());
                    };

                    if arguments.len() < 2 {
                        report_invalid_arguments_to_annotated(&self.context, subscript);
                    }

                    let [type_expr, metadata @ ..] = &arguments[..] else {
                        for argument in arguments {
                            self.infer_expression(argument, TypeContext::default());
                        }
                        self.store_expression_type(slice, Type::unknown());
                        return Type::unknown();
                    };

                    for element in metadata {
                        self.infer_expression(element, TypeContext::default());
                    }

                    let ty = self.infer_type_expression(type_expr);

                    return Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
                        db, ty,
                    )));
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
                        let mut elements = tuple
                            .elts
                            .iter()
                            .map(|elt| self.infer_type_expression(elt))
                            .peekable();

                        let is_empty = elements.peek().is_none();
                        let union_type = Type::KnownInstance(KnownInstanceType::UnionType(
                            UnionTypeInstance::new(
                                db,
                                None,
                                Ok(UnionType::from_elements(db, elements)),
                            ),
                        ));

                        if is_empty
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
            .inference_flags
            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
        let result = self.infer_explicit_callable_specialization_impl(
            subscript,
            value_ty,
            generic_context,
            specialize,
        );
        self.inference_flags.set(
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
        let mut inferred_type_arguments = Vec::with_capacity(type_arguments.len());

        let typevars = generic_context.variables(db);
        let typevars_len = typevars.len();

        let mut specialization_types = Vec::with_capacity(typevars_len);
        let mut typevar_with_defaults = 0;
        let mut missing_typevars = vec![];
        let mut first_excess_type_argument_index = None;

        // Helper to get the AST node corresponding to the type argument at `index`.
        let get_node = |index: usize| -> ast::AnyNodeRef<'_> {
            match slice_node {
                ast::Expr::Tuple(ast::ExprTuple { elts, .. }) if !exactly_one_paramspec => elts
                    .get(index)
                    .expect("type argument index should not be out of range")
                    .into(),
                _ => slice_node.into(),
            }
        };

        let mut error: Option<ExplicitSpecializationError> = None;

        for (index, item) in typevars.zip_longest(type_arguments.iter()).enumerate() {
            match item {
                EitherOrBoth::Both(typevar, expr) => {
                    if typevar.default_type(db).is_some() {
                        typevar_with_defaults += 1;
                    }

                    let provided_type = if typevar.is_paramspec(db) {
                        self.infer_paramspec_explicit_specialization_value(
                            expr,
                            exactly_one_paramspec,
                        )
                        .unwrap_or_else(|()| {
                            error = Some(ExplicitSpecializationError::InvalidParamSpec);
                            Type::paramspec_value_callable(db, Parameters::unknown())
                        })
                    } else {
                        self.infer_type_expression(expr)
                    };

                    inferred_type_arguments.push(provided_type);

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
                                let node = get_node(index);
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node)
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
                                let node = get_node(index);
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node)
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
                EitherOrBoth::Right(expr) => {
                    inferred_type_arguments.push(self.infer_type_expression(expr));
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
                    if let Some(CallableDescription { kind, name }) = description {
                        format!(" of {kind} `{name}`")
                    } else {
                        String::new()
                    }
                ));
            }
            error = Some(ExplicitSpecializationError::MissingTypeVars);
        }

        if let Some(first_excess_type_argument_index) = first_excess_type_argument_index {
            if let Type::GenericAlias(alias) = value_ty
                && let spec = alias.specialization(db)
                && spec
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
                let node = get_node(first_excess_type_argument_index);
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_ARGUMENTS, node) {
                    let description = CallableDescription::new(db, value_ty);
                    builder.into_diagnostic(format_args!(
                        "Too many type arguments{}: expected {}, got {}",
                        if let Some(CallableDescription { kind, name }) = description {
                            format!(" to {kind} `{name}`")
                        } else {
                            String::new()
                        },
                        if typevar_with_defaults == 0 {
                            format!("{typevars_len}")
                        } else {
                            format!(
                                "between {} and {}",
                                typevars_len - typevar_with_defaults,
                                typevars_len
                            )
                        },
                        type_arguments.len(),
                    ));
                }
                error = Some(ExplicitSpecializationError::TooManyArguments);
            }
        }

        if store_inferred_type_arguments {
            self.store_expression_type(
                slice_node,
                Type::heterogeneous_tuple(db, inferred_type_arguments),
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
                | ExplicitSpecializationError::InvalidParamSpec,
            )
            | None => specialize(&specialization_types),
        }
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
