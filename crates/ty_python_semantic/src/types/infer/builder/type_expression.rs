use itertools::Either;
use ruff_python_ast as ast;

use super::{DeferredExpressionState, TypeInferenceBuilder};
use crate::FxOrderSet;
use crate::semantic_index::semantic_index;
use crate::types::diagnostic::{
    self, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE, report_invalid_argument_number_to_special_form,
    report_invalid_arguments_to_callable,
};
use crate::types::generics::bind_typevar;
use crate::types::infer::builder::InnerExpressionInferenceState;
use crate::types::signatures::Signature;
use crate::types::string_annotation::parse_string_annotation;
use crate::types::tuple::{TupleSpecBuilder, TupleType};
use crate::types::{
    BindingContext, CallableType, DynamicType, GenericContext, IntersectionBuilder, KnownClass,
    KnownInstanceType, LintDiagnosticGuard, Parameter, Parameters, SpecialFormType, SubclassOfType,
    Type, TypeAliasType, TypeContext, TypeIsType, TypeMapping, TypeVarKind, UnionBuilder,
    UnionType, any_over_type, todo_type,
};

/// Type expressions
impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer the type of a type expression.
    pub(super) fn infer_type_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        if self.inner_expression_inference_state.is_get() {
            return self.expression_type(expression);
        }
        let previous_deferred_state = self.deferred_state;

        // `DeferredExpressionState::InStringAnnotation` takes precedence over other states.
        // However, if it's not a stringified annotation, we must still ensure that annotation expressions
        // are always deferred in stub files.
        match previous_deferred_state {
            DeferredExpressionState::None => {
                if self.in_stub() {
                    self.deferred_state = DeferredExpressionState::Deferred;
                }
            }
            DeferredExpressionState::InStringAnnotation(_) | DeferredExpressionState::Deferred => {}
        }

        let ty = self.infer_type_expression_no_store(expression);
        self.deferred_state = previous_deferred_state;
        self.store_expression_type(expression, ty);
        ty
    }

    /// Similar to [`infer_type_expression`], but accepts a [`DeferredExpressionState`].
    ///
    /// [`infer_type_expression`]: TypeInferenceBuilder::infer_type_expression
    fn infer_type_expression_with_state(
        &mut self,
        expression: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> Type<'db> {
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, deferred_state);
        let annotation_ty = self.infer_type_expression(expression);
        self.deferred_state = previous_deferred_state;
        annotation_ty
    }

    /// Similar to [`infer_type_expression`], but accepts an optional expression.
    ///
    /// [`infer_type_expression`]: TypeInferenceBuilder::infer_type_expression_with_state
    pub(super) fn infer_optional_type_expression(
        &mut self,
        expression: Option<&ast::Expr>,
    ) -> Option<Type<'db>> {
        expression.map(|expr| self.infer_type_expression(expr))
    }

    fn report_invalid_type_expression(
        &self,
        expression: &ast::Expr,
        message: std::fmt::Arguments,
    ) -> Option<LintDiagnosticGuard<'_, '_>> {
        self.context
            .report_lint(&INVALID_TYPE_FORM, expression)
            .map(|builder| {
                diagnostic::add_type_expression_reference_link(builder.into_diagnostic(message))
            })
    }

    /// Infer the type of a type expression without storing the result.
    pub(super) fn infer_type_expression_no_store(&mut self, expression: &ast::Expr) -> Type<'db> {
        if self.inner_expression_inference_state.is_get() {
            return self.expression_type(expression);
        }
        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-type_expression
        match expression {
            ast::Expr::Name(name) => match name.ctx {
                ast::ExprContext::Load => self
                    .infer_name_expression(name)
                    .default_specialize(self.db())
                    .in_type_expression(self.db(), self.scope(), self.typevar_binding_context)
                    .unwrap_or_else(|error| {
                        error.into_fallback_type(
                            &self.context,
                            expression,
                            self.is_reachable(expression),
                        )
                    }),
                ast::ExprContext::Invalid => Type::unknown(),
                ast::ExprContext::Store | ast::ExprContext::Del => {
                    todo_type!("Name expression annotation in Store/Del context")
                }
            },

            ast::Expr::Attribute(attribute_expression) => match attribute_expression.ctx {
                ast::ExprContext::Load => self
                    .infer_attribute_expression(attribute_expression)
                    .default_specialize(self.db())
                    .in_type_expression(self.db(), self.scope(), self.typevar_binding_context)
                    .unwrap_or_else(|error| {
                        error.into_fallback_type(
                            &self.context,
                            expression,
                            self.is_reachable(expression),
                        )
                    }),
                ast::ExprContext::Invalid => Type::unknown(),
                ast::ExprContext::Store | ast::ExprContext::Del => {
                    todo_type!("Attribute expression annotation in Store/Del context")
                }
            },

            ast::Expr::NoneLiteral(_literal) => Type::none(self.db()),

            // https://typing.python.org/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(string) => self.infer_string_type_expression(string),

            ast::Expr::Subscript(subscript) => {
                let ast::ExprSubscript {
                    value,
                    slice,
                    ctx: _,
                    range: _,
                    node_index: _,
                } = subscript;

                let value_ty = self.infer_expression(value, TypeContext::default());

                self.infer_subscript_type_expression_no_store(subscript, slice, value_ty)
            }

            ast::Expr::BinOp(binary) => {
                match binary.op {
                    // PEP-604 unions are okay, e.g., `int | str`
                    ast::Operator::BitOr => {
                        let left_ty = self.infer_type_expression(&binary.left);
                        let right_ty = self.infer_type_expression(&binary.right);
                        UnionType::from_elements_leave_aliases(self.db(), [left_ty, right_ty])
                    }
                    // anything else is an invalid annotation:
                    op => {
                        // Avoid inferring the types of invalid binary expressions that have been
                        // parsed from a string annotation, as they are not present in the semantic
                        // index.
                        if !self.deferred_state.in_string_annotation() {
                            self.infer_binary_expression(binary, TypeContext::default());
                        }
                        self.report_invalid_type_expression(
                            expression,
                            format_args!(
                                "Invalid binary operator `{}` in type annotation",
                                op.as_str()
                            ),
                        );
                        Type::unknown()
                    }
                }
            }

            // Avoid inferring the types of invalid type expressions that have been parsed from a
            // string annotation, as they are not present in the semantic index.
            _ if self.deferred_state.in_string_annotation() => Type::unknown(),

            // =====================================================================================
            // Forms which are invalid in the context of annotation expressions: we infer their
            // nested expressions as normal expressions, but the type of the top-level expression is
            // always `Type::unknown` in these cases.
            // =====================================================================================

            // TODO: add a subdiagnostic linking to type-expression grammar
            // and stating that it is only valid in `typing.Literal[]` or `typing.Annotated[]`
            ast::Expr::BytesLiteral(_) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Bytes literals are not allowed in this context in a type expression"
                    ),
                );
                Type::unknown()
            }

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(_),
                ..
            }) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Int literals are not allowed in this context in a type expression"
                    ),
                );

                Type::unknown()
            }

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Float(_),
                ..
            }) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Float literals are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Complex { .. },
                ..
            }) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Complex literals are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::BooleanLiteral(_) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Boolean literals are not allowed in this context in a type expression"
                    ),
                );
                Type::unknown()
            }

            ast::Expr::List(list) => {
                let db = self.db();

                let inner_types: Vec<Type<'db>> = list
                    .iter()
                    .map(|element| self.infer_type_expression(element))
                    .collect();

                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "List literals are not allowed in this context in a type expression"
                    ),
                ) {
                    if !inner_types.iter().any(|ty| {
                        matches!(
                            ty,
                            Type::Dynamic(DynamicType::Todo(_) | DynamicType::Unknown)
                        )
                    }) {
                        let hinted_type = if list.len() == 1 {
                            KnownClass::List.to_specialized_instance(db, inner_types)
                        } else {
                            Type::heterogeneous_tuple(db, inner_types)
                        };

                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(self.db()),
                        ));
                    }
                }
                Type::unknown()
            }

            ast::Expr::Tuple(tuple) => {
                let inner_types: Vec<Type<'db>> = tuple
                    .elts
                    .iter()
                    .map(|expr| self.infer_type_expression(expr))
                    .collect();

                if tuple.parenthesized {
                    if let Some(mut diagnostic) = self.report_invalid_type_expression(
                        expression,
                        format_args!(
                            "Tuple literals are not allowed in this context in a type expression"
                        ),
                    ) {
                        if !inner_types.iter().any(|ty| {
                            matches!(
                                ty,
                                Type::Dynamic(DynamicType::Todo(_) | DynamicType::Unknown)
                            )
                        }) {
                            let hinted_type = Type::heterogeneous_tuple(self.db(), inner_types);
                            diagnostic.set_primary_message(format_args!(
                                "Did you mean `{}`?",
                                hinted_type.display(self.db()),
                            ));
                        }
                    }
                }
                Type::unknown()
            }

            ast::Expr::BoolOp(bool_op) => {
                self.infer_boolean_expression(bool_op);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Boolean operations are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Named(named) => {
                self.infer_named_expression(named);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Named expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::UnaryOp(unary) => {
                self.infer_unary_expression(unary);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Unary operations are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Lambda(lambda_expression) => {
                self.infer_lambda_expression(lambda_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`lambda` expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::If(if_expression) => {
                self.infer_if_expression(if_expression, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`if` expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Dict(dict) => {
                self.infer_dict_expression(dict, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Dict literals are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Set(set) => {
                self.infer_set_expression(set, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Set literals are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::DictComp(dictcomp) => {
                self.infer_dict_comprehension_expression(dictcomp, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Dict comprehensions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::ListComp(listcomp) => {
                self.infer_list_comprehension_expression(listcomp, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("List comprehensions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::SetComp(setcomp) => {
                self.infer_set_comprehension_expression(setcomp, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Set comprehensions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Generator(generator) => {
                self.infer_generator_expression(generator);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Generator expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Await(await_expression) => {
                self.infer_await_expression(await_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`await` expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Yield(yield_expression) => {
                self.infer_yield_expression(yield_expression);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`yield` expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::YieldFrom(yield_from) => {
                self.infer_yield_from_expression(yield_from);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("`yield from` expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Compare(compare) => {
                self.infer_compare_expression(compare);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Comparison expressions are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Call(call_expr) => {
                self.infer_call_expression(call_expr, TypeContext::default());
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Function calls are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::FString(fstring) => {
                self.infer_fstring_expression(fstring);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("F-strings are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::TString(tstring) => {
                self.infer_tstring_expression(tstring);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("T-strings are not allowed in type expressions"),
                );
                Type::unknown()
            }

            ast::Expr::Slice(slice) => {
                self.infer_slice_expression(slice);
                self.report_invalid_type_expression(
                    expression,
                    format_args!("Slices are not allowed in type expressions"),
                );
                Type::unknown()
            }

            // =================================================================================
            // Branches where we probably should emit diagnostics in some context, but don't yet
            // =================================================================================
            // TODO: When this case is implemented and the `todo!` usage
            // is removed, consider adding `todo = "warn"` to the Clippy
            // lint configuration in `Cargo.toml`. At time of writing,
            // 2025-08-22, this was the only usage of `todo!` in ruff/ty.
            // ---AG
            ast::Expr::IpyEscapeCommand(_) => todo!("Implement Ipy escape command support"),

            ast::Expr::EllipsisLiteral(_) => {
                todo_type!("ellipsis literal in type expression")
            }

            ast::Expr::Starred(starred) => self.infer_starred_type_expression(starred),
        }
    }

    fn infer_starred_type_expression(&mut self, starred: &ast::ExprStarred) -> Type<'db> {
        let ast::ExprStarred {
            range: _,
            node_index: _,
            value,
            ctx: _,
        } = starred;

        let starred_type = self.infer_type_expression(value);
        if starred_type.exact_tuple_instance_spec(self.db()).is_some() {
            starred_type
        } else {
            Type::Dynamic(DynamicType::TodoStarredExpression)
        }
    }

    pub(super) fn infer_subscript_type_expression_no_store(
        &mut self,
        subscript: &ast::ExprSubscript,
        slice: &ast::Expr,
        value_ty: Type<'db>,
    ) -> Type<'db> {
        match value_ty {
            Type::ClassLiteral(class_literal) => match class_literal.known(self.db()) {
                Some(KnownClass::Tuple) => Type::tuple(self.infer_tuple_type_expression(slice)),
                Some(KnownClass::Type) => self.infer_subclass_of_type_expression(slice),
                _ => self.infer_subscript_type_expression(subscript, value_ty),
            },
            _ => self.infer_subscript_type_expression(subscript, value_ty),
        }
    }

    /// Infer the type of a string type expression.
    pub(super) fn infer_string_type_expression(
        &mut self,
        string: &ast::ExprStringLiteral,
    ) -> Type<'db> {
        match parse_string_annotation(&self.context, string) {
            Some(parsed) => {
                self.string_annotations
                    .insert(ruff_python_ast::ExprRef::StringLiteral(string).into());
                // String annotations are always evaluated in the deferred context.
                self.infer_type_expression_with_state(
                    parsed.expr(),
                    DeferredExpressionState::InStringAnnotation(
                        self.enclosing_node_key(string.into()),
                    ),
                )
            }
            None => Type::unknown(),
        }
    }

    /// Given the slice of a `tuple[]` annotation, return the type that the annotation represents
    pub(super) fn infer_tuple_type_expression(
        &mut self,
        tuple_slice: &ast::Expr,
    ) -> Option<TupleType<'db>> {
        /// In most cases, if a subelement of the tuple is inferred as `Todo`,
        /// we should only infer `Todo` for that specific subelement.
        /// Certain specific AST nodes can however change the meaning of the entire tuple,
        /// however: for example, `tuple[int, ...]` or `tuple[int, *tuple[str, ...]]` are a
        /// homogeneous tuple and a partly homogeneous tuple (respectively) due to the `...`
        /// and the starred expression (respectively), Neither is supported by us right now,
        /// so we should infer `Todo` for the *entire* tuple if we encounter one of those elements.
        fn element_could_alter_type_of_whole_tuple(
            element: &ast::Expr,
            element_ty: Type,
            builder: &mut TypeInferenceBuilder,
        ) -> bool {
            if !element_ty.is_todo() {
                return false;
            }

            match element {
                ast::Expr::Starred(_) => {
                    element_ty.exact_tuple_instance_spec(builder.db()).is_none()
                }
                ast::Expr::Subscript(ast::ExprSubscript { value, .. }) => {
                    let value_ty = if builder.deferred_state.in_string_annotation() {
                        // Using `.expression_type` does not work in string annotations, because
                        // we do not store types for sub-expressions. Re-infer the type here.
                        builder.infer_expression(value, TypeContext::default())
                    } else {
                        builder.expression_type(value)
                    };

                    value_ty == Type::SpecialForm(SpecialFormType::Unpack)
                }
                _ => false,
            }
        }

        // TODO: PEP 646
        match tuple_slice {
            ast::Expr::Tuple(elements) => {
                if let [element, ellipsis @ ast::Expr::EllipsisLiteral(_)] = &*elements.elts {
                    self.infer_expression(ellipsis, TypeContext::default());
                    let result =
                        TupleType::homogeneous(self.db(), self.infer_type_expression(element));
                    self.store_expression_type(tuple_slice, Type::tuple(Some(result)));
                    return Some(result);
                }

                let mut element_types = TupleSpecBuilder::with_capacity(elements.len());

                // Whether to infer `Todo` for the whole tuple
                // (see docstring for `element_could_alter_type_of_whole_tuple`)
                let mut return_todo = false;

                for element in elements {
                    let element_ty = self.infer_type_expression(element);
                    return_todo |=
                        element_could_alter_type_of_whole_tuple(element, element_ty, self);

                    if let ast::Expr::Starred(_) = element {
                        if let Some(inner_tuple) = element_ty.exact_tuple_instance_spec(self.db()) {
                            element_types = element_types.concat(self.db(), &inner_tuple);
                        } else {
                            // TODO: emit a diagnostic
                        }
                    } else {
                        element_types.push(element_ty);
                    }
                }

                let ty = if return_todo {
                    Some(TupleType::homogeneous(self.db(), todo_type!("PEP 646")))
                } else {
                    TupleType::new(self.db(), &element_types.build())
                };

                // Here, we store the type for the inner `int, str` tuple-expression,
                // while the type for the outer `tuple[int, str]` slice-expression is
                // stored in the surrounding `infer_type_expression` call:
                self.store_expression_type(tuple_slice, Type::tuple(ty));

                ty
            }
            single_element => {
                let single_element_ty = self.infer_type_expression(single_element);
                if element_could_alter_type_of_whole_tuple(single_element, single_element_ty, self)
                {
                    Some(TupleType::homogeneous(self.db(), todo_type!("PEP 646")))
                } else {
                    TupleType::heterogeneous(self.db(), std::iter::once(single_element_ty))
                }
            }
        }
    }

    /// Given the slice of a `type[]` annotation, return the type that the annotation represents
    fn infer_subclass_of_type_expression(&mut self, slice: &ast::Expr) -> Type<'db> {
        match slice {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                SubclassOfType::try_from_instance(self.db(), self.infer_type_expression(slice))
                    .unwrap_or(todo_type!("unsupported type[X] special form"))
            }
            ast::Expr::BinOp(binary) if binary.op == ast::Operator::BitOr => {
                let union_ty = UnionType::from_elements_leave_aliases(
                    self.db(),
                    [
                        self.infer_subclass_of_type_expression(&binary.left),
                        self.infer_subclass_of_type_expression(&binary.right),
                    ],
                );
                self.store_expression_type(slice, union_ty);

                union_ty
            }
            ast::Expr::Tuple(_) => {
                self.infer_type_expression(slice);
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, slice) {
                    builder.into_diagnostic("type[...] must have exactly one type argument");
                }
                Type::unknown()
            }
            ast::Expr::Subscript(
                subscript @ ast::ExprSubscript {
                    value,
                    slice: parameters,
                    ..
                },
            ) => {
                let parameters_ty = match self.infer_expression(value, TypeContext::default()) {
                    Type::SpecialForm(SpecialFormType::Union) => match &**parameters {
                        ast::Expr::Tuple(tuple) => {
                            let ty = UnionType::from_elements_leave_aliases(
                                self.db(),
                                tuple
                                    .iter()
                                    .map(|element| self.infer_subclass_of_type_expression(element)),
                            );
                            self.store_expression_type(parameters, ty);
                            ty
                        }
                        _ => self.infer_subclass_of_type_expression(parameters),
                    },
                    value_ty @ Type::ClassLiteral(class_literal) => {
                        if class_literal.is_protocol(self.db()) {
                            SubclassOfType::from(
                                self.db(),
                                todo_type!("type[T] for protocols").expect_dynamic(),
                            )
                        } else if class_literal.is_tuple(self.db()) {
                            let class_type = self
                                .infer_tuple_type_expression(parameters)
                                .map(|tuple_type| tuple_type.to_class_type(self.db()))
                                .unwrap_or_else(|| class_literal.default_specialization(self.db()));
                            SubclassOfType::from(self.db(), class_type)
                        } else {
                            match class_literal.generic_context(self.db()) {
                                Some(generic_context) => {
                                    let db = self.db();
                                    let specialize = |types: &[Option<Type<'db>>]| {
                                        SubclassOfType::from(
                                            db,
                                            class_literal.apply_specialization(db, |_| {
                                                generic_context
                                                    .specialize_partial(db, types.iter().copied())
                                            }),
                                        )
                                    };
                                    self.infer_explicit_callable_specialization(
                                        subscript,
                                        value_ty,
                                        generic_context,
                                        specialize,
                                    )
                                }
                                None => {
                                    // TODO: emit a diagnostic if you try to specialize a non-generic class.
                                    self.infer_type_expression(parameters);
                                    todo_type!("specialized non-generic class")
                                }
                            }
                        }
                    }
                    _ => {
                        self.infer_type_expression(parameters);
                        todo_type!("unsupported nested subscript in type[X]")
                    }
                };
                self.store_expression_type(slice, parameters_ty);
                parameters_ty
            }
            // TODO: subscripts, etc.
            _ => {
                self.infer_type_expression(slice);
                todo_type!("unsupported type[X] special form")
            }
        }
    }

    /// Infer the type of an explicitly specialized generic type alias (implicit or PEP 613).
    pub(crate) fn infer_explicit_type_alias_specialization(
        &mut self,
        subscript: &ast::ExprSubscript,
        mut value_ty: Type<'db>,
        in_type_expression: bool,
    ) -> Type<'db> {
        let db = self.db();

        if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = value_ty
            && let Some(definition) = typevar.definition(db)
        {
            value_ty = value_ty.apply_type_mapping(
                db,
                &TypeMapping::BindLegacyTypevars(BindingContext::Definition(definition)),
                TypeContext::default(),
            );
        }

        let mut variables = FxOrderSet::default();
        value_ty.find_legacy_typevars(db, None, &mut variables);
        let generic_context = GenericContext::from_typevar_instances(db, variables);

        let scope_id = self.scope();
        let current_typevar_binding_context = self.typevar_binding_context;

        // TODO
        // If we explicitly specialize a recursive generic (PEP-613 or implicit) type alias,
        // we currently miscount the number of type variables. For example, for a nested
        // dictionary type alias `NestedDict = dict[K, "V | NestedDict[K, V]"]]`, we might
        // infer `<class 'dict[K, Divergent]'>`, and therefore count just one type variable
        // instead of two. So until we properly support these, specialize all remaining type
        // variables with a `@Todo` type (since we don't know which of the type arguments
        // belongs to the remaining type variables).
        if any_over_type(self.db(), value_ty, &|ty| ty.is_divergent(), true) {
            let value_ty = value_ty.apply_specialization(
                db,
                generic_context.specialize(
                    db,
                    std::iter::repeat_n(
                        todo_type!("specialized recursive generic type alias"),
                        generic_context.len(db),
                    )
                    .collect(),
                ),
            );
            return if in_type_expression {
                value_ty
                    .in_type_expression(db, scope_id, current_typevar_binding_context)
                    .unwrap_or_else(|_| Type::unknown())
            } else {
                value_ty
            };
        }

        let specialize = |types: &[Option<Type<'db>>]| {
            let specialized = value_ty.apply_specialization(
                db,
                generic_context.specialize_partial(db, types.iter().copied()),
            );

            if in_type_expression {
                specialized
                    .in_type_expression(db, scope_id, current_typevar_binding_context)
                    .unwrap_or_else(|_| Type::unknown())
            } else {
                specialized
            }
        };

        self.infer_explicit_callable_specialization(
            subscript,
            value_ty,
            generic_context,
            specialize,
        )
    }

    pub(super) fn infer_subscript_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Type<'db> {
        let ast::ExprSubscript {
            range: _,
            node_index: _,
            value: _,
            slice,
            ctx: _,
        } = subscript;

        match value_ty {
            Type::Never => {
                // This case can be entered when we use a type annotation like `Literal[1]`
                // in unreachable code, since we infer `Never` for `Literal`.  We call
                // `infer_expression` (instead of `infer_type_expression`) here to avoid
                // false-positive `invalid-type-form` diagnostics (`1` is not a valid type
                // expression).
                self.infer_expression(slice, TypeContext::default());
                Type::unknown()
            }
            Type::SpecialForm(special_form) => {
                self.infer_parameterized_special_form_type_expression(subscript, special_form)
            }
            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::SubscriptedProtocol(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.Protocol` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::SubscriptedGeneric(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.Generic` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Deprecated(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`warnings.deprecated` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Field(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`dataclasses.Field` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::ConstraintSet(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions.ConstraintSet` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::GenericContext(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions.GenericContext` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Specialization(_) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions.Specialization` is not allowed in type expressions",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::TypeAliasType(type_alias @ TypeAliasType::PEP695(_)) => {
                    if type_alias.specialization(self.db()).is_some() {
                        if let Some(builder) =
                            self.context.report_lint(&NOT_SUBSCRIPTABLE, subscript)
                        {
                            let mut diagnostic =
                                builder.into_diagnostic("Cannot subscript non-generic type alias");
                            diagnostic.set_primary_message("Double specialization is not allowed");
                        }
                        return Type::unknown();
                    }
                    match type_alias.generic_context(self.db()) {
                        Some(generic_context) => {
                            let specialized_type_alias = self
                                .infer_explicit_type_alias_type_specialization(
                                    subscript,
                                    value_ty,
                                    type_alias,
                                    generic_context,
                                );

                            specialized_type_alias
                                .in_type_expression(
                                    self.db(),
                                    self.scope(),
                                    self.typevar_binding_context,
                                )
                                .unwrap_or(Type::unknown())
                        }
                        None => {
                            self.infer_type_expression(slice);

                            if let Some(builder) =
                                self.context.report_lint(&NOT_SUBSCRIPTABLE, subscript)
                            {
                                let value_type = type_alias.raw_value_type(self.db());
                                let mut diagnostic = builder
                                    .into_diagnostic("Cannot subscript non-generic type alias");
                                if value_type.is_definition_generic(self.db()) {
                                    diagnostic.set_primary_message(format_args!(
                                        "`{}` is already specialized",
                                        value_type.display(self.db()),
                                    ));
                                }
                            }

                            Type::unknown()
                        }
                    }
                }
                KnownInstanceType::TypeAliasType(TypeAliasType::ManualPEP695(_)) => {
                    // TODO: support generic "manual" PEP 695 type aliases
                    let slice_ty = self.infer_expression(slice, TypeContext::default());
                    let mut variables = FxOrderSet::default();
                    slice_ty.bind_and_find_all_legacy_typevars(
                        self.db(),
                        self.typevar_binding_context,
                        &mut variables,
                    );
                    let generic_context =
                        GenericContext::from_typevar_instances(self.db(), variables);
                    Type::Dynamic(DynamicType::UnknownGeneric(generic_context))
                }
                KnownInstanceType::LiteralStringAlias(_) => {
                    self.infer_type_expression(slice);
                    todo_type!("Generic stringified PEP-613 type alias")
                }
                KnownInstanceType::Literal(ty) => {
                    self.infer_type_expression(slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`{ty}` is not a generic class",
                            ty = ty.inner(self.db()).display(self.db())
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::TypeVar(typevar) => {
                    // The type variable designated as a generic type alias by `typing.TypeAlias` can be explicitly specialized.
                    // ```py
                    // from typing import TypeVar, TypeAlias
                    // T = TypeVar('T')
                    // Annotated: TypeAlias = T
                    // _: Annotated[int] = 1  # valid
                    // ```
                    if typevar.identity(self.db()).kind(self.db()) == TypeVarKind::Pep613Alias {
                        self.infer_explicit_type_alias_specialization(subscript, value_ty, false)
                    } else {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                        {
                            builder.into_diagnostic(format_args!(
                                "A type variable itself cannot be specialized",
                            ));
                        }
                        Type::unknown()
                    }
                }

                KnownInstanceType::UnionType(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::Annotated(_)
                | KnownInstanceType::TypeGenericAlias(_) => {
                    self.infer_explicit_type_alias_specialization(subscript, value_ty, true)
                }
                KnownInstanceType::NewType(newtype) => {
                    self.infer_type_expression(&subscript.slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`{}` is a `NewType` and cannot be specialized",
                            newtype.name(self.db())
                        ));
                    }
                    Type::unknown()
                }
            },
            Type::Dynamic(DynamicType::UnknownGeneric(_)) => {
                self.infer_explicit_type_alias_specialization(subscript, value_ty, true)
            }
            Type::Dynamic(_) => {
                // Infer slice as a value expression to avoid false-positive
                // `invalid-type-form` diagnostics, when we have e.g.
                // `MyCallable[[int, str], None]` but `MyCallable` is dynamic.
                self.infer_expression(slice, TypeContext::default());
                value_ty
            }
            Type::ClassLiteral(class) => {
                match class.generic_context(self.db()) {
                    Some(generic_context) => {
                        let specialized_class = self.infer_explicit_class_specialization(
                            subscript,
                            value_ty,
                            class,
                            generic_context,
                        );

                        specialized_class
                            .in_type_expression(
                                self.db(),
                                self.scope(),
                                self.typevar_binding_context,
                            )
                            .unwrap_or(Type::unknown())
                    }
                    None => {
                        // TODO: emit a diagnostic if you try to specialize a non-generic class.
                        self.infer_type_expression(slice);
                        todo_type!("specialized non-generic class")
                    }
                }
            }
            Type::GenericAlias(_) => {
                self.infer_explicit_type_alias_specialization(subscript, value_ty, true)
            }
            Type::StringLiteral(_) => {
                self.infer_type_expression(slice);
                // For stringified TypeAlias; remove once properly supported
                todo_type!("string literal subscripted in type expression")
            }
            Type::Union(union) => {
                self.infer_type_expression(slice);
                let previous_slice_inference_state = std::mem::replace(
                    &mut self.inner_expression_inference_state,
                    InnerExpressionInferenceState::Get,
                );
                let union = union
                    .elements(self.db())
                    .iter()
                    .fold(UnionBuilder::new(self.db()), |builder, elem| {
                        builder.add(self.infer_subscript_type_expression(subscript, *elem))
                    })
                    .build();
                self.inner_expression_inference_state = previous_slice_inference_state;
                union
            }
            _ => {
                self.infer_type_expression(slice);
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Invalid subscript of object of type `{}` in type expression",
                        value_ty.display(self.db())
                    ));
                }
                Type::unknown()
            }
        }
    }

    fn infer_parameterized_legacy_typing_alias(
        &mut self,
        subscript_node: &ast::ExprSubscript,
        expected_arg_count: usize,
        alias: SpecialFormType,
        class: KnownClass,
    ) -> Type<'db> {
        let arguments = &*subscript_node.slice;
        let args = if let ast::Expr::Tuple(t) = arguments {
            &*t.elts
        } else {
            std::slice::from_ref(arguments)
        };
        if args.len() != expected_arg_count {
            if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript_node) {
                let noun = if expected_arg_count == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                builder.into_diagnostic(format_args!(
                    "Legacy alias `{alias}` expected exactly {expected_arg_count} {noun}, \
                    got {}",
                    args.len()
                ));
            }
        }
        let ty = class.to_specialized_instance(
            self.db(),
            args.iter().map(|node| self.infer_type_expression(node)),
        );
        if arguments.is_tuple_expr() {
            self.store_expression_type(arguments, ty);
        }
        ty
    }

    /// Infer the type of a `Callable[...]` type expression.
    pub(crate) fn infer_callable_type(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        let db = self.db();

        let arguments_slice = &*subscript.slice;

        let mut arguments = match arguments_slice {
            ast::Expr::Tuple(tuple) => Either::Left(tuple.iter()),
            _ => {
                self.infer_callable_parameter_types(arguments_slice);
                Either::Right(std::iter::empty::<&ast::Expr>())
            }
        };

        let first_argument = arguments.next();

        let parameters = first_argument.and_then(|arg| self.infer_callable_parameter_types(arg));

        let return_type = arguments.next().map(|arg| self.infer_type_expression(arg));

        let correct_argument_number = if let Some(third_argument) = arguments.next() {
            self.infer_type_expression(third_argument);
            for argument in arguments {
                self.infer_type_expression(argument);
            }
            false
        } else {
            return_type.is_some()
        };

        if !correct_argument_number {
            report_invalid_arguments_to_callable(&self.context, subscript);
        }

        let callable_type = if let (Some(parameters), Some(return_type), true) =
            (parameters, return_type, correct_argument_number)
        {
            Type::single_callable(db, Signature::new(parameters, Some(return_type)))
        } else {
            Type::Callable(CallableType::unknown(db))
        };

        // `Signature` / `Parameters` are not a `Type` variant, so we're storing
        // the outer callable type on these expressions instead.
        self.store_expression_type(arguments_slice, callable_type);
        if let Some(first_argument) = first_argument {
            self.store_expression_type(first_argument, callable_type);
        }

        callable_type
    }

    pub(crate) fn infer_parameterized_special_form_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        special_form: SpecialFormType,
    ) -> Type<'db> {
        let db = self.db();
        let arguments_slice = &*subscript.slice;
        match special_form {
            SpecialFormType::Annotated => {
                let ty = self
                    .infer_subscript_load_impl(
                        Type::SpecialForm(SpecialFormType::Annotated),
                        subscript,
                    )
                    .in_type_expression(db, self.scope(), None)
                    .unwrap_or_else(|err| err.into_fallback_type(&self.context, subscript, true));
                // Only store on the tuple slice; non-tuple cases are handled by
                // `infer_subscript_load_impl` via `infer_expression`.
                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, ty);
                }
                ty
            }
            SpecialFormType::Literal => match self.infer_literal_parameter_type(arguments_slice) {
                Ok(ty) => ty,
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
                    Type::unknown()
                }
            },
            SpecialFormType::Optional => {
                let param_type = self.infer_type_expression(arguments_slice);
                UnionType::from_elements_leave_aliases(db, [param_type, Type::none(db)])
            }
            SpecialFormType::Union => match arguments_slice {
                ast::Expr::Tuple(t) => {
                    let union_ty = UnionType::from_elements_leave_aliases(
                        db,
                        t.iter().map(|elt| self.infer_type_expression(elt)),
                    );
                    self.store_expression_type(arguments_slice, union_ty);
                    union_ty
                }
                _ => self.infer_type_expression(arguments_slice),
            },
            SpecialFormType::Callable => self.infer_callable_type(subscript),

            // `ty_extensions` special forms
            SpecialFormType::Not => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();
                let negated_type = if num_arguments == 1 {
                    self.infer_type_expression(&arguments[0]).negate(db)
                } else {
                    for argument in arguments {
                        self.infer_type_expression(argument);
                    }
                    report_invalid_argument_number_to_special_form(
                        &self.context,
                        subscript,
                        special_form,
                        num_arguments,
                        1,
                    );
                    Type::unknown()
                };
                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, negated_type);
                }
                negated_type
            }
            SpecialFormType::Intersection => {
                let elements = match arguments_slice {
                    ast::Expr::Tuple(tuple) => Either::Left(tuple.iter()),
                    element => Either::Right(std::iter::once(element)),
                };

                let ty = elements
                    .fold(IntersectionBuilder::new(db), |builder, element| {
                        builder.add_positive(self.infer_type_expression(element))
                    })
                    .build();

                if matches!(arguments_slice, ast::Expr::Tuple(_)) {
                    self.store_expression_type(arguments_slice, ty);
                }
                ty
            }
            SpecialFormType::Top => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();
                let arg = if num_arguments == 1 {
                    self.infer_type_expression(&arguments[0])
                } else {
                    for argument in arguments {
                        self.infer_type_expression(argument);
                    }
                    report_invalid_argument_number_to_special_form(
                        &self.context,
                        subscript,
                        special_form,
                        num_arguments,
                        1,
                    );
                    Type::unknown()
                };
                arg.top_materialization(db)
            }
            SpecialFormType::Bottom => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();
                let arg = if num_arguments == 1 {
                    self.infer_type_expression(&arguments[0])
                } else {
                    for argument in arguments {
                        self.infer_type_expression(argument);
                    }
                    report_invalid_argument_number_to_special_form(
                        &self.context,
                        subscript,
                        special_form,
                        num_arguments,
                        1,
                    );
                    Type::unknown()
                };
                arg.bottom_materialization(db)
            }
            SpecialFormType::TypeOf => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();
                let type_of_type = if num_arguments == 1 {
                    // N.B. This uses `infer_expression` rather than `infer_type_expression`
                    self.infer_expression(&arguments[0], TypeContext::default())
                } else {
                    for argument in arguments {
                        self.infer_type_expression(argument);
                    }
                    report_invalid_argument_number_to_special_form(
                        &self.context,
                        subscript,
                        special_form,
                        num_arguments,
                        1,
                    );
                    Type::unknown()
                };
                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, type_of_type);
                }
                type_of_type
            }

            SpecialFormType::CallableTypeOf => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();

                if num_arguments != 1 {
                    for argument in arguments {
                        self.infer_expression(argument, TypeContext::default());
                    }
                    report_invalid_argument_number_to_special_form(
                        &self.context,
                        subscript,
                        special_form,
                        num_arguments,
                        1,
                    );
                    if arguments_slice.is_tuple_expr() {
                        self.store_expression_type(arguments_slice, Type::unknown());
                    }
                    return Type::unknown();
                }

                let argument_type = self.infer_expression(&arguments[0], TypeContext::default());

                let Some(callable_type) = argument_type
                    .try_upcast_to_callable(db)
                    .map(|callables| callables.into_type(self.db()))
                else {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_TYPE_FORM, arguments_slice)
                    {
                        builder.into_diagnostic(format_args!(
                            "Expected the first argument to `{special_form}` \
                                 to be a callable object, \
                                 but got an object of type `{actual_type}`",
                            actual_type = argument_type.display(db)
                        ));
                    }
                    if arguments_slice.is_tuple_expr() {
                        self.store_expression_type(arguments_slice, Type::unknown());
                    }
                    return Type::unknown();
                };

                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, callable_type);
                }
                callable_type
            }

            SpecialFormType::ChainMap => self.infer_parameterized_legacy_typing_alias(
                subscript,
                2,
                SpecialFormType::ChainMap,
                KnownClass::ChainMap,
            ),
            SpecialFormType::OrderedDict => self.infer_parameterized_legacy_typing_alias(
                subscript,
                2,
                SpecialFormType::OrderedDict,
                KnownClass::OrderedDict,
            ),
            SpecialFormType::Dict => self.infer_parameterized_legacy_typing_alias(
                subscript,
                2,
                SpecialFormType::Dict,
                KnownClass::Dict,
            ),
            SpecialFormType::List => self.infer_parameterized_legacy_typing_alias(
                subscript,
                1,
                SpecialFormType::List,
                KnownClass::List,
            ),
            SpecialFormType::DefaultDict => self.infer_parameterized_legacy_typing_alias(
                subscript,
                2,
                SpecialFormType::DefaultDict,
                KnownClass::DefaultDict,
            ),
            SpecialFormType::Counter => self.infer_parameterized_legacy_typing_alias(
                subscript,
                1,
                SpecialFormType::Counter,
                KnownClass::Counter,
            ),
            SpecialFormType::Set => self.infer_parameterized_legacy_typing_alias(
                subscript,
                1,
                SpecialFormType::Set,
                KnownClass::Set,
            ),
            SpecialFormType::FrozenSet => self.infer_parameterized_legacy_typing_alias(
                subscript,
                1,
                SpecialFormType::FrozenSet,
                KnownClass::FrozenSet,
            ),
            SpecialFormType::Deque => self.infer_parameterized_legacy_typing_alias(
                subscript,
                1,
                SpecialFormType::Deque,
                KnownClass::Deque,
            ),

            SpecialFormType::ClassVar
            | SpecialFormType::Final
            | SpecialFormType::Required
            | SpecialFormType::NotRequired
            | SpecialFormType::ReadOnly => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    let diag = builder.into_diagnostic(format_args!(
                        "Type qualifier `{special_form}` is not allowed in type expressions \
                         (only in annotation expressions)",
                    ));
                    diagnostic::add_type_expression_reference_link(diag);
                }
                self.infer_type_expression(arguments_slice)
            }
            SpecialFormType::TypeIs => match arguments_slice {
                ast::Expr::Tuple(_) => {
                    self.infer_type_expression(arguments_slice);

                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        let diag = builder.into_diagnostic(
                            "Special form `typing.TypeIs` expected exactly one type parameter",
                        );
                        diagnostic::add_type_expression_reference_link(diag);
                    }

                    Type::unknown()
                }
                _ => TypeIsType::unbound(
                    self.db(),
                    // N.B. Using the top materialization here is a pragmatic decision
                    // that makes us produce more intuitive results given how
                    // `TypeIs` is used in the real world (in particular, in typeshed).
                    // However, there's some debate about whether this is really
                    // fully correct. See <https://github.com/astral-sh/ruff/pull/20591>
                    // for more discussion.
                    self.infer_type_expression(arguments_slice)
                        .top_materialization(self.db()),
                ),
            },
            SpecialFormType::TypeGuard => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`TypeGuard[]` special form")
            }
            SpecialFormType::Concatenate => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                for argument in arguments {
                    self.infer_type_expression(argument);
                }
                let num_arguments = arguments.len();
                let inferred_type = if num_arguments < 2 {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "Special form `{special_form}` expected at least 2 parameters but got {num_arguments}",
                        ));
                    }
                    Type::unknown()
                } else {
                    todo_type!("`Concatenate[]` special form")
                };
                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, inferred_type);
                }
                inferred_type
            }
            SpecialFormType::Unpack => {
                self.infer_type_expression(arguments_slice);
                todo_type!("`Unpack[]` special form")
            }
            SpecialFormType::NoReturn
            | SpecialFormType::Never
            | SpecialFormType::AlwaysTruthy
            | SpecialFormType::AlwaysFalsy => {
                self.infer_type_expression(arguments_slice);

                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Type `{special_form}` expected no type parameter",
                    ));
                }
                Type::unknown()
            }
            SpecialFormType::TypingSelf
            | SpecialFormType::TypeAlias
            | SpecialFormType::TypedDict
            | SpecialFormType::Unknown
            | SpecialFormType::Any
            | SpecialFormType::NamedTuple => {
                self.infer_type_expression(arguments_slice);

                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Special form `{special_form}` expected no type parameter",
                    ));
                }
                Type::unknown()
            }
            SpecialFormType::LiteralString => {
                let arguments = self.infer_expression(arguments_slice, TypeContext::default());

                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    let mut diag =
                        builder.into_diagnostic("`LiteralString` expects no type parameter");

                    let arguments_as_tuple = arguments.exact_tuple_instance_spec(db);

                    let mut argument_elements = arguments_as_tuple
                        .as_ref()
                        .map(|tup| tup.all_elements())
                        .unwrap_or(std::slice::from_ref(&arguments))
                        .iter()
                        .copied();

                    let probably_meant_literal = argument_elements.all(|ty| match ty {
                        Type::StringLiteral(_)
                        | Type::BytesLiteral(_)
                        | Type::EnumLiteral(_)
                        | Type::BooleanLiteral(_) => true,
                        Type::NominalInstance(instance) => {
                            instance.has_known_class(db, KnownClass::NoneType)
                        }
                        _ => false,
                    });

                    if probably_meant_literal {
                        diag.annotate(
                            self.context
                                .secondary(&*subscript.value)
                                .message("Did you mean `Literal`?"),
                        );
                        diag.set_concise_message(
                            "`LiteralString` expects no type parameter - did you mean `Literal`?",
                        );
                    }
                }
                Type::unknown()
            }
            SpecialFormType::Type => self.infer_subclass_of_type_expression(arguments_slice),
            SpecialFormType::Tuple => {
                Type::tuple(self.infer_tuple_type_expression(arguments_slice))
            }
            SpecialFormType::Generic | SpecialFormType::Protocol => {
                self.infer_expression(arguments_slice, TypeContext::default());
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "`{special_form}` is not allowed in type expressions",
                    ));
                }
                Type::unknown()
            }
        }
    }

    pub(crate) fn infer_literal_parameter_type<'param>(
        &mut self,
        parameters: &'param ast::Expr,
    ) -> Result<Type<'db>, Vec<&'param ast::Expr>> {
        Ok(match parameters {
            ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                let value_ty = self.infer_expression(value, TypeContext::default());
                if matches!(value_ty, Type::SpecialForm(SpecialFormType::Literal)) {
                    let ty = self.infer_literal_parameter_type(slice)?;

                    // This branch deals with annotations such as `Literal[Literal[1]]`.
                    // Here, we store the type for the inner `Literal[1]` expression:
                    self.store_expression_type(parameters, ty);
                    ty
                } else {
                    self.infer_expression(slice, TypeContext::default());
                    self.store_expression_type(parameters, Type::unknown());

                    return Err(vec![parameters]);
                }
            }
            ast::Expr::Tuple(tuple) if !tuple.parenthesized => {
                let mut errors = vec![];
                let mut builder = UnionBuilder::new(self.db());
                for elt in tuple {
                    match self.infer_literal_parameter_type(elt) {
                        Ok(ty) => {
                            builder = builder.add(ty);
                        }
                        Err(nodes) => {
                            errors.extend(nodes);
                        }
                    }
                }
                if errors.is_empty() {
                    let union_type = builder.build();

                    // This branch deals with annotations such as `Literal[1, 2]`. Here, we
                    // store the type for the inner `1, 2` tuple-expression:
                    self.store_expression_type(parameters, union_type);

                    union_type
                } else {
                    self.store_expression_type(parameters, Type::unknown());

                    return Err(errors);
                }
            }

            literal @ (ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)) => self.infer_expression(literal, TypeContext::default()),
            literal @ ast::Expr::NumberLiteral(number) if number.value.is_int() => {
                self.infer_expression(literal, TypeContext::default())
            }
            // for negative and positive numbers
            ast::Expr::UnaryOp(u)
                if matches!(u.op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                    && u.operand.is_number_literal_expr() =>
            {
                let ty = self.infer_unary_expression(u);
                self.store_expression_type(parameters, ty);
                ty
            }
            // enum members and aliases to literal types
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                let subscript_ty = self.infer_expression(parameters, TypeContext::default());
                match subscript_ty {
                    // type aliases to literal types
                    Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias)) => {
                        let value_ty = type_alias.value_type(self.db());
                        if value_ty.is_literal_or_union_of_literals(self.db()) {
                            return Ok(value_ty);
                        }
                    }
                    Type::KnownInstance(KnownInstanceType::Literal(ty)) => {
                        return Ok(ty.inner(self.db()));
                    }
                    // `Literal[SomeEnum.Member]`
                    Type::EnumLiteral(_) => {
                        return Ok(subscript_ty);
                    }
                    // `Literal[SingletonEnum.Member]`, where `SingletonEnum.Member` simplifies to
                    // just `SingletonEnum`.
                    Type::NominalInstance(_) if subscript_ty.is_enum(self.db()) => {
                        return Ok(subscript_ty);
                    }
                    // suppress false positives for e.g. members of functional-syntax enums
                    Type::Dynamic(DynamicType::Todo(_)) => {
                        return Ok(subscript_ty);
                    }
                    _ => {}
                }
                return Err(vec![parameters]);
            }
            _ => {
                self.infer_expression(parameters, TypeContext::default());
                return Err(vec![parameters]);
            }
        })
    }

    /// Infer the first argument to a `typing.Callable` type expression and returns the
    /// corresponding [`Parameters`].
    ///
    /// It returns `None` if the argument is invalid i.e., not a list of types, parameter
    /// specification, `typing.Concatenate`, or `...`.
    pub(super) fn infer_callable_parameter_types(
        &mut self,
        parameters: &ast::Expr,
    ) -> Option<Parameters<'db>> {
        match parameters {
            ast::Expr::EllipsisLiteral(ast::ExprEllipsisLiteral { .. }) => {
                return Some(Parameters::gradual_form());
            }
            ast::Expr::List(ast::ExprList { elts: params, .. }) => {
                let mut parameter_types = Vec::with_capacity(params.len());

                // Whether to infer `Todo` for the parameters
                let mut return_todo = false;

                for param in params {
                    let param_type = self.infer_type_expression(param);
                    // This is similar to what we currently do for inferring tuple type expression.
                    // We currently infer `Todo` for the parameters to avoid invalid diagnostics
                    // when trying to check for assignability or any other relation. For example,
                    // `*tuple[int, str]`, `Unpack[]`, etc. are not yet supported.
                    return_todo |= param_type.is_todo()
                        && matches!(param, ast::Expr::Starred(_) | ast::Expr::Subscript(_));
                    parameter_types.push(param_type);
                }

                return Some(if return_todo {
                    // TODO: `Unpack`
                    Parameters::todo()
                } else {
                    Parameters::new(
                        self.db(),
                        parameter_types.iter().map(|param_type| {
                            Parameter::positional_only(None).with_annotated_type(*param_type)
                        }),
                    )
                });
            }
            ast::Expr::Subscript(subscript) => {
                let value_ty = self.infer_expression(&subscript.value, TypeContext::default());
                self.infer_subscript_type_expression(subscript, value_ty);
                // TODO: Support `Concatenate[...]`
                return Some(Parameters::todo());
            }
            ast::Expr::Name(name) => {
                if name.is_invalid() {
                    // This is a special case to avoid raising the error suggesting what the first
                    // argument should be. This only happens when there's already a syntax error like
                    // `Callable[]`.
                    return None;
                }
                let name_ty = self.infer_name_load(name);
                if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = name_ty
                    && typevar.is_paramspec(self.db())
                {
                    let index = semantic_index(self.db(), self.scope().file(self.db()));
                    let Some(bound_typevar) = bind_typevar(
                        self.db(),
                        index,
                        self.scope().file_scope_id(self.db()),
                        self.typevar_binding_context,
                        typevar,
                    ) else {
                        // TODO: What to do here?
                        return None;
                    };
                    return Some(Parameters::paramspec(self.db(), bound_typevar));
                }
            }
            _ => {}
        }
        if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, parameters) {
            let diag = builder.into_diagnostic(format_args!(
                "The first argument to `Callable` must be either a list of types, \
                ParamSpec, Concatenate, or `...`",
            ));
            diagnostic::add_type_expression_reference_link(diag);
        }
        None
    }
}
