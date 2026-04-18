use itertools::Either;
use ruff_python_ast::helpers::is_dotted_name;
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::Ranged;

use super::{DeferredExpressionState, TypeInferenceBuilder};
use crate::types::diagnostic::{
    self, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE, UNBOUND_TYPE_VARIABLE, UNSUPPORTED_OPERATOR,
    note_py_version_too_old_for_pep_604, report_invalid_argument_number_to_special_form,
    report_invalid_arguments_to_callable, report_invalid_concatenate_last_arg,
};
use crate::types::infer::InferenceFlags;
use crate::types::infer::builder::subscript::AnnotatedExprContext;
use crate::types::signatures::{ConcatenateTail, Signature};
use crate::types::special_form::{AliasSpec, LegacyStdlibAlias};
use crate::types::string_annotation::parse_string_annotation;
use crate::types::tuple::{TupleSpecBuilder, TupleType};
use crate::types::typed_dict::resolve_unpacked_typed_dict_kwargs_annotation_target;
use ty_python_core::scope::ScopeKind;

use crate::types::{
    BindingContext, CallableType, DynamicType, GenericContext, IntersectionBuilder, KnownClass,
    KnownInstanceType, LintDiagnosticGuard, LiteralValueTypeKind, Parameter, Parameters,
    SpecialFormType, SubclassOfType, Type, TypeAliasType, TypeContext, TypeGuardType, TypeIsType,
    TypeMapping, TypeVarKind, UnionBuilder, UnionType, any_over_type, todo_type,
};
use crate::{FxOrderSet, Program, add_inferred_python_version_hint_to_diagnostic};

/// Type expressions
impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) const fn type_expression_context(&self) -> &'static str {
        self.inference_flags.type_expression_context()
    }

    /// Infer the type of a type expression.
    pub(super) fn infer_type_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
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
    pub(super) fn infer_type_expression_with_state(
        &mut self,
        expression: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> Type<'db> {
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, deferred_state);
        let annotation_ty = self.infer_type_expression(expression);
        self.deferred_state = previous_deferred_state;
        annotation_ty
    }

    fn report_invalid_type_expression(
        &self,
        expression: impl Ranged,
        message: impl std::fmt::Display,
    ) -> Option<LintDiagnosticGuard<'_, '_>> {
        self.context
            .report_lint(&INVALID_TYPE_FORM, expression)
            .map(|builder| {
                diagnostic::add_type_expression_reference_link(builder.into_diagnostic(message))
            })
    }

    pub(super) fn infer_name_or_attribute_type_expression(
        &self,
        ty: Type<'db>,
        annotation: &ast::Expr,
    ) -> Type<'db> {
        if annotation.is_attribute_expr()
            && let Type::TypeVar(tvar) = ty
            && tvar.paramspec_attr(self.db()).is_some()
        {
            return ty;
        }
        let result_ty = ty
            .default_specialize(self.db())
            .in_type_expression(
                self.db(),
                self.scope(),
                self.typevar_binding_context,
                self.inference_flags,
            )
            .unwrap_or_else(|error| {
                error.into_fallback_type(&self.context, annotation, self.inference_flags)
            });
        self.check_for_unbound_type_variable(annotation, result_ty)
    }

    /// Infer the type of a type expression without storing the result.
    pub(super) fn infer_type_expression_no_store(&mut self, expression: &ast::Expr) -> Type<'db> {
        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-type_expression
        match expression {
            ast::Expr::Name(name) => match name.ctx {
                ast::ExprContext::Load => {
                    let ty = self.infer_name_expression(name);
                    self.infer_name_or_attribute_type_expression(ty, expression)
                }
                ast::ExprContext::Invalid => Type::unknown(),
                ast::ExprContext::Store | ast::ExprContext::Del => {
                    todo_type!("Name expression annotation in Store/Del context")
                }
            },

            ast::Expr::Attribute(attribute_expression) => {
                if is_dotted_name(expression) {
                    match attribute_expression.ctx {
                        ast::ExprContext::Load => {
                            let ty = self.infer_attribute_expression(attribute_expression);
                            self.infer_name_or_attribute_type_expression(ty, expression)
                        }
                        ast::ExprContext::Invalid => Type::unknown(),
                        ast::ExprContext::Store | ast::ExprContext::Del => {
                            todo_type!("Attribute expression annotation in Store/Del context")
                        }
                    }
                } else {
                    if !self.in_string_annotation() {
                        self.infer_attribute_expression(attribute_expression);
                    }
                    self.report_invalid_type_expression(
                        expression,
                        format_args!(
                            "Only simple names, dotted names and subscripts \
                            can be used in {}s",
                            self.type_expression_context()
                        ),
                    );
                    Type::unknown()
                }
            }

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

                if is_dotted_name(value) {
                    self.infer_subscript_type_expression_no_store(subscript, slice, value_ty)
                } else {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    self.report_invalid_type_expression(
                        expression,
                        format_args!(
                            "Only simple names and dotted names can be subscripted in {}s",
                            self.type_expression_context()
                        ),
                    );
                    Type::unknown()
                }
            }

            ast::Expr::BinOp(binary) => {
                match binary.op {
                    // PEP-604 unions are okay, e.g., `int | str`
                    ast::Operator::BitOr => {
                        let left_ty = self.infer_type_expression(&binary.left);
                        let right_ty = self.infer_type_expression(&binary.right);

                        // Detect runtime errors from e.g. `int | "bytes"` on Python <3.14 without `__future__` annotations.
                        if !self.deferred_state.is_deferred()
                            && !self.is_in_type_checking_block(self.scope(), binary)
                        {
                            let mut speculative_builder = self.speculate();
                            // If the left-hand side of the union is itself a PEP-604 union,
                            // we'll already have checked whether it can be used with `|` in a previous inference step
                            // and emitted a diagnostic if it was appropriate. We should skip inferring it here to
                            // avoid duplicate diagnostics; just assume that the l.h.s. is a `UnionType` instance
                            // in that case.
                            let left_type_value = speculative_builder
                                .infer_expression(&binary.left, TypeContext::default());
                            let right_type_value = speculative_builder
                                .infer_expression(&binary.right, TypeContext::default());

                            let dunder_fails = Type::try_call_bin_op(
                                self.db(),
                                left_type_value,
                                ast::Operator::BitOr,
                                right_type_value,
                            )
                            .is_err();

                            // As well as trying the normal dunder lookup,
                            // we also check for the case where one of the operands is a class-literal type
                            // or generic-alias type and the other is a string literal. The normal dunder lookup
                            // fails to catch this error, since typeshed annotates `type.__(r)or__` as accepting `Any`.
                            let should_emit_error = if dunder_fails {
                                true
                            } else {
                                let literal = match (left_type_value, right_type_value) {
                                    (Type::ClassLiteral(class), Type::LiteralValue(literal))
                                    | (Type::LiteralValue(literal), Type::ClassLiteral(class))
                                        if class.metaclass(self.db())
                                            == KnownClass::Type.to_class_literal(self.db()) =>
                                    {
                                        Some(literal)
                                    }
                                    (Type::GenericAlias(_), Type::LiteralValue(literal))
                                    | (Type::LiteralValue(literal), Type::GenericAlias(_)) => {
                                        Some(literal)
                                    }
                                    _ => None,
                                };
                                literal.is_some_and(|literal| !literal.is_enum())
                            };

                            if should_emit_error
                                && let Some(builder) =
                                    self.context.report_lint(&UNSUPPORTED_OPERATOR, binary)
                            {
                                let mut diagnostic =
                                    builder.into_diagnostic("Unsupported `|` operation");

                                if left_type_value.is_equivalent_to(self.db(), right_type_value) {
                                    diagnostic.set_primary_message(format_args!(
                                        "Both operands have type `{}`",
                                        left_type_value.display(self.db())
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "Operator `|` is unsupported between \
                                        two objects of type `{}`",
                                        left_type_value.display(self.db())
                                    ));
                                } else {
                                    for (operand, ty) in [
                                        (&*binary.left, left_type_value),
                                        (&*binary.right, right_type_value),
                                    ] {
                                        diagnostic.annotate(
                                            self.context.secondary(operand).message(format_args!(
                                                "Has type `{}`",
                                                ty.display(self.db())
                                            )),
                                        );
                                    }
                                    diagnostic.set_concise_message(format_args!(
                                        "Operator `|` is unsupported between \
                                        objects of type `{}` and `{}`",
                                        left_type_value.display(self.db()),
                                        right_type_value.display(self.db())
                                    ));
                                }

                                match self.scope.scope(self.db()).kind() {
                                    ScopeKind::TypeAlias => diagnostic.info(
                                        "A type alias scope is lazy but will be \
                                        executed at runtime if the `__value__` property is \
                                        accessed",
                                    ),
                                    ScopeKind::TypeParams => diagnostic.info(
                                        "Type parameter scopes are lazy but may be \
                                        executed at runtime if the `__bound__`, `__value__`
                                        or `__constraints__` property of a type parameter is \
                                        accessed",
                                    ),
                                    _ => {
                                        let python_version =
                                            Program::get(self.db()).python_version(self.db());

                                        if python_version < PythonVersion::PY310
                                            && !binary.left.is_string_literal_expr()
                                            && !binary.right.is_string_literal_expr()
                                        {
                                            note_py_version_too_old_for_pep_604(
                                                self.db(),
                                                self.index,
                                                &mut diagnostic,
                                            );
                                        } else if python_version < PythonVersion::PY314 {
                                            diagnostic.info(format_args!(
                                                "All {}s are evaluated at \
                                                runtime by default on Python <3.14",
                                                self.type_expression_context()
                                            ));
                                            add_inferred_python_version_hint_to_diagnostic(
                                                self.db(),
                                                &mut diagnostic,
                                                "inferring types",
                                            );
                                            if binary.left.is_string_literal_expr()
                                                || binary.right.is_string_literal_expr()
                                            {
                                                diagnostic.help(
                                                    "Put quotes around the whole union \
                                                    rather than just certain elements",
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        UnionType::from_elements_leave_aliases(self.db(), [left_ty, right_ty])
                    }
                    // anything else is an invalid annotation:
                    op => {
                        // Avoid inferring the types of invalid binary expressions that have been
                        // parsed from a string annotation, as they are not present in the semantic
                        // index.
                        if !self.in_string_annotation() {
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

            // =====================================================================================
            // Forms which are invalid in the context of annotation expressions: we infer their
            // nested expressions as normal expressions, but the type of the top-level expression is
            // always `Type::unknown` in these cases.
            // =====================================================================================
            ast::Expr::BytesLiteral(bytes) => {
                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Bytes literals are not allowed in this context in a {}",
                        self.type_expression_context()
                    ),
                ) {
                    if let Some(single_element) = bytes.as_single_part_bytestring()
                        && let Ok(valid_string) = String::from_utf8(single_element.value.to_vec())
                    {
                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `typing.Literal[b\"{valid_string}\"]`?"
                        ));
                    }
                }
                Type::unknown()
            }

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(int),
                ..
            }) => {
                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Int literals are not allowed in this context in a {}",
                        self.type_expression_context()
                    ),
                ) {
                    if let Some(int) = int.as_i64() {
                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `typing.Literal[{int}]`?"
                        ));
                    }
                }

                Type::unknown()
            }

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Float(_),
                ..
            }) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Float literals are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Complex { .. },
                ..
            }) => {
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Complex literals are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::BooleanLiteral(bool_value) => {
                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Boolean literals are not allowed in this context in a {}",
                        self.type_expression_context()
                    ),
                ) {
                    diagnostic.set_primary_message(format_args!(
                        "Did you mean `typing.Literal[{}]`?",
                        if bool_value.value { "True" } else { "False" }
                    ));
                }
                Type::unknown()
            }

            ast::Expr::List(list) => {
                let db = self.db();

                if !self.in_string_annotation() {
                    self.infer_list_expression(list, TypeContext::default());
                }

                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "List literals are not allowed in this context in a {}",
                        self.type_expression_context()
                    ),
                ) && let [single_element] = &*list.elts
                {
                    let mut speculative_builder = self.speculate();
                    let inner_type = speculative_builder.infer_type_expression(single_element);

                    if inner_type.is_hintable(self.db()) {
                        let hinted_type =
                            KnownClass::List.to_specialized_instance(db, &[inner_type]);

                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(self.db()),
                        ));
                    }
                }
                Type::unknown()
            }

            ast::Expr::Tuple(tuple) => {
                if tuple.parenthesized {
                    if !self.in_string_annotation() {
                        for element in tuple {
                            self.infer_expression(element, TypeContext::default());
                        }
                    }

                    if let Some(mut diagnostic) = self.report_invalid_type_expression(
                        expression,
                        format_args!(
                            "Tuple literals are not allowed in this context in a {}",
                            self.type_expression_context()
                        ),
                    ) {
                        let mut speculative = self.speculate();
                        let inner_types: Vec<Type<'db>> = tuple
                            .elts
                            .iter()
                            .map(|element| speculative.infer_type_expression(element))
                            .collect();

                        if inner_types.iter().all(|ty| ty.is_hintable(self.db())) {
                            let hinted_type = Type::heterogeneous_tuple(self.db(), inner_types);
                            diagnostic.set_primary_message(format_args!(
                                "Did you mean `{}`?",
                                hinted_type.display(self.db()),
                            ));
                        }
                    }
                } else {
                    for element in tuple {
                        self.infer_type_expression(element);
                    }
                }

                Type::unknown()
            }

            ast::Expr::BoolOp(bool_op) => {
                if !self.in_string_annotation() {
                    self.infer_boolean_expression(bool_op);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Boolean operations are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Named(named) => {
                if !self.in_string_annotation() {
                    self.infer_named_expression(named);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Named expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::UnaryOp(unary) => {
                if !self.in_string_annotation() {
                    self.infer_unary_expression(unary);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Unary operations are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Lambda(lambda_expression) => {
                if !self.in_string_annotation() {
                    self.infer_lambda_expression(lambda_expression, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "`lambda` expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::If(if_expression) => {
                if !self.in_string_annotation() {
                    self.infer_if_expression(if_expression, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "`if` expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Dict(dict) => {
                if !self.in_string_annotation() {
                    self.infer_dict_expression(dict, TypeContext::default());
                }
                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Dict literals are not allowed in {}s",
                        self.type_expression_context()
                    ),
                ) && let [
                    ast::DictItem {
                        key: Some(key),
                        value,
                    },
                ] = &*dict.items
                {
                    let mut speculative = self.speculate();
                    let key_type = speculative.infer_type_expression(key);
                    let value_type = speculative.infer_type_expression(value);
                    if key_type.is_hintable(self.db()) && value_type.is_hintable(self.db()) {
                        let hinted_type = KnownClass::Dict
                            .to_specialized_instance(self.db(), &[key_type, value_type]);
                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(self.db()),
                        ));
                    }
                }
                Type::unknown()
            }

            ast::Expr::Set(set) => {
                if !self.in_string_annotation() {
                    self.infer_set_expression(set, TypeContext::default());
                }
                if let Some(mut diagnostic) = self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Set literals are not allowed in {}s",
                        self.type_expression_context()
                    ),
                ) && let [single_element] = &*set.elts
                {
                    let mut speculative_builder = self.speculate();
                    let inner_type = speculative_builder.infer_type_expression(single_element);

                    if inner_type.is_hintable(self.db()) {
                        let hinted_type =
                            KnownClass::Set.to_specialized_instance(self.db(), &[inner_type]);

                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(self.db()),
                        ));
                    }
                }
                Type::unknown()
            }

            ast::Expr::DictComp(dictcomp) => {
                if !self.in_string_annotation() {
                    self.infer_dict_comprehension_expression(dictcomp, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Dict comprehensions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::ListComp(listcomp) => {
                if !self.in_string_annotation() {
                    self.infer_list_comprehension_expression(listcomp, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "List comprehensions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::SetComp(setcomp) => {
                if !self.in_string_annotation() {
                    self.infer_set_comprehension_expression(setcomp, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Set comprehensions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Generator(generator) => {
                if !self.in_string_annotation() {
                    self.infer_generator_expression(generator);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Generator expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Await(await_expression) => {
                if !self.in_string_annotation() {
                    self.infer_await_expression(await_expression, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "`await` expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Yield(yield_expression) => {
                if !self.in_string_annotation() {
                    self.infer_yield_expression(yield_expression);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "`yield` expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::YieldFrom(yield_from) => {
                if !self.in_string_annotation() {
                    self.infer_yield_from_expression(yield_from);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "`yield from` expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Compare(compare) => {
                if !self.in_string_annotation() {
                    self.infer_compare_expression(compare);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Comparison expressions are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Call(call_expr) => {
                if !self.in_string_annotation() {
                    self.infer_call_expression(call_expr, TypeContext::default());
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Function calls are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::FString(fstring) => {
                if !self.in_string_annotation() {
                    self.infer_fstring_expression(fstring);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "F-strings are not allowed in {}s",
                        self.type_expression_context(),
                    ),
                );
                Type::unknown()
            }

            ast::Expr::TString(tstring) => {
                if !self.in_string_annotation() {
                    self.infer_tstring_expression(tstring);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "T-strings are not allowed in {}s",
                        self.type_expression_context()
                    ),
                );
                Type::unknown()
            }

            ast::Expr::Slice(slice) => {
                if !self.in_string_annotation() {
                    self.infer_slice_expression(slice);
                }
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "Slices are not allowed in {}s",
                        self.type_expression_context()
                    ),
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
                self.report_invalid_type_expression(
                    expression,
                    format_args!(
                        "`...` is not allowed in this context in a {}",
                        self.type_expression_context(),
                    ),
                );
                Type::unknown()
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
                Some(KnownClass::Tuple) => Type::tuple(self.infer_tuple_type_expression(subscript)),
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
        match parse_string_annotation(&self.context, self.inference_flags, string) {
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

    /// Return the type represented by a `tuple[]` expression in a type annotation.
    ///
    /// This method assumes that a type has already been inferred and stored for the `value`
    /// of the subscript passed in.
    pub(super) fn infer_tuple_type_expression(
        &mut self,
        tuple: &ast::ExprSubscript,
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
                    let value_ty = builder.expression_type(value);

                    value_ty == Type::SpecialForm(SpecialFormType::Unpack)
                }
                _ => false,
            }
        }

        // TODO: TypeVarTuple
        match &*tuple.slice {
            ast::Expr::Tuple(elements) => {
                if let [element, ellipsis @ ast::Expr::EllipsisLiteral(_)] = &*elements.elts {
                    if element.is_starred_expr()
                        && let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, tuple)
                    {
                        let mut diagnostic =
                            builder.into_diagnostic("Invalid `tuple` specialization");
                        diagnostic
                            .set_primary_message("`...` cannot be used after an unpacked element");
                    }
                    self.infer_expression(ellipsis, TypeContext::default());
                    let result =
                        TupleType::homogeneous(self.db(), self.infer_type_expression(element));
                    self.store_expression_type(&tuple.slice, Type::tuple(Some(result)));
                    return Some(result);
                }

                let mut element_types = TupleSpecBuilder::with_capacity(elements.len());

                // Whether to infer `Todo` for the whole tuple
                // (see docstring for `element_could_alter_type_of_whole_tuple`)
                let mut return_todo = false;

                let mut first_unpacked_variadic_tuple = None;

                for element in elements {
                    if element.is_ellipsis_literal_expr() {
                        if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, tuple) {
                            let mut diagnostic =
                                builder.into_diagnostic("Invalid `tuple` specialization");
                            diagnostic.set_primary_message(
                                "`...` can only be used as the second element \
                                in a two-element `tuple` specialization",
                            );
                        }
                        self.store_expression_type(element, Type::unknown());
                        element_types.push(Type::unknown());
                        continue;
                    }
                    let element_ty = self.infer_type_expression(element);
                    return_todo |=
                        element_could_alter_type_of_whole_tuple(element, element_ty, self);

                    // Determine if this element unpacks a tuple: either `*expr` or `Unpack[expr]`
                    let unpack_inner = if let ast::Expr::Starred(ast::ExprStarred {
                        value, ..
                    }) = element
                    {
                        Some(&**value)
                    } else if let ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) =
                        element
                        && self.expression_type(value) == Type::SpecialForm(SpecialFormType::Unpack)
                    {
                        Some(&**slice)
                    } else {
                        None
                    };

                    if let Some(unpack_inner) = unpack_inner {
                        let mut report_too_many_unpacked_tuples = || {
                            if let Some(first_unpacked_variadic_tuple) =
                                first_unpacked_variadic_tuple
                            {
                                if let Some(builder) =
                                    self.context.report_lint(&INVALID_TYPE_FORM, tuple)
                                {
                                    let mut diagnostic = builder.into_diagnostic(
                                        "Multiple unpacked variadic tuples \
                                            are not allowed in a `tuple` specialization",
                                    );
                                    diagnostic.annotate(
                                        self.context
                                            .secondary(first_unpacked_variadic_tuple)
                                            .message("First unpacked variadic tuple"),
                                    );
                                    diagnostic.annotate(
                                        self.context
                                            .secondary(element)
                                            .message("Later unpacked variadic tuple"),
                                    );
                                }
                            } else {
                                first_unpacked_variadic_tuple = Some(element);
                            }
                        };

                        if let Some(inner_tuple) = element_ty.exact_tuple_instance_spec(self.db()) {
                            element_types = element_types.concat(self.db(), &inner_tuple);

                            if inner_tuple.is_variadic() {
                                report_too_many_unpacked_tuples();
                            }
                        } else if self.expression_type(unpack_inner)
                            == Type::Dynamic(DynamicType::TodoTypeVarTuple)
                        {
                            report_too_many_unpacked_tuples();
                        } else {
                            // TODO: emit a diagnostic
                        }
                    } else {
                        element_types.push(element_ty);
                    }
                }

                let ty = if return_todo {
                    Some(TupleType::homogeneous(
                        self.db(),
                        Type::Dynamic(DynamicType::TodoTypeVarTuple),
                    ))
                } else {
                    TupleType::new(self.db(), &element_types.build())
                };

                // Here, we store the type for the inner `int, str` tuple-expression,
                // while the type for the outer `tuple[int, str]` slice-expression is
                // stored in the surrounding `infer_type_expression` call:
                self.store_expression_type(&tuple.slice, Type::tuple(ty));

                ty
            }
            single_element => {
                if single_element.is_ellipsis_literal_expr() {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, tuple) {
                        let mut diagnostic =
                            builder.into_diagnostic("Invalid `tuple` specialization");
                        diagnostic.set_primary_message(
                            "`...` can only be used as the second element \
                                in a two-element `tuple` specialization",
                        );
                    }
                    self.store_expression_type(single_element, Type::unknown());
                    return TupleType::heterogeneous(self.db(), std::iter::once(Type::unknown()));
                }
                let single_element_ty = self.infer_type_expression(single_element);
                if element_could_alter_type_of_whole_tuple(single_element, single_element_ty, self)
                {
                    Some(TupleType::homogeneous(
                        self.db(),
                        Type::Dynamic(DynamicType::TodoTypeVarTuple),
                    ))
                } else {
                    TupleType::heterogeneous(self.db(), std::iter::once(single_element_ty))
                }
            }
        }
    }

    /// Given the slice of a `type[]` annotation, return the type that the annotation represents
    fn infer_subclass_of_type_expression(&mut self, slice: &ast::Expr) -> Type<'db> {
        let invalid_type_argument = |builder: &Self, slice: &ast::Expr| {
            builder.report_invalid_type_expression(
                slice,
                "The argument to `type[]` must be a class object type",
            );
            SubclassOfType::subclass_of_unknown()
        };

        let infer_type_argument = |builder: &mut Self, slice: &ast::Expr| {
            let slice_ty = builder.infer_type_expression(slice);
            if matches!(slice_ty, Type::ProtocolInstance(_)) {
                return SubclassOfType::from(
                    builder.db(),
                    todo_type!("type[T] for protocols").expect_dynamic(),
                );
            }
            SubclassOfType::try_from_instance(builder.db(), slice_ty).unwrap_or_else(|| {
                match slice_ty {
                    Type::Callable(_) => invalid_type_argument(builder, slice),
                    _ => todo_type!("unsupported type[X] special form"),
                }
            })
        };

        match slice {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::StringLiteral(_) => {
                infer_type_argument(self, slice)
            }
            ast::Expr::BinOp(binary) if binary.op == ast::Operator::BitOr => {
                infer_type_argument(self, slice)
            }
            ast::Expr::Tuple(_) => {
                if !self.in_string_annotation() {
                    self.infer_expression(slice, TypeContext::default());
                }
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, slice) {
                    builder.into_diagnostic("type[...] must have exactly one type argument");
                }
                Type::unknown()
            }
            ast::Expr::NoneLiteral(_) => {
                self.infer_expression(slice, TypeContext::default());
                KnownClass::NoneType.to_subclass_of(self.db())
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
                                .infer_tuple_type_expression(subscript)
                                .map(|tuple_type| tuple_type.to_class_type(self.db()))
                                .unwrap_or_else(|| class_literal.default_specialization(self.db()));
                            SubclassOfType::from(self.db(), class_type)
                        } else {
                            match class_literal.generic_context(self.db()) {
                                Some(generic_context) => {
                                    let db = self.db();
                                    let specialize = &|types: &[Option<Type<'db>>]| {
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
                                    self.infer_expression(parameters, TypeContext::default());
                                    todo_type!("specialized non-generic class")
                                }
                            }
                        }
                    }
                    Type::SpecialForm(special_form @ SpecialFormType::Callable) => {
                        self.infer_parameterized_special_form_type_expression(
                            subscript,
                            special_form,
                        );
                        invalid_type_argument(self, slice)
                    }
                    _ => {
                        self.infer_expression(parameters, TypeContext::default());
                        todo_type!("unsupported nested subscript in type[X]")
                    }
                };
                self.store_expression_type(slice, parameters_ty);
                parameters_ty
            }
            _ => {
                self.infer_expression(slice, TypeContext::default());
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
        let current_inference_flags = self.inference_flags;

        // TODO
        // If we explicitly specialize a recursive generic (PEP-613 or implicit) type alias,
        // we currently miscount the number of type variables. For example, for a nested
        // dictionary type alias `NestedDict = dict[K, "V | NestedDict[K, V]"]]`, we might
        // infer `<class 'dict[K, Divergent]'>`, and therefore count just one type variable
        // instead of two. So until we properly support these, specialize all remaining type
        // variables with a `@Todo` type (since we don't know which of the type arguments
        // belongs to the remaining type variables).
        if any_over_type(self.db(), value_ty, true, |ty| ty.is_divergent()) {
            let value_ty = value_ty.apply_specialization(
                db,
                generic_context.specialize(
                    db,
                    std::iter::repeat_n(
                        todo_type!("specialized recursive generic type alias"),
                        generic_context.len(db),
                    )
                    .collect::<Vec<_>>(),
                ),
            );
            return if in_type_expression {
                value_ty
                    .in_type_expression(
                        db,
                        scope_id,
                        current_typevar_binding_context,
                        current_inference_flags,
                    )
                    .unwrap_or_else(|_| Type::unknown())
            } else {
                value_ty
            };
        }

        let specialize = &|types: &[Option<Type<'db>>]| {
            let specialized = value_ty.apply_specialization(
                db,
                generic_context.specialize_partial(db, types.iter().copied()),
            );

            if in_type_expression {
                specialized
                    .in_type_expression(
                        db,
                        scope_id,
                        current_typevar_binding_context,
                        current_inference_flags,
                    )
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
                if !self.in_string_annotation() {
                    self.infer_expression(slice, TypeContext::default());
                }
                Type::unknown()
            }
            Type::SpecialForm(special_form) => {
                self.infer_parameterized_special_form_type_expression(subscript, special_form)
            }
            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::SubscriptedProtocol(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.Protocol` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::SubscriptedGeneric(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`typing.Generic` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Deprecated(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`warnings.deprecated` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Field(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`dataclasses.Field` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::ConstraintSet(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions.ConstraintSet` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::GenericContext(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions.GenericContext` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Specialization(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions.Specialization` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::TypeAliasType(type_alias @ TypeAliasType::PEP695(_)) => {
                    if type_alias.specialization(self.db()).is_some() {
                        if !self.in_string_annotation() {
                            self.infer_expression(slice, TypeContext::default());
                        }
                        if let Some(builder) =
                            self.context.report_lint(&NOT_SUBSCRIPTABLE, subscript)
                        {
                            let mut diagnostic =
                                builder.into_diagnostic("Cannot specialize non-generic type alias");
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
                                    self.inference_flags,
                                )
                                .unwrap_or(Type::unknown())
                        }
                        None => {
                            if !self.in_string_annotation() {
                                self.infer_expression(slice, TypeContext::default());
                            }
                            if let Some(builder) =
                                self.context.report_lint(&NOT_SUBSCRIPTABLE, subscript)
                            {
                                let mut diagnostic = builder.into_diagnostic(format_args!(
                                    "Cannot specialize non-generic type alias `{}`",
                                    type_alias.name(self.db())
                                ));
                                let secondary = self.context.secondary(&*subscript.value);
                                let value_type = type_alias.raw_value_type(self.db());
                                if value_type.is_specialized_generic(self.db()) {
                                    diagnostic.annotate(secondary.message(format_args!(
                                        "Alias to `{}`, which is already specialized",
                                        value_type.display(self.db())
                                    )));
                                } else {
                                    diagnostic.annotate(secondary.message(format_args!(
                                        "Alias to `{}`, which is not generic",
                                        value_type.display(self.db())
                                    )));
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
                    self.infer_expression(slice, TypeContext::default());
                    todo_type!("Generic stringified PEP-613 type alias")
                }
                KnownInstanceType::Literal(ty) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
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
                        if !self.in_string_annotation() {
                            self.infer_expression(slice, TypeContext::default());
                        }
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
                    if !self.in_string_annotation() {
                        self.infer_expression(&subscript.slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`{}` is a `NewType` and cannot be specialized",
                            newtype.name(self.db())
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::NamedTupleSpec(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(&subscript.slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`NamedTuple` specs cannot be specialized",
                        ));
                    }
                    Type::unknown()
                }
            },
            Type::Dynamic(DynamicType::UnknownGeneric(_)) => {
                self.infer_explicit_type_alias_specialization(subscript, value_ty, true)
            }
            Type::Dynamic(_) | Type::Divergent(_) => {
                // Infer slice as a value expression to avoid false-positive
                // `invalid-type-form` diagnostics, when we have e.g.
                // `MyCallable[[int, str], None]` but `MyCallable` is dynamic.
                if !self.in_string_annotation() {
                    self.infer_expression(slice, TypeContext::default());
                }
                value_ty
            }
            Type::ClassLiteral(class) => {
                match (class.generic_context(self.db()), class.as_static()) {
                    (Some(generic_context), Some(static_class)) => {
                        let specialized_class = self.infer_explicit_class_specialization(
                            subscript,
                            value_ty,
                            static_class,
                            generic_context,
                        );

                        specialized_class
                            .in_type_expression(
                                self.db(),
                                self.scope(),
                                self.typevar_binding_context,
                                self.inference_flags,
                            )
                            .unwrap_or(Type::unknown())
                    }
                    _ => {
                        // TODO: emit a diagnostic if you try to specialize a non-generic class.
                        self.infer_expression(slice, TypeContext::default());
                        todo_type!("specialized non-generic class")
                    }
                }
            }
            Type::GenericAlias(_) => {
                self.infer_explicit_type_alias_specialization(subscript, value_ty, true)
            }
            Type::LiteralValue(literal) if literal.is_string() => {
                self.infer_expression(slice, TypeContext::default());
                // For stringified TypeAlias; remove once properly supported
                todo_type!("string literal subscripted in type expression")
            }
            Type::Union(union) => {
                self.infer_type_expression(slice);
                union.map(self.db(), |element| {
                    let mut speculative_builder = self.speculate();
                    let subscript_ty =
                        speculative_builder.infer_subscript_type_expression(subscript, *element);
                    self.context.extend(&speculative_builder.context.finish());
                    subscript_ty
                })
            }
            _ => {
                if !self.in_string_annotation() {
                    self.infer_expression(slice, TypeContext::default());
                }
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Invalid subscript of object of type `{}` in a {}",
                        value_ty.display(self.db()),
                        self.type_expression_context()
                    ));
                }
                Type::unknown()
            }
        }
    }

    fn infer_parameterized_legacy_typing_alias(
        &mut self,
        subscript_node: &ast::ExprSubscript,
        alias: LegacyStdlibAlias,
    ) -> Type<'db> {
        let arguments = &*subscript_node.slice;
        let args = if let ast::Expr::Tuple(t) = arguments {
            &*t.elts
        } else {
            std::slice::from_ref(arguments)
        };

        let AliasSpec {
            class,
            expected_argument_number,
        } = alias.alias_spec();

        if args.len() != expected_argument_number {
            if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript_node) {
                let noun = if expected_argument_number == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                builder.into_diagnostic(format_args!(
                    "Legacy alias `{alias}` expected exactly {expected_argument_number} {noun}, \
                    got {}",
                    args.len()
                ));
            }
        }
        let ty = class.to_specialized_instance(
            self.db(),
            args.iter()
                .map(|node| self.infer_type_expression(node))
                .collect::<Vec<_>>(),
        );
        if arguments.is_tuple_expr() {
            self.store_expression_type(arguments, ty);
        }
        ty
    }

    /// Infer the type of a `Callable[...]` type expression.
    pub(crate) fn infer_callable_type(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        fn inner<'db>(
            builder: &mut TypeInferenceBuilder<'db, '_>,
            subscript: &ast::ExprSubscript,
        ) -> Type<'db> {
            let db = builder.db();

            let arguments_slice = &*subscript.slice;

            let mut arguments = match arguments_slice {
                ast::Expr::Tuple(tuple) => Either::Left(tuple.iter()),
                _ => {
                    builder.infer_callable_parameter_types(arguments_slice);
                    Either::Right(std::iter::empty::<&ast::Expr>())
                }
            };

            let first_argument = arguments.next();

            let previously_allowed_concatenate = builder
                .inference_flags
                .replace(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT, true);
            let parameters =
                first_argument.and_then(|arg| builder.infer_callable_parameter_types(arg));
            builder.inference_flags.set(
                InferenceFlags::IN_VALID_CONCATENATE_CONTEXT,
                previously_allowed_concatenate,
            );

            let return_type = arguments
                .next()
                .map(|arg| builder.infer_type_expression(arg));

            let callable_type = if parameters.is_none()
                && let Some(first_argument) = first_argument
                && let ast::Expr::List(list) = first_argument
                && let [single_param] = &list.elts[..]
                && single_param.is_ellipsis_literal_expr()
            {
                builder.store_expression_type(single_param, Type::unknown());
                if let Some(mut diagnostic) = builder.report_invalid_type_expression(
                    first_argument,
                    "`[...]` is not a valid parameter list for `Callable`",
                ) {
                    if let Some(returns) = return_type {
                        diagnostic.set_primary_message(format_args!(
                            "Did you mean `Callable[..., {}]`?",
                            returns.display(db)
                        ));
                    }
                }
                Type::single_callable(
                    db,
                    Signature::new(
                        Parameters::unknown(),
                        return_type.unwrap_or_else(Type::unknown),
                    ),
                )
            } else {
                let correct_argument_number = if let Some(third_argument) = arguments.next() {
                    builder.infer_type_expression(third_argument);
                    for argument in arguments {
                        builder.infer_type_expression(argument);
                    }
                    false
                } else {
                    return_type.is_some()
                };

                if !correct_argument_number {
                    report_invalid_arguments_to_callable(&builder.context, subscript);
                }

                if correct_argument_number
                    && let (Some(parameters), Some(return_type)) = (parameters, return_type)
                {
                    Type::single_callable(db, Signature::new(parameters, return_type))
                } else {
                    Type::Callable(CallableType::unknown(db))
                }
            };

            // `Signature` / `Parameters` are not a `Type` variant, so we're storing
            // the outer callable type on these expressions instead.
            builder.store_expression_type(arguments_slice, callable_type);
            if let Some(first_argument) = first_argument {
                builder.store_expression_type(first_argument, callable_type);
            }

            callable_type
        }

        // There is disagreement among type checkers about whether `Callable` annotations
        // in the global scope or similar should be considered to create an implicit generic context.
        // For now, we do not report unbound type variables in any `Callable` contexts, but we may
        // decide to revisit this in the future.
        let previous_check_unbound_typevars = self
            .inference_flags
            .replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, false);
        let result = inner(self, subscript);
        self.inference_flags.set(
            InferenceFlags::CHECK_UNBOUND_TYPEVARS,
            previous_check_unbound_typevars,
        );
        result
    }

    fn infer_parameterized_special_form_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        special_form: SpecialFormType,
    ) -> Type<'db> {
        let db = self.db();
        let arguments_slice = &*subscript.slice;
        match special_form {
            SpecialFormType::Annotated => self
                .parse_subscription_of_annotated_special_form(
                    subscript,
                    AnnotatedExprContext::TypeExpression,
                )
                .inner_type()
                .in_type_expression(self.db(), self.scope(), None, self.inference_flags)
                .unwrap_or_else(|err| {
                    err.into_fallback_type(&self.context, subscript, self.inference_flags)
                }),
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
                    if !self.in_string_annotation() {
                        for argument in arguments {
                            self.infer_expression(argument, TypeContext::default());
                        }
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
                    if !self.in_string_annotation() {
                        for argument in arguments {
                            self.infer_expression(argument, TypeContext::default());
                        }
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
                    if !self.in_string_annotation() {
                        for argument in arguments {
                            self.infer_expression(argument, TypeContext::default());
                        }
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
                    if !self.in_string_annotation() {
                        for argument in arguments {
                            self.infer_expression(argument, TypeContext::default());
                        }
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

            SpecialFormType::CallableTypeOf | SpecialFormType::RegularCallableTypeOf => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();

                if num_arguments != 1 {
                    if !self.in_string_annotation() {
                        for argument in arguments {
                            self.infer_expression(argument, TypeContext::default());
                        }
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

                let Some(callable_type) =
                    argument_type.try_upcast_to_callable(db).map(|callables| {
                        if special_form == SpecialFormType::RegularCallableTypeOf {
                            callables
                                .map(|callable| callable.into_regular(db))
                                .into_type(db)
                        } else {
                            callables.into_type(db)
                        }
                    })
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
            SpecialFormType::LegacyStdlibAlias(alias) => {
                self.infer_parameterized_legacy_typing_alias(subscript, alias)
            }
            SpecialFormType::TypeQualifier(qualifier) => {
                if self.inference_flags.intersects(
                    InferenceFlags::IN_PARAMETER_ANNOTATION
                        | InferenceFlags::IN_RETURN_TYPE
                        | InferenceFlags::IN_TYPE_ALIAS,
                ) {
                    self.report_invalid_type_expression(
                        subscript,
                        format_args!(
                            "Type qualifier `{qualifier}` is not allowed in {}s",
                            self.inference_flags.type_expression_context(),
                        ),
                    );
                } else {
                    self.report_invalid_type_expression(
                        subscript,
                        format_args!(
                            "Type qualifier `{qualifier}` is not allowed in type expressions \
                            (only in annotation expressions)",
                        ),
                    );
                }
                self.infer_type_expression(arguments_slice)
            }
            SpecialFormType::TypeIs => match arguments_slice {
                ast::Expr::Tuple(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(arguments_slice, TypeContext::default());
                    }

                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        let diag = builder.into_diagnostic(
                            "Special form `typing.TypeIs` expected exactly one type parameter",
                        );
                        diagnostic::add_type_expression_reference_link(diag);
                    }

                    Type::unknown()
                }
                _ => {
                    let narrowed = self.infer_type_expression(arguments_slice);
                    let expanded = narrowed.expand_eagerly(self.db());

                    if expanded.is_divergent() {
                        expanded
                    } else {
                        TypeIsType::unbound(
                            self.db(),
                            // N.B. Using the top materialization here is a pragmatic decision
                            // that makes us produce more intuitive results given how
                            // `TypeIs` is used in the real world (in particular, in typeshed).
                            // However, there's some debate about whether this is really
                            // fully correct. See <https://github.com/astral-sh/ruff/pull/20591>
                            // for more discussion.
                            narrowed.top_materialization(self.db()),
                        )
                    }
                }
            },
            SpecialFormType::TypeGuard => match arguments_slice {
                ast::Expr::Tuple(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(arguments_slice, TypeContext::default());
                    }

                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        let diag = builder.into_diagnostic(
                            "Special form `typing.TypeGuard` expected exactly one type parameter",
                        );
                        diagnostic::add_type_expression_reference_link(diag);
                    }

                    Type::unknown()
                }
                _ => TypeGuardType::unbound(
                    self.db(),
                    // Unlike `TypeIs`, don't use top materialization, because
                    // `TypeGuard` clobbering behavior makes it counterintuitive
                    self.infer_type_expression(arguments_slice),
                ),
            },
            SpecialFormType::Concatenate => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "`typing.Concatenate` is not allowed in this context in a {}",
                        self.type_expression_context()
                    ));
                    diag.info("`typing.Concatenate` is only valid:");
                    diag.info(" - as the first argument to `typing.Callable`");
                    diag.info(" - as a type argument for a `ParamSpec` parameter");
                }

                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };

                for (i, argument) in arguments.iter().enumerate() {
                    if argument.is_ellipsis_literal_expr() {
                        // The trailing `...` in `Concatenate[int, str, ...]` is valid;
                        // store without going through type-expression inference.
                        self.store_expression_type(argument, Type::unknown());
                    } else if i < arguments.len() - 1 {
                        let previously_allowed_paramspec = self
                            .inference_flags
                            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
                        self.infer_type_expression(argument);
                        self.inference_flags.set(
                            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                            previously_allowed_paramspec,
                        );
                    } else {
                        let previously_allowed_paramspec = self
                            .inference_flags
                            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
                        self.infer_type_expression(argument);
                        self.inference_flags.set(
                            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                            previously_allowed_paramspec,
                        );
                    }
                }

                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, Type::unknown());
                }

                Type::Dynamic(DynamicType::InvalidConcatenateUnknown)
            }
            SpecialFormType::Unpack => {
                let inner_ty = self.infer_type_expression(arguments_slice);

                if self
                    .inference_flags
                    .contains(InferenceFlags::IN_KWARG_ANNOTATION)
                {
                    if resolve_unpacked_typed_dict_kwargs_annotation_target(self.db(), inner_ty)
                        .is_some()
                    {
                        return inner_ty;
                    }

                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        let diag = builder.into_diagnostic(format_args!(
                            "Unpacked value for `**kwargs` must be a TypedDict, not `{}`",
                            inner_ty.display(self.db())
                        ));
                        diagnostic::add_type_expression_reference_link(diag);
                    }
                    return Type::unknown();
                }

                // When the argument is a tuple type, return it directly so that
                // `Unpack[tuple[int, ...]]` behaves identically to `*tuple[int, ...]`.
                //
                // However, we still need a Todo type for things like
                // `def f(*args: Unpack[tuple[int, Unpack[tuple[str, ...]]]]): ...`,
                // which we don't yet support.
                if self
                    .inference_flags
                    .contains(InferenceFlags::IN_VARARG_ANNOTATION)
                    || inner_ty.exact_tuple_instance_spec(self.db()).is_none()
                {
                    todo_type!("`Unpack[]` special form")
                } else {
                    inner_ty
                }
            }
            SpecialFormType::NoReturn
            | SpecialFormType::Never
            | SpecialFormType::AlwaysTruthy
            | SpecialFormType::AlwaysFalsy => {
                if !self.in_string_annotation() {
                    self.infer_expression(arguments_slice, TypeContext::default());
                }

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
                if !self.in_string_annotation() {
                    self.infer_expression(arguments_slice, TypeContext::default());
                }

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
                        Type::LiteralValue(literal)
                            if matches!(
                                literal.kind(),
                                LiteralValueTypeKind::String(_)
                                    | LiteralValueTypeKind::Bytes(_)
                                    | LiteralValueTypeKind::Enum(_)
                                    | LiteralValueTypeKind::Bool(_)
                            ) =>
                        {
                            true
                        }
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
            SpecialFormType::Tuple => Type::tuple(self.infer_tuple_type_expression(subscript)),
            SpecialFormType::Generic | SpecialFormType::Protocol => {
                if !self.in_string_annotation() {
                    self.infer_expression(arguments_slice, TypeContext::default());
                }
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "`{special_form}` is not allowed in {}s",
                        self.type_expression_context(),
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
        let ty = match parameters {
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
            ast::Expr::UnaryOp(unary @ ast::ExprUnaryOp { op, operand, .. })
                if matches!(op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                    && matches!(
                        &**operand,
                        ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                            value: ast::Number::Int(_),
                            ..
                        })
                    ) =>
            {
                let ty = self.infer_unary_expression(unary);
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
                    Type::LiteralValue(literal) if literal.is_enum() => {
                        // Avoid promoting values originating from an explicitly annotated literal type.
                        return Ok(Type::LiteralValue(literal.to_unpromotable()));
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
                if !self.in_string_annotation() {
                    self.infer_expression(parameters, TypeContext::default());
                }
                return Err(vec![parameters]);
            }
        };

        Ok(if let Type::LiteralValue(literal) = ty {
            // Avoid promoting values originating from an explicitly annotated literal type.
            Type::LiteralValue(literal.to_unpromotable())
        } else {
            ty
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
                if let [ast::Expr::EllipsisLiteral(_)] = &params[..] {
                    // Return `None` here so that we emit a specific diagnostic at the callsite.
                    return None;
                }

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

                if matches!(value_ty, Type::SpecialForm(SpecialFormType::Concatenate)) {
                    return Some(self.infer_concatenate_special_form(subscript));
                }

                self.infer_subscript_type_expression(subscript, value_ty);

                // Non-Concatenate subscript (e.g. Unpack): fall back to todo
                return Some(Parameters::todo());
            }
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                if parameters
                    .as_name_expr()
                    .is_some_and(ast::ExprName::is_invalid)
                {
                    // This is a special case to avoid raising the error suggesting what the first
                    // argument should be. This only happens when there's already a syntax error like
                    // `Callable[]`.
                    return None;
                }
                let previously_allowed_paramspec = self
                    .inference_flags
                    .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
                let parameters_type = self.infer_type_expression_no_store(parameters);
                self.inference_flags.set(
                    InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                    previously_allowed_paramspec,
                );
                if let Type::TypeVar(tvar) = parameters_type
                    && tvar.is_paramspec(self.db())
                {
                    return Some(Parameters::paramspec(self.db(), tvar));
                }
                if parameters_type == Type::Dynamic(DynamicType::InvalidConcatenateUnknown) {
                    // Avoid emitting a confusing error here saying that the first argument to
                    // `Callable` must be "Concatenate, `...`, a parameter list or a ParamSpec"
                    // if the first argument *was* in fact `Concatenate` -- it was just used
                    // incorrectly. We'll have emitted an error elsewhere about the invalid use.
                    return Some(Parameters::unknown());
                }
            }
            ast::Expr::StringLiteral(string) => {
                if let Some(parsed) =
                    parse_string_annotation(&self.context, self.inference_flags, string)
                {
                    self.string_annotations
                        .insert(ruff_python_ast::ExprRef::StringLiteral(string).into());
                    let node_key = self.enclosing_node_key(string.into());

                    let previous_deferred_state = std::mem::replace(
                        &mut self.deferred_state,
                        DeferredExpressionState::InStringAnnotation(node_key),
                    );
                    let result = matches!(
                        parsed.expr(),
                        ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_)
                    )
                    .then(|| self.infer_callable_parameter_types(parsed.expr()));
                    self.deferred_state = previous_deferred_state;

                    if let Some(result) = result {
                        return result;
                    }
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

    /// Infer the parameter types represented by a `typing.Concatenate` special form.
    pub(super) fn infer_concatenate_special_form(
        &mut self,
        subscript: &ast::ExprSubscript,
    ) -> Parameters<'db> {
        let previous_concatenate_context = self
            .inference_flags
            .replace(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT, false);

        let arguments_slice = &*subscript.slice;
        let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
            &*tuple.elts
        } else {
            std::slice::from_ref(arguments_slice)
        };

        let (last_arg, prefix_args) = match arguments.split_last() {
            Some((last_arg, prefix_args)) if !prefix_args.is_empty() => (last_arg, prefix_args),
            _ => {
                if !self.in_string_annotation() {
                    for argument in arguments {
                        self.infer_expression(argument, TypeContext::default());
                    }
                }
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "`typing.Concatenate` requires at least 2 arguments when used in a \
                        type expression (got {})",
                        arguments.len()
                    ));
                }
                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, Type::unknown());
                }
                return Parameters::gradual_form();
            }
        };

        let previously_allowed_paramspec = self
            .inference_flags
            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
        let prefix_params = prefix_args
            .iter()
            .map(|arg| {
                Parameter::positional_only(None)
                    .with_annotated_type(self.infer_type_expression(arg))
            })
            .collect();
        self.inference_flags.set(
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
            previously_allowed_paramspec,
        );

        let parameters = self
            .infer_concatenate_tail(last_arg)
            .map(|tail| Parameters::concatenate(self.db(), prefix_params, tail));

        if arguments_slice.is_tuple_expr() {
            // TODO: What type to store for the argument slice in `Concatenate` because
            // `Parameters` is not a `Type` variant?
            self.store_expression_type(arguments_slice, Type::unknown());
        }

        let result = parameters.unwrap_or_else(Parameters::unknown);

        self.inference_flags.set(
            InferenceFlags::IN_VALID_CONCATENATE_CONTEXT,
            previous_concatenate_context,
        );
        result
    }

    /// Infer the last argument to a `typing.Concatenate` special form, which can be either `...`
    /// (for gradual typing), a `ParamSpec` type variable, or a string annotation that evaluates to
    /// a `ParamSpec` type variable.
    fn infer_concatenate_tail(&mut self, expr: &ast::Expr) -> Option<ConcatenateTail<'db>> {
        match expr {
            ast::Expr::EllipsisLiteral(_) => Some(ConcatenateTail::Gradual),
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                if expr.as_name_expr().is_some_and(ast::ExprName::is_invalid) {
                    return None;
                }
                let previously_allowed_paramspec = self
                    .inference_flags
                    .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
                let expr_type = self.infer_type_expression_no_store(expr);
                self.inference_flags.set(
                    InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                    previously_allowed_paramspec,
                );
                let Type::TypeVar(typevar) = expr_type else {
                    // `Concatenate` *is* allowed inside `Concatenate`, so avoid emitting here a diagnostic
                    // saying that the argument is invalid if the inner type is an invalid use of the
                    // `Concatenate` special form (we'll already have complained about the invalid use
                    // elsewhere)
                    if expr_type != Type::Dynamic(DynamicType::InvalidConcatenateUnknown) {
                        report_invalid_concatenate_last_arg(&self.context, expr, expr_type);
                    }
                    return None;
                };
                if !typevar.is_paramspec(self.db()) {
                    report_invalid_concatenate_last_arg(&self.context, expr, expr_type);
                    return None;
                }
                Some(ConcatenateTail::ParamSpec(typevar))
            }
            ast::Expr::StringLiteral(string) => {
                let Some(parsed) =
                    parse_string_annotation(&self.context, self.inference_flags, string)
                else {
                    report_invalid_concatenate_last_arg(&self.context, expr, Type::unknown());
                    return None;
                };

                self.string_annotations
                    .insert(ruff_python_ast::ExprRef::StringLiteral(string).into());
                let node_key = self.enclosing_node_key(string.into());

                if !matches!(
                    parsed.expr(),
                    ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_)
                ) {
                    report_invalid_concatenate_last_arg(&self.context, expr, Type::unknown());
                    return None;
                }

                let previous_deferred_state = std::mem::replace(
                    &mut self.deferred_state,
                    DeferredExpressionState::InStringAnnotation(node_key),
                );
                let result = self.infer_concatenate_tail(parsed.expr());
                self.deferred_state = previous_deferred_state;

                result
            }
            _ => {
                let ty = self.infer_type_expression(expr);
                if ty != Type::Dynamic(DynamicType::InvalidConcatenateUnknown) {
                    report_invalid_concatenate_last_arg(&self.context, expr, ty);
                }
                None
            }
        }
    }

    /// Checks if the inferred type is an unbound type variable and reports a diagnostic if so.
    ///
    /// Returns `Unknown` as a fallback if the type variable is unbound, otherwise returns the
    /// original type unchanged.
    pub(super) fn check_for_unbound_type_variable(
        &self,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Type<'db> {
        if !self
            .inference_flags
            .contains(InferenceFlags::CHECK_UNBOUND_TYPEVARS)
        {
            return ty;
        }
        if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = ty {
            if let Some(builder) = self.context.report_lint(&UNBOUND_TYPE_VARIABLE, expression) {
                builder.into_diagnostic(format_args!(
                    "Type variable `{name}` is not bound to any outer generic context",
                    name = typevar.name(self.db())
                ));
            }
            Type::unknown()
        } else {
            ty
        }
    }
}
