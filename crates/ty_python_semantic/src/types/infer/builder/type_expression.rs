use std::convert::Infallible;

use itertools::Either;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::name::Name;
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};
use ty_mapping_probe_macros::shared_semantic_family;

use super::local::tuple_annotation::ResultMode as TupleAnnotationResultMode;
use super::{DeferredExpressionState, TypeInferenceBuilder};
use crate::types::call::CallArguments;
use crate::types::definition_resolution::{ImportAliasResolution, resolve_definition};
use crate::types::diagnostic::{
    self, CYCLIC_TYPE_ALIAS_DEFINITION, EXPERIMENTAL_SYNTAX, INVALID_TYPE_FORM, NOT_SUBSCRIPTABLE,
    UNSUPPORTED_OPERATOR, report_invalid_argument_number_to_special_form,
    report_invalid_arguments_to_callable, report_invalid_concatenate_last_arg,
    report_missing_type_arguments, report_unsupported_binary_operation,
};
use crate::types::infer::builder::subscript::AnnotatedExprContext;
use crate::types::infer::{
    CyclicTypeAliasError, ImplicitAliasInference, InferenceFlags, TypeExpressionFlags,
    implicit_alias_parameters, infer_implicit_alias_type,
};
use crate::types::signatures::{ConcatenateTail, Signature};
use crate::types::special_form::{AliasSpec, LegacyStdlibAlias};
use crate::types::string_annotation::parse_string_annotation;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::place_table;
use ty_python_core::scope::ScopeKind;

use crate::types::{
    BindingContext, CallableType, ClassLiteral, DynamicType, GenericContext, IntersectionBuilder,
    IntersectionType, InvalidTypeExpression, InvalidTypeExpressionError, KnownClass,
    KnownInstanceType, LintDiagnosticGuard, Parameter, Parameters, SpecialFormType,
    StaticClassLiteral, SubclassOfType, Type, TypeContext, TypeFormType, TypeGuardType, TypeIsType,
    TypeMapping, TypeVarKind, UnionBuilder, UnionType, any_over_type, todo_type,
};
use crate::{FxOrderSet, SemanticModel, add_inferred_python_version_hint_to_diagnostic};

pub(in crate::types::infer) mod variable_scope;

/// Type expressions
impl<'db> TypeInferenceBuilder<'db, '_> {
    fn recursive_implicit_alias_reference(
        &mut self,
        value_ty: Type<'db>,
        definition: Option<Definition<'db>>,
    ) -> Option<(Type<'db>, Option<GenericContext<'db>>)> {
        let Ok(result) = recursive_alias_reference_sync(
            self,
            value_ty,
            definition,
            TypeExpressionFacts,
            &OrdinaryTypeExpressionEffects,
        );
        result
    }

    fn resolve_recursive_implicit_alias_reference(
        &mut self,
        mut definition: Definition<'db>,
    ) -> Option<(Type<'db>, Option<GenericContext<'db>>)> {
        let db = self.db();
        if definition.kind(db).is_import() {
            // Imports bind names without declaring their types, so resolve their definitions directly.
            let table = place_table(db, definition.scope(db));
            let symbol = table.symbol(definition.place(db).as_symbol()?);
            let definitions = resolve_definition(
                db,
                self.program_environment(),
                definition,
                Some(symbol.name().as_str()),
                ImportAliasResolution::ResolveAliases,
            );
            let [resolved] = definitions.as_slice() else {
                return None;
            };
            definition = resolved.definition()?;
        }
        let module = parsed_module(db, definition.program_file(db).python_file(db)).load(db);
        let value = definition.kind(db).value(&module)?;
        if !matches!(
            value,
            ast::Expr::Name(_)
                | ast::Expr::Attribute(_)
                | ast::Expr::Subscript(_)
                | ast::Expr::BinOp(_)
                | ast::Expr::StringLiteral(_)
        ) {
            return None;
        }
        match definition.kind(db) {
            DefinitionKind::Assignment(_) if !value.is_string_literal_expr() => {}
            DefinitionKind::AnnotatedAssignment(assignment)
                if crate::types::definition_expression_type(
                    db,
                    definition,
                    assignment.annotation(&module),
                )
                .is_typealias_special_form() => {}
            _ => return None,
        }
        let parameters = implicit_alias_parameters(db, definition);
        let result = infer_implicit_alias_type(db, definition, parameters).ty;
        // Preserve cycle errors even when recovery removes every recursive reference. Both
        // runtime-value inference and enclosing aliases need the fallback type to converge.
        let ty = result.unwrap_or_else(|error| error.fallback_type);
        let is_recursive = any_over_type(db, self.program_environment(), ty, false, |ty| {
            matches!(ty, Type::Recursive(_))
        });
        if result.is_ok() && !is_recursive {
            return None;
        }

        // Diagnostics and suppression usage belong to the file defining the alias.
        if definition.program_file(db) == self.program_file()
            && self.context.should_collect_diagnostics()
        {
            self.implicit_aliases.insert(definition);
        }
        Some((ty, parameters))
    }

    pub(in crate::types::infer) fn finish_implicit_alias_type(
        mut self,
        definition: Definition<'db>,
        value: &ast::Expr,
    ) -> ImplicitAliasInference<'db> {
        self.typevar_binding_context = Some(definition);
        self.context.inference_flags |= InferenceFlags::IN_TYPE_ALIAS;
        let ty = self.infer_type_expression(value);
        let db = self.db();
        let ty = if ty.has_unguarded_alias_cycle(db) {
            let target = match definition.kind(db) {
                DefinitionKind::Assignment(assignment) => Some(assignment.target(self.module())),
                DefinitionKind::AnnotatedAssignment(assignment) => {
                    Some(assignment.target(self.module()))
                }
                _ => None,
            };
            if let Some(name) = target.and_then(ast::Expr::as_name_expr)
                && let Some(diagnostic) = self
                    .context
                    .report_lint(&CYCLIC_TYPE_ALIAS_DEFINITION, value)
            {
                diagnostic.into_diagnostic(format_args!(
                    "Type alias `{}` has a circular definition",
                    name.id
                ));
            }
            Err(CyclicTypeAliasError { fallback_type: ty })
        } else {
            Ok(ty)
        };
        ImplicitAliasInference {
            ty,
            diagnostics: self.context.finish(),
            implicit_aliases: self.implicit_aliases.into_iter().collect(),
        }
    }

    const fn type_expression_context(&self) -> &'static str {
        self.inference_flags().type_expression_context()
    }

    /// Infer the type of a type expression.
    pub(super) fn infer_type_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        super::local::type_expression(self, expression, TypeExpressionMode::Scoped)
    }

    /// Similar to [`infer_type_expression`], but accepts a [`DeferredExpressionState`].
    ///
    /// [`infer_type_expression`]: TypeInferenceBuilder::infer_type_expression
    pub(super) fn infer_type_expression_with_state(
        &mut self,
        expression: &ast::Expr,
        deferred_state: DeferredExpressionState,
    ) -> Type<'db> {
        super::local::type_expression(
            self,
            expression,
            TypeExpressionMode::ScopedWithState(deferred_state),
        )
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

    /// Resolve a type-expression reference once, retaining its source definition.
    pub(super) fn infer_type_expression_reference(
        &mut self,
        expression: &ast::Expr,
    ) -> (Type<'db>, Option<Definition<'db>>) {
        match expression {
            ast::Expr::Name(name) if name.ctx.is_load() => {
                self.infer_name_load_with_definition(name)
            }
            ast::Expr::Attribute(attribute) if attribute.ctx.is_load() => {
                let resolved = self.infer_attribute_load(attribute).unwrap_or_else(|ty| ty);
                (resolved.inner_type(), resolved.provenance().definition())
            }
            _ => (
                self.infer_expression(expression, TypeContext::default()),
                None,
            ),
        }
    }

    pub(super) fn infer_name_or_attribute_type_expression(
        &mut self,
        ty: Type<'db>,
        definition: Option<Definition<'db>>,
        annotation: &ast::Expr,
    ) -> Type<'db> {
        let Ok(ty) = convert_reference_sync(
            self,
            annotation,
            (ty, definition),
            TypeExpressionFacts,
            &OrdinaryTypeExpressionEffects,
        );
        ty
    }

    /// Infer the type of a type expression without storing the result.
    pub(super) fn infer_type_expression_no_store(&mut self, expression: &ast::Expr) -> Type<'db> {
        super::local::type_expression(self, expression, TypeExpressionMode::NoStore)
    }

    fn validate_union_type_expression_runtime(&mut self, binary: &ast::ExprBinOp) {
        let db = self.db();
        let env = self.program_environment();
        // Detect runtime errors from e.g. `int | "bytes"` on Python <3.14 without `__future__` annotations.
        let mut speculative_builder = self.speculate_without_diagnostics();
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
            db,
            env,
            left_type_value,
            ast::Operator::BitOr,
            right_type_value,
        )
        .is_err();

        // As well as trying the normal dunder lookup,
        // we also check for the case where one of the operands is a class-literal type
        // or generic-alias type and the other is a string literal. The normal dunder lookup
        // fails to catch this error, since typeshed annotates `type.__(r)or__` as accepting `Any`.
        // ABCMeta and _ProtocolMeta inherit these operators unchanged. The typeshed
        // protocol fallback does not establish a custom operator either.
        let should_emit_error = if dunder_fails {
            true
        } else {
            let literal = match (left_type_value, right_type_value) {
                (Type::ClassLiteral(class), Type::LiteralValue(literal))
                | (Type::LiteralValue(literal), Type::ClassLiteral(class))
                    if matches!(
                        class
                            .inferred_metaclass(db)
                            .for_inheritance(db, env)
                            .to_class_type(db)
                            .and_then(|metaclass| metaclass.known(db)),
                        Some(
                            KnownClass::Type
                                | KnownClass::ABCMeta
                                | KnownClass::ProtocolMeta
                        )
                    ) =>
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

            if left_type_value.is_equivalent_to(db, env, right_type_value) {
                diagnostic.set_primary_annotation_message(format_args!(
                    "Both operands have type `{}`",
                    left_type_value.display(db, env)
                ));
                diagnostic.set_concise_message(format_args!(
                    "Operator `|` is unsupported between \
                    two objects of type `{}`",
                    left_type_value.display(db, env)
                ));
            } else {
                for (operand, ty) in [
                    (&*binary.left, left_type_value),
                    (&*binary.right, right_type_value),
                ] {
                    diagnostic.annotate(
                        self.context.secondary(operand).message(format_args!(
                            "Has type `{}`",
                            ty.display(db, env)
                        )),
                    );
                }
                diagnostic.set_concise_message(format_args!(
                    "Operator `|` is unsupported between \
                    objects of type `{}` and `{}`",
                    left_type_value.display(db, env),
                    right_type_value.display(db, env)
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
                        self.program_environment().python_version(db);

                    if python_version < PythonVersion::PY314 {
                        diagnostic.info(format_args!(
                            "All {}s are evaluated at \
                            runtime by default on Python <3.14",
                            self.type_expression_context()
                        ));
                        add_inferred_python_version_hint_to_diagnostic(
                            db,
                            self.file(),
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

    fn infer_type_expression_legacy_no_store(&mut self, expression: &ast::Expr) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ignore_runtime_errors = |builder: &Self| {
            builder.deferred_state.is_deferred()
                || builder.in_stub()
                || builder.is_in_type_checking_block(builder.scope(), expression)
        };
        let ignore_experimental_runtime_errors = |builder: &Self| {
            ignore_runtime_errors(builder)
                || matches!(builder.scope.scope(db).kind(), ScopeKind::TypeAlias)
        };

        // https://typing.python.org/en/latest/spec/annotations.html#grammar-token-expression-grammar-type_expression
        match expression {
            ast::Expr::Name(_) => self.infer_type_expression_no_store(expression),

            ast::Expr::Attribute(attribute_expression) => {
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

            ast::Expr::NoneLiteral(_literal) => Type::none(db, env),

            // https://typing.python.org/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(string) => self.infer_string_type_expression(string),

            ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if !self.in_string_annotation() {
                    self.infer_expression(value, TypeContext::default());
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

            ast::Expr::BinOp(binary) => {
                match binary.op {
                    ast::Operator::BitAnd => {
                        if let Some(builder) =
                            self.context.report_lint(&EXPERIMENTAL_SYNTAX, binary)
                        {
                            builder.into_diagnostic("Intersection type syntax is experimental");
                        }

                        let left_ty = self.infer_type_expression(&binary.left);
                        let right_ty = self.infer_type_expression(&binary.right);

                        if !ignore_experimental_runtime_errors(self) {
                            // Infer the operands as values to report the types used by the runtime
                            // operation rather than their interpretation as type expressions.
                            let mut speculative_builder = self.speculate_without_diagnostics();
                            let left_value = speculative_builder
                                .infer_expression(&binary.left, TypeContext::default());
                            let right_value = speculative_builder
                                .infer_expression(&binary.right, TypeContext::default());
                            if Type::try_call_bin_op(
                                db,
                                env,
                                left_value,
                                ast::Operator::BitAnd,
                                right_value,
                            )
                            .is_err()
                            {
                                report_unsupported_binary_operation(
                                    &self.context,
                                    binary,
                                    left_value,
                                    right_value,
                                    ast::Operator::BitAnd,
                                );
                            }
                        }

                        IntersectionType::from_two_elements(db, env, left_ty, right_ty)
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
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Did you mean `typing.Literal[b\"{valid_string}\"]`?"
                        ));
                        diagnostic::autofix_with_literal(
                            &self.context,
                            &mut diagnostic,
                            expression,
                        );
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
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Did you mean `typing.Literal[{int}]`?"
                        ));
                        diagnostic::autofix_with_literal(
                            &self.context,
                            &mut diagnostic,
                            expression,
                        );
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
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Did you mean `typing.Literal[{}]`?",
                        if bool_value.value { "True" } else { "False" }
                    ));
                    diagnostic::autofix_with_literal(&self.context, &mut diagnostic, expression);
                }
                Type::unknown()
            }

            ast::Expr::List(list) => {
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
                    let mut speculative_builder = self.speculate_without_diagnostics();
                    let inner_type = speculative_builder.infer_type_expression(single_element);

                    if inner_type.is_hintable(self.db()) {
                        let hinted_type =
                            KnownClass::List.to_specialized_instance(db, env, &[inner_type]);

                        diagnostic.set_primary_annotation_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(db, env),
                        ));
                    }

                    if !self.in_string_annotation()
                        && env.python_version(db) >= PythonVersion::PY39
                        && !single_element.is_starred_expr()
                        && !source_text(db, self.file()).contains_line_break(list.range())
                        && SemanticModel::new(db, self.program_file())
                            .definitely_has_builtin_binding("list", expression.into())
                    {
                        diagnostic.help("Replace with `list[...]`");
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                            "list".to_string(),
                            expression.start(),
                        )));
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
                        let mut speculative = self.speculate_without_diagnostics();
                        let inner_types: Vec<Type<'db>> = tuple
                            .elts
                            .iter()
                            .map(|element| speculative.infer_type_expression(element))
                            .collect();

                        if inner_types.iter().all(|ty| ty.is_hintable(self.db())) {
                            let hinted_type = Type::heterogeneous_tuple(db, env, inner_types);
                            diagnostic.set_primary_annotation_message(format_args!(
                                "Did you mean `{}`?",
                                hinted_type.display(db, env),
                            ));
                        }

                        if !self.in_string_annotation()
                            && !source_text(db, self.file()).contains_line_break(tuple.range())
                            && env.python_version(db) >= PythonVersion::PY39
                            && !tuple.elts.iter().any(ast::Expr::is_starred_expr)
                            && SemanticModel::new(db, self.program_file())
                                .definitely_has_builtin_binding("tuple", tuple.into())
                        {
                            diagnostic.help("Replace with `tuple[...]`");
                            if let (Some(first_elt), Some(last_elt)) =
                                (tuple.elts.first(), tuple.elts.last())
                            {
                                let first_range = parenthesized_range(
                                    first_elt.into(),
                                    tuple.into(),
                                    self.module().tokens(),
                                )
                                .unwrap_or(first_elt.range());
                                let last_range = parenthesized_range(
                                    last_elt.into(),
                                    tuple.into(),
                                    self.module().tokens(),
                                )
                                .unwrap_or(last_elt.range());
                                diagnostic.set_fix(Fix::unsafe_edits(
                                    Edit::range_replacement(
                                        "tuple[".to_string(),
                                        TextRange::new(tuple.start(), first_range.start()),
                                    ),
                                    [Edit::range_replacement(
                                        "]".to_string(),
                                        TextRange::new(last_range.end(), tuple.end()),
                                    )],
                                ));
                            } else {
                                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                    "tuple[()]".to_string(),
                                    tuple.range(),
                                )));
                            }
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
                    self.infer_boolean_expression(bool_op, TypeContext::default());
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

            ast::Expr::UnaryOp(
                unary @ ast::ExprUnaryOp {
                    op: ast::UnaryOp::Invert,
                    operand,
                    ..
                },
            ) => {
                if let Some(builder) = self.context.report_lint(&EXPERIMENTAL_SYNTAX, unary) {
                    builder.into_diagnostic("Negation type syntax is experimental");
                }

                let operand_ty = self.infer_type_expression(operand);

                if !ignore_experimental_runtime_errors(self) {
                    let operand_value = self
                        .speculate_without_diagnostics()
                        .infer_expression(operand, TypeContext::default());
                    if let Err(error) = operand_value.try_call_dunder(
                        db,
                        env,
                        "__invert__",
                        CallArguments::none(),
                        TypeContext::default(),
                    ) {
                        self.report_unsupported_unary_operator(
                            unary,
                            ast::UnaryOp::Invert,
                            operand_value,
                            "__invert__",
                            Some(&error),
                        );
                    }
                }

                operand_ty.negate(db, env)
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
                    let mut speculative = self.speculate_without_diagnostics();
                    let key_type = speculative.infer_type_expression(key);
                    let value_type = speculative.infer_type_expression(value);
                    if key_type.is_hintable(self.db()) && value_type.is_hintable(self.db()) {
                        let hinted_type = KnownClass::Dict.to_specialized_instance(
                            db,
                            env,
                            &[key_type, value_type],
                        );
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(db, env),
                        ));
                    }
                    if !self.in_string_annotation()
                        && env.python_version(db) >= PythonVersion::PY39
                        && !source_text(db, self.file()).contains_line_break(dict.range())
                        && SemanticModel::new(db, self.program_file())
                            .definitely_has_builtin_binding("dict", dict.into())
                    {
                        let key_range =
                            parenthesized_range(key.into(), dict.into(), self.module().tokens())
                                .unwrap_or(key.range());
                        let value_range =
                            parenthesized_range(value.into(), dict.into(), self.module().tokens())
                                .unwrap_or(value.range());
                        diagnostic.help("Replace with `dict[...]`");
                        diagnostic.set_fix(Fix::unsafe_edits(
                            Edit::range_replacement(
                                "dict[".to_string(),
                                TextRange::new(dict.start(), key_range.start()),
                            ),
                            [
                                Edit::range_replacement(
                                    ", ".to_string(),
                                    TextRange::new(key_range.end(), value_range.start()),
                                ),
                                Edit::range_replacement(
                                    "]".to_string(),
                                    TextRange::new(value_range.end(), dict.end()),
                                ),
                            ],
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
                    let mut speculative_builder = self.speculate_without_diagnostics();
                    let inner_type = speculative_builder.infer_type_expression(single_element);

                    if inner_type.is_hintable(self.db()) {
                        let hinted_type =
                            KnownClass::Set.to_specialized_instance(db, env, &[inner_type]);

                        diagnostic.set_primary_annotation_message(format_args!(
                            "Did you mean `{}`?",
                            hinted_type.display(db, env),
                        ));
                    }

                    if !self.in_string_annotation()
                        && env.python_version(db) >= PythonVersion::PY39
                        && !single_element.is_starred_expr()
                        && !source_text(db, self.file()).contains_line_break(set.range())
                        && SemanticModel::new(db, self.program_file())
                            .definitely_has_builtin_binding("set", set.into())
                    {
                        let element_range = parenthesized_range(
                            single_element.into(),
                            set.into(),
                            self.module().tokens(),
                        )
                        .unwrap_or(single_element.range());
                        diagnostic.help("Replace with `set[...]`");
                        diagnostic.set_fix(Fix::unsafe_edits(
                            Edit::range_replacement(
                                "set[".to_string(),
                                TextRange::new(set.start(), element_range.start()),
                            ),
                            [Edit::range_replacement(
                                "]".to_string(),
                                TextRange::new(element_range.end(), set.end()),
                            )],
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
                    self.infer_generator_expression(generator, TypeContext::default());
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

            ast::Expr::Starred(_) => self.infer_type_expression_no_store(expression),
        }
    }

    pub(super) fn infer_subscript_type_expression_no_store(
        &mut self,
        subscript: &ast::ExprSubscript,
        _slice: &ast::Expr,
        value_ty: Type<'db>,
        definition: Option<Definition<'db>>,
    ) -> Type<'db> {
        super::local::type_expression_request(
            self,
            TypeExpressionRequest::ResolvedSubscript {
                subscript,
                value_ty,
                definition,
            },
        )
    }

    /// Infer the type of a string type expression.
    pub(super) fn infer_string_type_expression(
        &mut self,
        string: &ast::ExprStringLiteral,
    ) -> Type<'db> {
        super::local::string_type_expression(self, string)
    }

    /// Infer a `tuple[]` annotation and return its instance, class, or subclass type.
    ///
    /// This method assumes that a type has already been inferred and stored for the `value`
    /// of the subscript passed in.
    ///
    /// Recovers a bare `TypeVarTuple` as `*tuple[Unknown, ...]`, preserving surrounding elements.
    /// An enclosing `tuple[tuple[Ts]]` still has exactly one element.
    pub(super) fn infer_tuple_type_expression(
        &mut self,
        tuple: &ast::ExprSubscript,
        mode: super::local::tuple_annotation::ResultMode,
    ) -> Type<'db> {
        super::local::tuple_annotation(self, super::local::tuple_annotation::Request { subscript: tuple, mode })
    }

    /// Given the slice of a `type[]` annotation, return the type that the annotation represents
    fn infer_subclass_of_type_expression(&mut self, slice: &ast::Expr) -> Type<'db> {
        super::local::type_expression_request(
            self,
            TypeExpressionRequest::SubclassArgument { slice },
        )
    }

    fn infer_subclass_of_type_expression_legacy(&mut self, slice: &ast::Expr) -> Type<'db> {
        match slice {
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
                KnownClass::NoneType.to_subclass_of(self.db(), self.program_environment())
            }
            _ => {
                self.infer_type_expression(slice);
                todo_type!("unsupported type[X] special form")
            }
        }
    }

    /// Infers a nested `type[]` argument after resolving a receiver that is not a class literal.
    ///
    /// For example:
    ///
    /// ```python
    /// import typing
    ///
    /// def accepts(value: type[typing.Union[int, str]]) -> None:
    ///     pass
    /// ```
    ///
    /// `slice` and `subscript` both refer to `typing.Union[int, str]`; the resolved receiver
    /// is `typing.Union`, while `subscript.slice` contains `int, str`.
    /// The shared caller stores the result on `slice` after this operation completes.
    fn infer_resolved_subclass_subscript_legacy(
        &mut self,
        slice: &ast::Expr,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let parameters = &subscript.slice;
        let invalid_type_argument = |builder: &Self, slice: &ast::Expr| {
            builder.report_invalid_type_expression(
                slice,
                "The argument to `type[]` must be a class object type",
            );
            SubclassOfType::subclass_of_unknown()
        };
        let subclass_of_type_argument = |builder: &Self, slice: &ast::Expr, slice_ty: Type<'db>| {
            let Ok(ty) = convert_subclass_argument_sync(
                builder,
                slice,
                slice_ty,
                TypeExpressionFacts,
                &OrdinaryTypeExpressionEffects,
            );
            ty
        };
        match value_ty {
            Type::SpecialForm(SpecialFormType::Union) => match &**parameters {
                ast::Expr::Tuple(tuple) => {
                    let ty = UnionType::from_elements_leave_aliases(
                        db,
                        env,
                        tuple
                            .iter()
                            .map(|element| self.infer_subclass_of_type_expression(element)),
                    );
                    self.store_expression_type(parameters, ty);
                    ty
                }
                _ => self.infer_subclass_of_type_expression(parameters),
            },
            Type::SpecialForm(
                special_form @ (SpecialFormType::TypingCallable
                | SpecialFormType::CollectionsAbcCallable),
            ) => {
                self.infer_parameterized_special_form_type_expression(
                    subscript,
                    special_form,
                );
                invalid_type_argument(self, slice)
            }
            value_ty @ (Type::SpecialForm(
                SpecialFormType::Top
                | SpecialFormType::Bottom
                | SpecialFormType::Annotated
                | SpecialFormType::Intersection,
            )
            | Type::KnownInstance(_)
            | Type::GenericAlias(_)
            | Type::Callable(_)) => {
                let slice_ty = self.infer_subscript_type_expression(subscript, value_ty);
                subclass_of_type_argument(self, slice, slice_ty)
            }
            _ => {
                self.infer_type_expression(parameters);
                todo_type!("unsupported nested subscript in type[X]")
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
        let env = self.program_environment();
        let db = self.db();

        if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = value_ty
            && let Some(definition) = typevar.definition(db)
        {
            value_ty = value_ty.apply_type_mapping(
                db,
                env,
                &TypeMapping::BindLegacyTypevars(BindingContext::Definition(definition)),
                TypeContext::default(),
            );
        }

        let mut variables = FxOrderSet::default();
        value_ty.find_legacy_typevars(db, env, None, &mut variables);
        let generic_context = GenericContext::from_typevar_instances(db, env, variables);

        let scope_id = self.scope();
        let current_typevar_binding_context = self.typevar_binding_context;
        let current_inference_flags = self.inference_flags();

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

    fn infer_subscript_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
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
                            "`ty_extensions._internal.ConstraintSet` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::ConstraintSetSolution(_) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`ty_extensions._internal.ConstraintSetSolution` is not allowed in {}s",
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
                            "`ty_extensions._internal.GenericContext` is not allowed in {}s",
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
                            "`ty_extensions._internal.Specialization` is not allowed in {}s",
                            self.type_expression_context(),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::TypeAliasType(type_alias) => {
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
                                    db,
                                    self.scope(),
                                    self.typevar_binding_context,
                                    self.inference_flags(),
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
                                        value_type.display(db, env)
                                    )));
                                } else {
                                    diagnostic.annotate(secondary.message(format_args!(
                                        "Alias to `{}`, which is not generic",
                                        value_type.display(db, env)
                                    )));
                                }
                            }

                            Type::unknown()
                        }
                    }
                }
                KnownInstanceType::Literal(ty) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`{ty}` is not a generic class",
                            ty = ty.inner(self.db()).display(db, env)
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
                KnownInstanceType::LiteralStringAlias(_)
                | KnownInstanceType::UnionType(_)
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
                KnownInstanceType::Sentinel(sentinel) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(&subscript.slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`{}` is a sentinel and cannot be specialized",
                            sentinel.name(self.db())
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
                KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::FunctoolsPartialCall(_) => {
                    self.infer_type_expression(&subscript.slice);
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`functools.partial` instances cannot be specialized",
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::MethodWrapper(wrapper) => {
                    if !self.in_string_annotation() {
                        self.infer_expression(&subscript.slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`{}` instances cannot be specialized",
                            wrapper.class(db).name(env.python_version(db)),
                        ));
                    }
                    Type::unknown()
                }
                KnownInstanceType::Range { .. } => {
                    if !self.in_string_annotation() {
                        self.infer_expression(&subscript.slice, TypeContext::default());
                    }
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        builder.into_diagnostic(format_args!(
                            "`range` instances cannot be specialized"
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
            Type::ClassLiteral(class) => super::local::type_expression_request(
                self,
                TypeExpressionRequest::ClassSubscript {
                    subscript,
                    value_ty,
                    class,
                },
            ),
            Type::GenericAlias(_) => {
                self.infer_explicit_type_alias_specialization(subscript, value_ty, true)
            }
            Type::LiteralValue(literal) if literal.is_string() => {
                self.infer_expression(slice, TypeContext::default());
                // For stringified TypeAlias; remove once properly supported
                todo_type!("string literal subscripted in type expression")
            }
            Type::Union(union) => {
                let db = self.db();
                let mut union_builder = UnionBuilder::new(db, env)
                    .or_recursively_defined(union.recursively_defined(db));

                for (index, element) in union.elements(db).iter().enumerate() {
                    let mut speculative_builder = self.speculate();
                    let subscript_ty =
                        speculative_builder.infer_subscript_type_expression(subscript, *element);
                    if index == 0 {
                        self.extend(speculative_builder);
                    } else {
                        self.context.extend(&speculative_builder.context.finish());
                    }
                    union_builder = union_builder.add(subscript_ty);
                }

                union_builder.build()
            }
            _ => {
                if !self.in_string_annotation() {
                    self.infer_expression(slice, TypeContext::default());
                }
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Invalid subscript of object of type `{}` in a {}",
                        value_ty.display(db, env),
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
        let db = self.db();
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
            db,
            self.program_environment(),
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
        if let Some(request) = super::local::callable_annotation::request(subscript) {
            return super::local::callable_annotation(self, request);
        }
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
                .context
                .inference_flags
                .replace(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT, true);
            let parameters =
                first_argument.and_then(|arg| builder.infer_callable_parameter_types(arg));
            builder.context.inference_flags.set(
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
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Did you mean `Callable[..., {}]`?",
                            returns.display(db, builder.program_environment())
                        ));
                        if !builder.in_string_annotation()
                            && !source_text(db, builder.file())
                                .contains_line_break(first_argument.range())
                        {
                            diagnostic.help("Replace `[...]` with `...`");
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                "...".to_string(),
                                first_argument.range(),
                            )));
                        }
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
            .context
            .inference_flags
            .replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, false);
        let result = inner(self, subscript);
        self.context.inference_flags.set(
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
        let env = self.program_environment();
        let db = self.db();
        let arguments_slice = &*subscript.slice;
        match special_form {
            SpecialFormType::Annotated => self
                .parse_subscription_of_annotated_special_form(
                    subscript,
                    AnnotatedExprContext::TypeExpression,
                )
                .inner_type()
                .in_type_expression(db, self.scope(), None, self.inference_flags())
                .unwrap_or_else(|err| {
                    err.into_fallback_type(&self.context, subscript, self.inference_flags())
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
                UnionType::from_elements_leave_aliases(db, env, [param_type, Type::none(db, env)])
            }
            SpecialFormType::Union => {
                // TODO: Support the union of a `TypeVarTuple`'s elements. Until then, reject
                // `Union[*Ts]` and recover to `object` rather than treating `Ts` as one member.
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let mut has_unpacked_typevartuple = false;
                let union_ty = UnionType::from_elements_leave_aliases(
                    db,
                    env,
                    arguments.iter().map(|argument| {
                        let ty = self.infer_type_expression(argument);
                        if self
                            .type_expression_flags(argument)
                            .contains(TypeExpressionFlags::UNPACK)
                        {
                            let is_typevartuple =
                                matches!(
                                    ty,
                                    Type::TypeVar(typevar) if typevar.is_typevartuple(db)
                                ) || if let ast::Expr::Subscript(subscript) = argument {
                                    matches!(
                                        self.expression_type(&subscript.slice),
                                        Type::TypeVar(typevar) if typevar.is_typevartuple(db)
                                    )
                                } else {
                                    false
                                };

                            if is_typevartuple {
                                has_unpacked_typevartuple = true;
                                if !ty.is_unknown()
                                    && let Some(builder) =
                                        self.context.report_lint(&INVALID_TYPE_FORM, argument)
                                {
                                    diagnostic::add_type_expression_reference_link(
                                        builder.into_diagnostic(
                                            "Unpacking a `TypeVarTuple` in `Union` \
                                            is not supported",
                                        ),
                                    );
                                }
                            }
                        }
                        ty
                    }),
                );
                let ty = if has_unpacked_typevartuple {
                    Type::object()
                } else {
                    union_ty
                };
                if arguments_slice.is_tuple_expr() {
                    self.store_expression_type(arguments_slice, ty);
                }
                ty
            }
            SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => {
                self.infer_callable_type(subscript)
            }

            // `ty_extensions` special forms
            SpecialFormType::Not => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let num_arguments = arguments.len();
                let negated_type = if num_arguments == 1 {
                    self.infer_type_expression(&arguments[0]).negate(db, env)
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
                    .fold(IntersectionBuilder::new(db, env), |builder, element| {
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
                arg.top_materialization(db, env)
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
                arg.bottom_materialization(db, env)
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
            SpecialFormType::TypeForm => {
                let arguments = if let ast::Expr::Tuple(tuple) = arguments_slice {
                    &*tuple.elts
                } else {
                    std::slice::from_ref(arguments_slice)
                };
                let type_argument = if let [argument] = arguments {
                    self.infer_type_expression(argument)
                } else {
                    let num_arguments = arguments.len();

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
                    self.store_expression_type(arguments_slice, type_argument);
                }
                TypeFormType::from_type_expression(db, type_argument)
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
                let Some(callable_type) = argument_type
                    .try_upcast_to_callable_with_recursive_fallback(
                        db,
                        env,
                        self.recursive_type_expression_definition(),
                    )
                    .map(|callables| {
                        if special_form == SpecialFormType::RegularCallableTypeOf {
                            callables
                                .map(|callable| callable.into_regular(db))
                                .to_type(db, env)
                        } else {
                            callables.to_type(db, env)
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
                            actual_type = argument_type.display(db, env)
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
                if self.inference_flags().intersects(
                    InferenceFlags::IN_PARAMETER_ANNOTATION
                        | InferenceFlags::IN_RETURN_TYPE
                        | InferenceFlags::IN_TYPE_ALIAS,
                ) {
                    self.report_invalid_type_expression(
                        subscript,
                        format_args!(
                            "Type qualifier `{qualifier}` is not allowed in {}s",
                            self.inference_flags().type_expression_context(),
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
                    let expanded = narrowed.expand_eagerly(db, env);

                    if expanded.is_divergent() {
                        expanded
                    } else {
                        TypeIsType::from_type_expression(self.db(), narrowed)
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
                _ => TypeGuardType::unbound(self.db(), self.infer_type_expression(arguments_slice)),
            },
            SpecialFormType::Concatenate => {
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "`typing.Concatenate` is not allowed in this context in a {}",
                        self.type_expression_context()
                    ));
                    diag.info("`typing.Concatenate` is only valid:");
                    diag.info(" - as the first argument to `Callable`");
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
                            .context
                            .inference_flags
                            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
                        self.infer_type_expression(argument);
                        self.context.inference_flags.set(
                            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                            previously_allowed_paramspec,
                        );
                    } else {
                        let previously_allowed_paramspec = self
                            .context
                            .inference_flags
                            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
                        self.infer_type_expression(argument);
                        self.context.inference_flags.set(
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
                self.store_type_expression_flags(
                    ast::ExprRef::from(subscript),
                    TypeExpressionFlags::UNPACK,
                );

                let inference_flags = self.inference_flags();
                let is_nested_unpack =
                    inference_flags.contains(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT);
                let is_nested_kwargs = inference_flags
                    .contains(InferenceFlags::IN_KWARG_ANNOTATION)
                    && inference_flags.contains(InferenceFlags::IN_NESTED_TYPE_EXPRESSION);
                let is_invalid_context = !inference_flags.intersects(
                    InferenceFlags::IN_VARARG_ANNOTATION
                        | InferenceFlags::IN_KWARG_ANNOTATION
                        | InferenceFlags::IN_VALID_UNPACK_CONTEXT,
                );

                let previously_in_unpack_type_argument = self
                    .context
                    .inference_flags
                    .replace(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, true);
                let inner_ty = if self.in_string_annotation()
                    && (is_nested_unpack || is_nested_kwargs || is_invalid_context)
                {
                    // Invalid string annotations never execute, so their operands must not
                    // produce runtime errors even though their inferred types are still needed.
                    let mut speculative = self.speculate_without_diagnostics();
                    let inner_ty = speculative.infer_type_expression(arguments_slice);
                    self.extend(speculative);
                    inner_ty
                } else {
                    self.infer_type_expression(arguments_slice)
                };
                self.context.inference_flags.set(
                    InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
                    previously_in_unpack_type_argument,
                );

                if is_nested_unpack {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        diagnostic::add_type_expression_reference_link(
                            builder.into_diagnostic("`Unpack` cannot be nested"),
                        );
                    }
                    return Type::unknown();
                }

                if is_nested_kwargs {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        diagnostic::add_type_expression_reference_link(builder.into_diagnostic(
                            "`Unpack` is only valid as the top-level `**kwargs` annotation form",
                        ));
                    }
                    return Type::unknown();
                }

                if is_invalid_context {
                    if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                        diagnostic::add_type_expression_reference_link(builder.into_diagnostic(
                            format_args!(
                                "`Unpack` is not allowed in {}s",
                                self.type_expression_context()
                            ),
                        ));
                    }
                    return Type::unknown();
                }

                if self
                    .inference_flags()
                    .contains(InferenceFlags::IN_KWARG_ANNOTATION)
                {
                    return inner_ty;
                }

                let inner_ty = inner_ty.resolve_type_alias(db);

                // Preserve valid unpack targets so that `Unpack[...]` follows the same
                // argument-binding path as an equivalent starred annotation.
                if inner_ty.exact_tuple_instance_spec(self.db()).is_some()
                    || matches!(
                        inner_ty,
                        Type::TypeVar(typevar) if typevar.is_typevartuple(self.db())
                    )
                {
                    inner_ty
                } else {
                    self.store_type_expression_flags(
                        ast::ExprRef::from(subscript),
                        TypeExpressionFlags::INVALID_UNPACK,
                    );
                    if !inner_ty.is_unknown()
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, subscript)
                    {
                        diagnostic::add_type_expression_reference_link(builder.into_diagnostic(
                            "`Unpack` can only unpack a tuple type or `TypeVarTuple`",
                        ));
                    }
                    Type::homogeneous_tuple(db, env, Type::unknown())
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
            | SpecialFormType::TypedDict(_)
            | SpecialFormType::Unknown
            | SpecialFormType::Divergent
            | SpecialFormType::Todo
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
                self.infer_expression(arguments_slice, TypeContext::default());
                if let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, subscript) {
                    let mut diag =
                        builder.into_diagnostic("`LiteralString` expects no type parameter");

                    if self
                        .speculate_without_diagnostics()
                        .infer_literal_parameter_type(arguments_slice)
                        .is_ok()
                    {
                        diag.annotate(
                            self.context
                                .secondary(&*subscript.value)
                                .message("Did you mean `Literal`?"),
                        );
                        diag.set_concise_message(
                            "`LiteralString` expects no type parameter - did you mean `Literal`?",
                        );
                        if let Some(action) = diagnostic::import_literal_for_fix(
                            &self.context,
                            subscript.value.start(),
                        ) {
                            diag.help("Replace `LiteralString` with `Literal`");
                            diag.set_fix(Fix::unsafe_edits(
                                Edit::range_replacement(
                                    action.symbol_text().to_string(),
                                    subscript.value.range(),
                                ),
                                action.import().cloned(),
                            ));
                        }
                    }
                }
                Type::unknown()
            }
            SpecialFormType::Type => self.infer_subclass_of_type_expression(arguments_slice),
            SpecialFormType::Tuple => self.infer_tuple_type_expression(subscript, super::local::tuple_annotation::ResultMode::Instance),
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
        let db = self.db();
        let env = self.program_environment();
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
                let mut builder = UnionBuilder::new(db, env);
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
                        let value_ty = type_alias.value_type(db);
                        if value_ty.is_literal_or_union_of_literals(db, env) {
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
                    Type::NominalInstance(_) if subscript_ty.is_enum(db, env) => {
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
    fn infer_callable_parameter_types(
        &mut self,
        parameters: &ast::Expr,
    ) -> Option<Parameters<'db>> {
        let db = self.db();
        match parameters {
            ast::Expr::EllipsisLiteral(ast::ExprEllipsisLiteral { .. }) => {
                return Some(Parameters::gradual_form());
            }
            ast::Expr::List(ast::ExprList { elts: params, .. }) => {
                if let [ast::Expr::EllipsisLiteral(_)] = &params[..] {
                    // Return `None` here so that we emit a specific diagnostic at the callsite.
                    return None;
                }

                let mut parameters = Vec::with_capacity(params.len());

                let previously_in_valid_unpack_context = self
                    .context
                    .inference_flags
                    .replace(InferenceFlags::IN_VALID_UNPACK_CONTEXT, true);
                for param in params {
                    let param_type = self.infer_type_expression(param);
                    parameters.push(self.callable_parameter_from_annotation(param, param_type));
                }
                self.context.inference_flags.set(
                    InferenceFlags::IN_VALID_UNPACK_CONTEXT,
                    previously_in_valid_unpack_context,
                );

                return Some(Parameters::from_annotation(db, parameters));
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
                    .context
                    .inference_flags
                    .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
                let parameters_type = self.infer_type_expression_no_store(parameters);
                self.context.inference_flags.set(
                    InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                    previously_allowed_paramspec,
                );
                if let Type::TypeVar(tvar) = parameters_type
                    && tvar.is_paramspec(self.db())
                {
                    return Some(Parameters::paramspec(db, tvar));
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
                    parse_string_annotation(&self.context, self.inference_flags(), string)
                {
                    self.string_annotations
                        .insert(ruff_python_ast::ExprRef::StringLiteral(string).into());
                    let node_key = self.enclosing_node_key(string.into());

                    let previous_deferred_state = self.replace_deferred_state(
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

    pub(super) fn callable_parameter_from_annotation(
        &self,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Parameter<'db> {
        if self.type_expression_flags(expression).contains(TypeExpressionFlags::UNPACK) {
            if let Type::TypeVar(typevar) = ty && typevar.is_typevartuple(self.db()) {
                return Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::TypeVar(typevar))
                    .with_starred_annotation();
            }
            if ty.exact_tuple_instance_spec(self.db()).is_some() {
                return Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(ty)
                    .with_starred_annotation();
            }
        }
        Parameter::positional_only(None).with_annotated_type(ty)
    }

    /// Infer the parameter types represented by a `typing.Concatenate` special form.
    pub(super) fn infer_concatenate_special_form(
        &mut self,
        subscript: &ast::ExprSubscript,
    ) -> Parameters<'db> {
        let db = self.db();
        let previous_concatenate_context = self
            .context
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
            .context
            .inference_flags
            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
        let prefix_params = prefix_args
            .iter()
            .map(|arg| {
                Parameter::positional_only(None)
                    .with_annotated_type(self.infer_type_expression(arg))
            })
            .collect();
        self.context.inference_flags.set(
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
            previously_allowed_paramspec,
        );

        let parameters = self
            .infer_concatenate_tail(last_arg)
            .map(|tail| Parameters::concatenate(db, prefix_params, tail));

        if arguments_slice.is_tuple_expr() {
            // TODO: What type to store for the argument slice in `Concatenate` because
            // `Parameters` is not a `Type` variant?
            self.store_expression_type(arguments_slice, Type::unknown());
        }

        let result = parameters.unwrap_or_else(Parameters::unknown);

        self.context.inference_flags.set(
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
                    .context
                    .inference_flags
                    .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
                let expr_type = self.infer_type_expression_no_store(expr);
                self.context.inference_flags.set(
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
                    parse_string_annotation(&self.context, self.inference_flags(), string)
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

                let previous_deferred_state = self
                    .replace_deferred_state(DeferredExpressionState::InStringAnnotation(node_key));
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

    fn check_type_variable_scope(&self, expression: &ast::Expr, ty: Type<'db>) -> Type<'db> {
        let Ok(ty) = variable_scope::check_type_variable_scope_sync(
            self,
            expression,
            ty,
            variable_scope::TypeVariableScopeFacts,
            &variable_scope::OrdinaryTypeVariableScopeEffects { db: self.db() },
        );
        ty
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    fn infer_recursive_subscript_type_expression(
        &mut self,
        subscript: &ast::ExprSubscript,
        alias: Type<'db>,
        parameters: GenericContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        self.infer_explicit_callable_specialization(subscript, alias, parameters, &|arguments| {
            alias.apply_specialization(
                db,
                parameters.specialize_partial(db, arguments.iter().copied()),
            )
        })
    }
}

#[derive(Clone, Copy)]
pub(in crate::types::infer) enum TypeExpressionMode {
    Scoped,
    ScopedWithState(DeferredExpressionState),
    NoStore,
}

pub(in crate::types::infer) enum TypeExpressionRequest<'db, 'expr> {
    Expression {
        expression: &'expr ast::Expr,
        mode: TypeExpressionMode,
    },
    ResolvedSubscript {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        definition: Option<Definition<'db>>,
    },
    ClassSubscript {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: ClassLiteral<'db>,
    },
    SubclassArgument {
        slice: &'expr ast::Expr,
    },
    ResolvedSubclassSubscript {
        slice: &'expr ast::Expr,
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
    },
}

pub(in crate::types::infer) enum TypeExpressionPending<'db, 'expr> {
    Identity,
    RestoreUnpack(Option<bool>),
    Starred {
        expression: &'expr ast::ExprStarred,
        previous_unpack: bool,
    },
    SubclassArgument(&'expr ast::Expr),
    SubclassReceiver {
        slice: &'expr ast::Expr,
        subscript: &'expr ast::ExprSubscript,
    },
    StoreSubclass(&'expr ast::Expr),
    ClassSpecialization,
    UnionLeft(&'expr ast::ExprBinOp),
    UnionRight(&'expr ast::ExprBinOp, Type<'db>),
}

pub(in crate::types::infer) enum TypeExpressionStep<'db, 'expr> {
    String(&'expr ast::ExprStringLiteral),
    Complete(Type<'db>),
    Infer {
        pending: TypeExpressionPending<'db, 'expr>,
        request: TypeExpressionRequest<'db, 'expr>,
    },
    ClassSpecialization {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    },
    RuntimeExpression {
        expression: &'expr ast::Expr,
        pending: TypeExpressionPending<'db, 'expr>,
    },
    SubclassSpecialization {
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
    },
    Callable(super::local::callable_annotation::Request<'expr>),
    Tuple(super::local::tuple_annotation::Request<'expr>),
}

pub(in crate::types::infer) struct TypeExpressionFacts;
pub(in crate::types::infer::builder) struct OrdinaryTypeExpressionEffects;

pub(in crate::types::infer) enum DottedNamePart {
    Name,
    Attribute,
    Other,
}

pub(in crate::types::infer) fn next_dotted_name<'expr>(
    cursor: &mut Option<&'expr ast::Expr>,
) -> Option<DottedNamePart> {
    match cursor.take()? {
        ast::Expr::Name(_) => Some(DottedNamePart::Name),
        ast::Expr::Attribute(attribute) => {
            *cursor = Some(&attribute.value);
            Some(DottedNamePart::Attribute)
        }
        _ => Some(DottedNamePart::Other),
    }
}

pub(in crate::types::infer) fn may_hide_recursive_alias(ty: Type<'_>) -> bool {
    matches!(
        ty,
        Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Recursive(_)
            | Type::TypeAlias(_)
            | Type::KnownInstance(
                KnownInstanceType::UnionType(_) | KnownInstanceType::LiteralStringAlias(_)
            )
    )
}

shared_semantic_family! {
    #[synchronous(SynchronousTypeExpressionEffects)]
    pub(in crate::types::infer) trait TypeExpressionEffects<'db, 'ast> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(checkpoint)]
        async fn subclass_checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(checkpoint)]
        async fn starred_checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn store_unpack_flag(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprStarred, flag: TypeExpressionFlags) -> Result<(), Self::Error>;
        /// Enables `IN_UNPACK_TYPE_ARGUMENT` and returns its previous value.
        #[operation(local)]
        async fn enter_starred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        /// Restores the unpack flag to the value returned by `enter_starred`.
        #[operation(local)]
        async fn restore_starred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, previous: bool) -> Result<(), Self::Error>;
        /// Resolves aliases in the starred expression's inferred operand type.
        #[operation(child)]
        async fn resolve_starred(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        /// Returns whether the operand is an exact tuple instance or a bound `TypeVarTuple`.
        #[operation(child)]
        async fn is_unpackable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn invalid_starred(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprStarred) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn unknown_tuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_dotted<'expr>(&self, cursor: &mut Option<&'expr ast::Expr>) -> Result<Option<DottedNamePart>, Self::Error>;
        #[operation(child)]
        async fn dotted(&self, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn reference(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(Type<'db>, Option<Definition<'db>>), Self::Error>;
        #[operation(child)]
        async fn finish_receiver(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn enter_unpack(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<bool>, Self::Error>;
        #[operation(local)]
        async fn restore_unpack(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, previous: Option<bool>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn convert_reference(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, reference: (Type<'db>, Option<Definition<'db>>)) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_reference(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, definition: Option<Definition<'db>>) -> Result<Option<(Type<'db>, Option<GenericContext<'db>>)>, Self::Error>;
        #[operation(child)]
        async fn has_alias_shape(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn resolve_recursive(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Option<(Type<'db>, Option<GenericContext<'db>>)>, Self::Error>;
        #[operation(child)]
        async fn specialize_recursive(&self, builder: &TypeInferenceBuilder<'db, 'ast>, alias: Type<'db>, parameters: GenericContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_subscript(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, alias: Type<'db>, parameters: GenericContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn known_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: crate::types::ClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn class_subscript<'expr>(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, class: ClassLiteral<'db>) -> Result<TypeExpressionStep<'db, 'expr>, Self::Error>;
        #[operation(child)]
        async fn class_generic_context(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(local)]
        async fn in_string_annotation(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn non_generic_class_slice(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn invalid_class_subscript(&self, builder: &TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, class: ClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn tuple<'expr>(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &'expr ast::ExprSubscript, mode: super::local::tuple_annotation::ResultMode) -> Result<TypeExpressionStep<'db, 'expr>, Self::Error>;
        #[operation(child)]
        async fn subscript(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, value_ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn none(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn annotation_is_deferred(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn annotation_in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn annotation_in_type_checking_block(&self, builder: &TypeInferenceBuilder<'db, 'ast>, binary: &ast::ExprBinOp) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn union_runtime_validation(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, binary: &ast::ExprBinOp) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn union(&self, builder: &TypeInferenceBuilder<'db, 'ast>, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn legacy(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn legacy_subclass(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn resolved_subclass_subscript(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, subscript: &ast::ExprSubscript, value_ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn store_subclass(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn convert_subclass(&self, builder: &TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn paramspec_attribute(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: crate::types::BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn missing_arguments(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn default_specialize(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn in_type_expression(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn invalid(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, error: InvalidTypeExpressionError<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn variable_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn normalize_subclass(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass_from_instance(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Result<Type<'db>, Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn invalid_subclass(&self, builder: &TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TypeExpressionFacts {
        fn string<'db, 'expr>(&self, string: &'expr ast::ExprStringLiteral) -> TypeExpressionStep<'db, 'expr> { TypeExpressionStep::String(string) }
        fn name_context(&self, name: &ast::ExprName) -> ast::ExprContext { name.ctx }
        fn attribute_context(&self, attribute: &ast::ExprAttribute) -> ast::ExprContext { attribute.ctx }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn is_unknown(&self, ty: Type<'_>) -> bool { ty.is_unknown() }
        fn starred_request<'db, 'expr>(&self, expression: &'expr ast::ExprStarred, previous_unpack: bool) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer {
                pending: TypeExpressionPending::Starred { expression, previous_unpack },
                request: TypeExpressionRequest::Expression { expression: &expression.value, mode: TypeExpressionMode::Scoped },
            }
        }
        fn invalid_name<'db>(&self) -> Type<'db> { todo_type!("Name expression annotation in Store/Del context") }
        fn invalid_attribute<'db>(&self) -> Type<'db> { todo_type!("Attribute expression annotation in Store/Del context") }
        fn complete<'db, 'expr>(&self, ty: Type<'db>) -> TypeExpressionStep<'db, 'expr> { TypeExpressionStep::Complete(ty) }
        fn callable_request<'db, 'expr>(&self, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>) -> Option<TypeExpressionStep<'db, 'expr>> {
            if matches!(value_ty, Type::SpecialForm(SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable)) {
                super::local::callable_annotation::request(subscript).map(TypeExpressionStep::Callable)
            } else { None }
        }
        fn static_class<'db>(&self, class: ClassLiteral<'db>) -> Option<StaticClassLiteral<'db>> { class.as_static() }
        fn slice<'expr>(&self, subscript: &'expr ast::ExprSubscript) -> &'expr ast::Expr { &subscript.slice }
        fn class_specialization<'db, 'expr>(&self, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::ClassSpecialization { subscript, value_ty, class, generic_context }
        }
        fn subscript_request<'db, 'expr>(&self, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, definition: Option<Definition<'db>>, previous: Option<bool>) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer { pending: TypeExpressionPending::RestoreUnpack(previous), request: TypeExpressionRequest::ResolvedSubscript { subscript, value_ty, definition } }
        }
        fn subclass_request<'db, 'expr>(&self, subscript: &'expr ast::ExprSubscript) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer { pending: TypeExpressionPending::Identity, request: TypeExpressionRequest::SubclassArgument { slice: &subscript.slice } }
        }
        fn argument_request<'db, 'expr>(&self, slice: &'expr ast::Expr) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer { pending: TypeExpressionPending::SubclassArgument(slice), request: TypeExpressionRequest::Expression { expression: slice, mode: TypeExpressionMode::Scoped } }
        }
        fn subclass_receiver<'db, 'expr>(&self, slice: &'expr ast::Expr, subscript: &'expr ast::ExprSubscript) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::RuntimeExpression { expression: &subscript.value, pending: TypeExpressionPending::SubclassReceiver { slice, subscript } }
        }
        fn resolved_subclass<'db, 'expr>(&self, slice: &'expr ast::Expr, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer { pending: TypeExpressionPending::StoreSubclass(slice), request: TypeExpressionRequest::ResolvedSubclassSubscript { slice, subscript, value_ty } }
        }
        fn subclass_specialization<'db, 'expr>(&self, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::SubclassSpecialization { subscript, value_ty, class, generic_context }
        }
        fn is_type_argument(&self, slice: &ast::Expr) -> bool {
            matches!(slice, ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::StringLiteral(_))
                || matches!(slice, ast::Expr::BinOp(binary) if matches!(binary.op, ast::Operator::BitOr | ast::Operator::BitAnd))
        }
        fn attribute(&self, expression: &ast::Expr) -> bool { expression.is_attribute_expr() }
        fn callable(&self, ty: Type<'_>) -> bool { matches!(ty, Type::Callable(_)) }
        fn is_union(&self, binary: &ast::ExprBinOp) -> bool { binary.op == ast::Operator::BitOr }
        fn union_left<'db, 'expr>(&self, binary: &'expr ast::ExprBinOp) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer { pending: TypeExpressionPending::UnionLeft(binary), request: TypeExpressionRequest::Expression { expression: &binary.left, mode: TypeExpressionMode::Scoped } }
        }
        fn union_right<'db, 'expr>(&self, binary: &'expr ast::ExprBinOp, left: Type<'db>) -> TypeExpressionStep<'db, 'expr> {
            TypeExpressionStep::Infer { pending: TypeExpressionPending::UnionRight(binary, left), request: TypeExpressionRequest::Expression { expression: &binary.right, mode: TypeExpressionMode::Scoped } }
        }
        fn unsupported_subclass<'db>(&self) -> Type<'db> { todo_type!("unsupported type[X] special form") }
    }

    #[synchronous(dotted_name_sync)]
    #[capabilities(effects = TypeExpressionEffects)]
    #[passive_values()]
    pub(in crate::types::infer) async fn dotted_name_with<'db, 'ast, E: TypeExpressionEffects<'db, 'ast>>(expression: &ast::Expr, effects: &E) -> Result<bool, E::Error> {
        #[passive_state]
        let mut cursor = Some(expression);
        #[cursor_loop]
        while let Some(part) = effects.next_dotted(&mut cursor).await? {
            match part {
                DottedNamePart::Name => return Ok(true),
                DottedNamePart::Attribute => {},
                DottedNamePart::Other => return Ok(false),
            }
        }
        Ok(false)
    }

    #[synchronous(recursive_alias_reference_sync)]
    #[capabilities(effects = TypeExpressionEffects)]
    #[passive_values()]
    pub(in crate::types::infer) async fn recursive_alias_reference_with<'db, 'ast, E: TypeExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, definition: Option<Definition<'db>>, _facts: TypeExpressionFacts, effects: &E,
    ) -> Result<Option<(Type<'db>, Option<GenericContext<'db>>)>, E::Error> {
        let Some(definition) = definition else { return Ok(None); };
        // A resolved non-recursive value already describes the alias. Gradual types, unions,
        // and quoted aliases can hide recursive references, so they still need inference.
        // Even a valid union can have lost a cyclic member during value inference.
        if !effects.has_alias_shape(builder, ty).await? { return Ok(None); }
        effects.resolve_recursive(builder, definition).await
    }

    #[synchronous(convert_reference_sync)]
    #[capabilities(effects = TypeExpressionEffects, facts = TypeExpressionFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn convert_reference_with<'db, 'ast, E: TypeExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, reference: (Type<'db>, Option<Definition<'db>>), facts: TypeExpressionFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let (ty, definition) = reference;
        if let Some((alias, parameters)) = effects.recursive_reference(builder, ty, definition).await? {
            return match parameters {
                Some(parameters) => effects.specialize_recursive(builder, alias, parameters).await,
                None => Ok(alias),
            };
        }
        if facts.attribute(expression)
            && let Type::TypeVar(typevar) = ty
            && effects.paramspec_attribute(builder, typevar).await?
        { return Ok(ty); }
        effects.missing_arguments(builder, ty, expression).await?;
        let specialized = effects.default_specialize(builder, ty).await?;
        let result = match effects.in_type_expression(builder, specialized).await? {
            Ok(ty) => ty,
            Err(error) => effects.invalid(builder, expression, error).await?,
        };
        match result {
            Type::TypeVar(_) | Type::KnownInstance(KnownInstanceType::TypeVar(_)) => effects.variable_scope(builder, expression, result).await,
            _ => Ok(result),
        }
    }

    #[synchronous(convert_subclass_argument_sync)]
    #[capabilities(effects = TypeExpressionEffects, facts = TypeExpressionFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn convert_subclass_argument_with<'db, 'ast, E: TypeExpressionEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>, slice: &ast::Expr, ty: Type<'db>, facts: TypeExpressionFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let ty = effects.normalize_subclass(builder, ty).await?;
        match effects.subclass_from_instance(builder, ty).await? {
            Ok(ty) => Ok(ty),
            Err(unsupported) => {
                if facts.callable(unsupported) { effects.invalid_subclass(builder, slice).await }
                else { Ok(facts.unsupported_subclass()) }
            }
        }
    }

    #[synchronous(class_subscript_sync)]
    #[capabilities(effects = TypeExpressionEffects, facts = TypeExpressionFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn class_subscript_with<'db, 'ast, 'expr, E: TypeExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, class: ClassLiteral<'db>, facts: TypeExpressionFacts, effects: &E,
    ) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
        if let Some(generic_context) = effects.class_generic_context(builder, class).await?
            && let Some(class) = facts.static_class(class)
        {
            return Ok(facts.class_specialization(subscript, value_ty, class, generic_context));
        }
        if !effects.in_string_annotation(builder).await? {
            effects.non_generic_class_slice(builder, facts.slice(subscript)).await?;
        }
        effects.invalid_class_subscript(builder, subscript, class).await?;
        Ok(facts.complete(facts.unknown()))
    }

    #[synchronous(start_type_expression_impl_sync)]
    #[capabilities(effects = TypeExpressionEffects, facts = TypeExpressionFacts)]
    #[passive_values(TupleAnnotationResultMode::Instance, TupleAnnotationResultMode::Subclass, TypeExpressionFlags::UNPACK)]
    async fn start_type_expression_impl_with<'db, 'ast, 'expr, E: TypeExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, request: TypeExpressionRequest<'db, 'expr>, facts: TypeExpressionFacts, effects: &E,
    ) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
        effects.checkpoint().await?;
        match request {
            TypeExpressionRequest::Expression { expression, .. } => {
                match expression {
                    ast::Expr::Name(name) => {
                        let ty = match facts.name_context(name) {
                            ast::ExprContext::Load => {
                                let reference = effects.reference(builder, expression).await?;
                                effects.convert_reference(builder, expression, reference).await?
                            }
                            ast::ExprContext::Invalid => facts.unknown(),
                            ast::ExprContext::Store | ast::ExprContext::Del => facts.invalid_name(),
                        };
                        Ok(facts.complete(ty))
                    }
                    ast::Expr::Attribute(attribute) => {
                        if !effects.dotted(expression).await? {
                            let ty = effects.legacy(builder, expression).await?;
                            return Ok(facts.complete(ty));
                        }
                        let ty = match facts.attribute_context(attribute) {
                            ast::ExprContext::Load => {
                                let reference = effects.reference(builder, expression).await?;
                                effects.convert_reference(builder, expression, reference).await?
                            }
                            ast::ExprContext::Invalid => facts.unknown(),
                            ast::ExprContext::Store | ast::ExprContext::Del => facts.invalid_attribute(),
                        };
                        Ok(facts.complete(ty))
                    }
                    ast::Expr::Subscript(subscript) => {
                        if effects.dotted(&subscript.value).await? {
                            let (ty, definition) = effects.reference(builder, &subscript.value).await?;
                            let ty = effects.finish_receiver(builder, &subscript.value, ty).await?;
                            let previous = effects.enter_unpack(builder, ty).await?;
                            Ok(facts.subscript_request(subscript, ty, definition, previous))
                        } else {
                            let ty = effects.legacy(builder, expression).await?;
                            Ok(facts.complete(ty))
                        }
                    }
                    ast::Expr::Starred(starred) => {
                        effects.starred_checkpoint().await?;
                        effects.store_unpack_flag(builder, starred, TypeExpressionFlags::UNPACK).await?;
                        let previous = effects.enter_starred(builder).await?;
                        Ok(facts.starred_request(starred, previous))
                    }
                    ast::Expr::StringLiteral(string) => Ok(facts.string(string)),
                    ast::Expr::NoneLiteral(_) => {
                        let ty = effects.none(builder).await?;
                        Ok(facts.complete(ty))
                    }
                    // PEP-604 unions are okay, e.g., `int | str`
                    ast::Expr::BinOp(binary) if facts.is_union(binary) => Ok(facts.union_left(binary)),
                    _ => { let ty = effects.legacy(builder, expression).await?; Ok(facts.complete(ty)) }
                }
            }
            TypeExpressionRequest::ResolvedSubscript { subscript, value_ty, definition } => {
                if let Some((alias, Some(parameters))) = effects.recursive_reference(builder, value_ty, definition).await? {
                    let ty = effects.recursive_subscript(builder, subscript, alias, parameters).await?;
                    return Ok(facts.complete(ty));
                }
                if let Type::ClassLiteral(class) = value_ty {
                    match effects.known_class(builder, class).await? {
                        Some(KnownClass::Type) => return Ok(facts.subclass_request(subscript)),
                        Some(KnownClass::Tuple) => {
                            return effects.tuple(builder, subscript, TupleAnnotationResultMode::Instance).await;
                        }
                        _ => {},
                    }
                    return effects.class_subscript(builder, subscript, value_ty, class).await;
                }
                if let Type::SpecialForm(SpecialFormType::Tuple) = value_ty { return effects.tuple(builder, subscript, TupleAnnotationResultMode::Instance).await; }
                if let Some(step) = facts.callable_request(subscript, value_ty) { return Ok(step); }
                let ty = effects.subscript(builder, subscript, value_ty).await?;
                Ok(facts.complete(ty))
            }
            TypeExpressionRequest::ClassSubscript { subscript, value_ty, class } => {
                effects.class_subscript(builder, subscript, value_ty, class).await
            }
            TypeExpressionRequest::SubclassArgument { slice } => {
                if facts.is_type_argument(slice) { return Ok(facts.argument_request(slice)); }
                if let ast::Expr::Subscript(subscript) = slice {
                    if !effects.dotted(&subscript.value).await? {
                        return Ok(facts.argument_request(slice));
                    }
                    effects.subclass_checkpoint().await?;
                    return Ok(facts.subclass_receiver(slice, subscript));
                }
                let ty = effects.legacy_subclass(builder, slice).await?;
                Ok(facts.complete(ty))
            }
            TypeExpressionRequest::ResolvedSubclassSubscript { slice, subscript, value_ty } => {
                effects.subclass_checkpoint().await?;
                if let Type::ClassLiteral(class) = value_ty {
                    match effects.known_class(builder, class).await? {
                        Some(KnownClass::Tuple) => return effects.tuple(builder, subscript, TupleAnnotationResultMode::Subclass).await,
                        _ => {},
                    }
                    if let Some(generic_context) = effects.class_generic_context(builder, class).await?
                        && let Some(class) = facts.static_class(class)
                    {
                        return Ok(facts.subclass_specialization(subscript, value_ty, class, generic_context));
                    }
                    if !effects.in_string_annotation(builder).await? {
                        effects.non_generic_class_slice(builder, facts.slice(subscript)).await?;
                    }
                    effects.invalid_class_subscript(builder, subscript, class).await?;
                    return Ok(facts.complete(facts.unknown()));
                }
                let ty = effects.resolved_subclass_subscript(builder, slice, subscript, value_ty).await?;
                Ok(facts.complete(ty))
            }
        }
    }

    #[synchronous(resume_type_expression_impl_sync)]
    #[capabilities(effects = TypeExpressionEffects, facts = TypeExpressionFacts)]
    #[passive_values(TypeExpressionFlags::INVALID_UNPACK)]
    async fn resume_type_expression_impl_with<'db, 'ast, 'expr, E: TypeExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, pending: TypeExpressionPending<'db, 'expr>, child_ty: Type<'db>, facts: TypeExpressionFacts, effects: &E,
    ) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
        match pending {
            TypeExpressionPending::Starred { expression, previous_unpack } => {
                effects.starred_checkpoint().await?;
                let ty = effects.resolve_starred(builder, child_ty).await?;
                effects.restore_starred(builder, previous_unpack).await?;
                if effects.is_unpackable(builder, ty).await? {
                    return Ok(facts.complete(ty));
                }
                effects.store_unpack_flag(builder, expression, TypeExpressionFlags::INVALID_UNPACK).await?;
                if !facts.is_unknown(ty) {
                    effects.invalid_starred(builder, expression).await?;
                }
                let ty = effects.unknown_tuple(builder).await?;
                Ok(facts.complete(ty))
            }
            TypeExpressionPending::UnionLeft(binary) => Ok(facts.union_right(binary, child_ty)),
            TypeExpressionPending::UnionRight(binary, left) => {
                if !effects.annotation_is_deferred(builder).await?
                    && !effects.annotation_in_stub(builder).await?
                    && !effects.annotation_in_type_checking_block(builder, binary).await?
                {
                    effects.union_runtime_validation(builder, binary).await?;
                }
                let ty = effects.union(builder, left, child_ty).await?;
                Ok(facts.complete(ty))
            }
            TypeExpressionPending::SubclassReceiver { slice, subscript } => {
                effects.subclass_checkpoint().await?;
                Ok(facts.resolved_subclass(slice, subscript, child_ty))
            }
            TypeExpressionPending::StoreSubclass(slice) => {
                effects.subclass_checkpoint().await?;
                effects.store_subclass(builder, slice, child_ty).await?;
                Ok(facts.complete(child_ty))
            }
            TypeExpressionPending::Identity => Ok(facts.complete(child_ty)),
            TypeExpressionPending::RestoreUnpack(previous) => {
                effects.restore_unpack(builder, previous).await?;
                Ok(facts.complete(child_ty))
            }
            TypeExpressionPending::SubclassArgument(slice) => {
                let ty = effects.convert_subclass(builder, slice, child_ty).await?;
                Ok(facts.complete(ty))
            }
            TypeExpressionPending::ClassSpecialization => {
                let ty = match effects.in_type_expression(builder, child_ty).await? {
                    Ok(ty) => ty,
                    Err(_) => facts.unknown(),
                };
                Ok(facts.complete(ty))
            }
        }
    }
}

pub(in crate::types::infer) async fn start_type_expression_with<
    'db,
    'ast,
    'expr,
    E: TypeExpressionEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    request: TypeExpressionRequest<'db, 'expr>,
    effects: &E,
) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
    start_type_expression_impl_with(builder, request, TypeExpressionFacts, effects).await
}
pub(in crate::types::infer) fn start_type_expression_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousTypeExpressionEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    request: TypeExpressionRequest<'db, 'expr>,
    effects: &E,
) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
    start_type_expression_impl_sync(builder, request, TypeExpressionFacts, effects)
}
pub(in crate::types::infer) async fn resume_type_expression_with<
    'db,
    'ast,
    'expr,
    E: TypeExpressionEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    pending: TypeExpressionPending<'db, 'expr>,
    child_ty: Type<'db>,
    effects: &E,
) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
    resume_type_expression_impl_with(builder, pending, child_ty, TypeExpressionFacts, effects).await
}
pub(in crate::types::infer) fn resume_type_expression_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousTypeExpressionEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    pending: TypeExpressionPending<'db, 'expr>,
    child_ty: Type<'db>,
    effects: &E,
) -> Result<TypeExpressionStep<'db, 'expr>, E::Error> {
    resume_type_expression_impl_sync(builder, pending, child_ty, TypeExpressionFacts, effects)
}

pub(in crate::types::infer) fn enter_subscript_unpack(
    builder: &mut TypeInferenceBuilder<'_, '_>,
    ty: Type<'_>,
) -> Option<bool> {
    // Preserve the flag for another `Unpack` so that nested unpacking emits a
    // diagnostic. Other subscripts are no longer the direct unpack operand.
    if ty == Type::SpecialForm(SpecialFormType::Unpack) {
        None
    } else {
        Some(
            builder
                .context
                .inference_flags
                .replace(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, false),
        )
    }
}

pub(in crate::types::infer) fn restore_subscript_unpack(
    builder: &mut TypeInferenceBuilder<'_, '_>,
    previous: Option<bool>,
) {
    if let Some(previous) = previous {
        builder
            .context
            .inference_flags
            .set(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, previous);
    }
}

impl<'db, 'ast> SynchronousTypeExpressionEffects<'db, 'ast> for OrdinaryTypeExpressionEffects {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn subclass_checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn starred_checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn store_unpack_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::ExprStarred,
        flag: TypeExpressionFlags,
    ) -> Result<(), Infallible> {
        builder.store_type_expression_flags(ast::ExprRef::from(expression), flag);
        Ok(())
    }
    fn enter_starred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(builder.context.inference_flags.replace(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, true))
    }
    fn restore_starred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, previous: bool) -> Result<(), Infallible> {
        builder.context.inference_flags.set(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT, previous);
        Ok(())
    }
    fn resolve_starred(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.resolve_type_alias(builder.db()))
    }
    fn is_unpackable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty.exact_tuple_instance_spec(builder.db()).is_some()
            || matches!(ty, Type::TypeVar(variable) if variable.is_typevartuple(builder.db())))
    }
    fn invalid_starred(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprStarred) -> Result<(), Infallible> {
        if let Some(builder) = builder.context.report_lint(&INVALID_TYPE_FORM, expression) {
            diagnostic::add_type_expression_reference_link(
                builder.into_diagnostic("`*` can only unpack a tuple type or `TypeVarTuple`"),
            );
        }
        Ok(())
    }
    fn unknown_tuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Infallible> {
        Ok(Type::homogeneous_tuple(builder.db(), builder.program_environment(), Type::unknown()))
    }
    fn next_dotted<'expr>(
        &self,
        cursor: &mut Option<&'expr ast::Expr>,
    ) -> Result<Option<DottedNamePart>, Infallible> {
        Ok(next_dotted_name(cursor))
    }
    fn dotted(&self, expression: &ast::Expr) -> Result<bool, Infallible> {
        dotted_name_sync(expression, self)
    }
    fn reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(Type<'db>, Option<Definition<'db>>), Infallible> {
        Ok(builder.infer_type_expression_reference(expression))
    }
    fn finish_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.finish_expression_type(expression, ty, TypeContext::default()))
    }
    fn enter_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<bool>, Infallible> {
        Ok(enter_subscript_unpack(builder, ty))
    }
    fn restore_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        previous: Option<bool>,
    ) -> Result<(), Infallible> {
        restore_subscript_unpack(builder, previous);
        Ok(())
    }
    fn convert_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        reference: (Type<'db>, Option<Definition<'db>>),
    ) -> Result<Type<'db>, Infallible> {
        convert_reference_sync(builder, expression, reference, TypeExpressionFacts, self)
    }
    fn recursive_reference(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<Option<(Type<'db>, Option<GenericContext<'db>>)>, Infallible> {
        Ok(builder.recursive_implicit_alias_reference(ty, definition))
    }
    fn has_alias_shape(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(any_over_type(
            builder.db(),
            builder.program_environment(),
            ty,
            false,
            may_hide_recursive_alias,
        ))
    }
    fn resolve_recursive(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<(Type<'db>, Option<GenericContext<'db>>)>, Infallible> {
        Ok(builder.resolve_recursive_implicit_alias_reference(definition))
    }
    fn specialize_recursive(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        alias: Type<'db>,
        parameters: GenericContext<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(alias.apply_specialization(
            builder.db(),
            parameters.default_specialization(builder.db(), None),
        ))
    }
    fn recursive_subscript(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        alias: Type<'db>,
        parameters: GenericContext<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_recursive_subscript_type_expression(subscript, alias, parameters))
    }
    fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: crate::types::ClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(builder.db()))
    }
    fn class_subscript<'expr>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<TypeExpressionStep<'db, 'expr>, Infallible> {
        class_subscript_sync(
            builder,
            subscript,
            value_ty,
            class,
            TypeExpressionFacts,
            self,
        )
    }
    fn class_generic_context(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(class.generic_context(builder.db()))
    }
    fn in_string_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder.in_string_annotation())
    }
    fn non_generic_class_slice(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.infer_expression(slice, TypeContext::default());
        Ok(())
    }
    fn invalid_class_subscript(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        class: ClassLiteral<'db>,
    ) -> Result<(), Infallible> {
        builder.report_invalid_type_expression(
            subscript,
            format_args!(
                "Non-generic class `{}` cannot be specialized in a type expression",
                class.name(builder.db())
            ),
        );
        Ok(())
    }
    fn tuple<'expr>(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &'expr ast::ExprSubscript,
        mode: super::local::tuple_annotation::ResultMode,
    ) -> Result<TypeExpressionStep<'db, 'expr>, Infallible> {
        Ok(TypeExpressionStep::Tuple(super::local::tuple_annotation::Request { subscript, mode }))
    }
    fn subscript(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_subscript_type_expression(subscript, value_ty))
    }
    fn none(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Infallible> {
        Ok(Type::none(builder.db(), builder.program_environment()))
    }
    fn annotation_is_deferred(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder.deferred_state.is_deferred())
    }
    fn annotation_in_stub(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder.in_stub())
    }
    fn annotation_in_type_checking_block(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        binary: &ast::ExprBinOp,
    ) -> Result<bool, Infallible> {
        Ok(builder.is_in_type_checking_block(builder.scope(), binary))
    }
    fn union_runtime_validation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        binary: &ast::ExprBinOp,
    ) -> Result<(), Infallible> {
        builder.validate_union_type_expression_runtime(binary);
        Ok(())
    }
    fn union(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(UnionType::from_elements_leave_aliases(
            builder.db(),
            builder.program_environment(),
            [left, right],
        ))
    }
    fn legacy(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_type_expression_legacy_no_store(expression))
    }
    fn legacy_subclass(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_subclass_of_type_expression_legacy(slice))
    }
    fn resolved_subclass_subscript(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_resolved_subclass_subscript_legacy(slice, subscript, value_ty))
    }
    fn store_subclass(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.store_expression_type(slice, ty);
        Ok(())
    }
    fn convert_subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        convert_subclass_argument_sync(builder, slice, ty, TypeExpressionFacts, self)
    }
    fn paramspec_attribute(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: crate::types::BoundTypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty.paramspec_attr(builder.db()).is_some())
    }
    fn missing_arguments(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        expression: &ast::Expr,
    ) -> Result<(), Infallible> {
        report_missing_type_arguments(&builder.context, ty, expression);
        Ok(())
    }
    fn default_specialize(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.default_specialize(builder.db(), builder.program_environment()))
    }
    fn in_type_expression(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Infallible> {
        Ok(ty.in_type_expression(
            builder.db(),
            builder.scope(),
            builder.typevar_binding_context,
            builder.inference_flags(),
        ))
    }
    fn invalid(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        error: InvalidTypeExpressionError<'db>,
    ) -> Result<Type<'db>, Infallible> {
        if error
            .invalid_expressions
            .iter()
            .any(|invalid| matches!(invalid, InvalidTypeExpression::InvalidBareTypeVarTuple(_)))
        {
            builder.store_type_expression_flags(
                expression,
                TypeExpressionFlags::INVALID_BARE_TYPE_VAR_TUPLE,
            );
        }
        Ok(error.into_fallback_type(&builder.context, expression, builder.inference_flags()))
    }
    fn variable_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.check_type_variable_scope(expression, ty))
    }
    fn normalize_subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        crate::types::type_expression_conversion::normalize_subclass_argument_sync(
            builder.db(),
            builder.program_environment(),
            ty,
            &crate::types::type_expression_conversion::InlineConversion { db: builder.db() },
        )
    }
    fn subclass_from_instance(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Result<Type<'db>, Type<'db>>, Infallible> {
        Ok(SubclassOfType::try_from_instance(
            builder.db(),
            builder.program_environment(),
            ty,
        ))
    }
    fn invalid_subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        builder.report_invalid_type_expression(
            slice,
            "The argument to `type[]` must be a class object type",
        );
        Ok(SubclassOfType::subclass_of_unknown())
    }
}
