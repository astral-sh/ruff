//! Diagnostic messages and fixes for conditions selected by the redundant-condition checker.

use std::borrow::Cow;

use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
    source::source_text,
};
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::{
    self as ast, PythonVersion,
    helpers::any_over_expr,
    token::{TokenKind, Tokens},
};
use ruff_python_trivia::indentation_at_offset;
use ruff_source_file::{LineRanges, UniversalNewlineIterator, find_newline};
use ruff_text_size::{Ranged, TextSize};
use ty_module_resolver::{SearchPath, file_to_module};
use ty_python_core::{
    Truthiness,
    ast_ids::HasScopedUseId,
    definition::DefinitionKind,
    place::PlaceExpr,
    predicate::{Predicate, PredicateNode},
    scope::{NodeWithScopeKind, ScopeKind},
};

use crate::{
    SemanticModel,
    importer::ImportRequest,
    place::{Place, PlaceAndQualifiers},
    place_load::{PlaceLoadMode, PlaceLoadResolutionStep, resolve_place_load},
    types::{
        KnownClass, LintDiagnosticGuard, LintDiagnosticGuardBuilder, MemberLookupPolicy, Type,
        TypeContext,
        call::bind::CallableDescription,
        diagnostic::typing_module_for_fix,
        enum_metadata,
        function::KnownFunction,
        infer::TypeInferenceBuilder,
        infer_definition_types, infer_scope_types,
        narrow::{NarrowingConstraint, infer_narrowing_constraints},
        signatures::CallableSignature,
        tuple::{Tuple, TupleLength},
    },
};

use super::{ConditionKind, RedundantCondition, exemptions::condition_definition_info};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn report_redundant_condition<'ctx>(
        &'ctx self,
        condition: &RedundantCondition<'_, 'db>,
    ) -> Option<LintDiagnosticGuard<'ctx, 'ctx>> {
        #[derive(Debug)]
        enum FunctionInfo<'db> {
            Function(&'db CallableSignature<'db>, &'db str),
            Method(&'db CallableSignature<'db>, Cow<'db, str>),
            Lambda(&'db CallableSignature<'db>),
        }

        impl<'db> FunctionInfo<'db> {
            fn kind(&self) -> &'static str {
                match self {
                    FunctionInfo::Function(..) | FunctionInfo::Lambda(..) => "function",
                    FunctionInfo::Method(_, _) => "method",
                }
            }

            fn signature(&self) -> &'db CallableSignature<'db> {
                match self {
                    FunctionInfo::Function(signature, _) => signature,
                    FunctionInfo::Method(signature, _) => signature,
                    FunctionInfo::Lambda(signature) => signature,
                }
            }
        }

        impl std::fmt::Display for FunctionInfo<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    FunctionInfo::Function(_, name) => write!(f, "Function `{name}`"),
                    FunctionInfo::Method(_, name) => write!(f, "Method `{name}`"),
                    FunctionInfo::Lambda(_) => write!(f, "Function object"),
                }
            }
        }

        let RedundantCondition {
            expression: test,
            value_type: test_type,
            is_truthy,
            kind,
        } = condition;

        let rule = kind.rule();
        let db = self.db();
        let env = self.program_environment();

        // Quoting a nested test identifies which part of the enclosing condition is redundant.
        let should_quote_test_expression = || {
            !matches!(test, ast::Expr::Name(_) | ast::Expr::Attribute(_))
                || !self.index.is_boolean_test_root(test)
        };

        let annotate_inferred_type = |diagnostic: &mut LintDiagnosticGuard| {
            diagnostic.set_primary_annotation_message(format_args!(
                "Inferred type is `{}`",
                test_type.display(db, env)
            ));
        };

        let describe_condition = |diagnostic: &mut LintDiagnosticGuard| {
            let source = source_text(db, self.file());
            if source.contains_line_break(test.range()) {
                diagnostic.set_concise_message(format_args!("Condition is always {is_truthy}"));
            } else {
                diagnostic.set_concise_message(format_args!(
                    "Condition `{}` is always {is_truthy}",
                    &source[test.range()]
                ));
            }

            if let ast::Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                ..
            }) = test
                && ops.len() == 1
                && let [single_comparator] = &**comparators
            {
                if let (Type::LiteralValue(left_type), Type::LiteralValue(right_type)) = (
                    self.expression_type(left),
                    self.expression_type(single_comparator),
                ) && ((left_type.is_string() && (right_type.is_bytes() || right_type.is_int()))
                    || ((left_type.is_bytes() || left_type.is_int()) && right_type.is_string()))
                {
                    // For the specific case of a string-literal type compared with a bytes-literal type,
                    // cite their nominal-instance supertypes rather than their `Literal` types,
                    // since their `Literal` types look quite similar in their display representations.
                    for node in [left, single_comparator] {
                        if let Some(class) = self.expression_type(node).nominal_class(db, env) {
                            diagnostic.annotate(
                                self.context
                                    .secondary(node)
                                    .message(format_args!("Instance of `{}`", class.name(db))),
                            );
                        }
                    }
                } else {
                    for node in [left, single_comparator] {
                        match node {
                            ast::Expr::NoneLiteral(_)
                            | ast::Expr::BooleanLiteral(_)
                            | ast::Expr::NumberLiteral(_) => {}
                            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                                op: ast::UnaryOp::USub | ast::UnaryOp::UAdd,
                                operand,
                                ..
                            }) if matches!(&**operand, ast::Expr::NumberLiteral(_)) => {}
                            _ => {
                                diagnostic.annotate(self.context.secondary(node).message(
                                    format_args!(
                                        "Has type `{}`",
                                        self.expression_type(node).display(db, env)
                                    ),
                                ));
                            }
                        }
                    }
                }
            } else {
                annotate_inferred_type(diagnostic);
            }
        };

        // Short-circuit evaluation can determine a condition's truthiness even when its
        // value type does not. In that case, describe the condition rather than the type.
        let describe_as_condition =
            test_type.is_subtype_of(db, env, KnownClass::Bool.to_instance(db, env))
                || test_type.bool(db, env) != Truthiness::from(*is_truthy);

        let builder = self.context.report_lint(rule, test)?;
        let diagnostic = if *is_truthy {
            let add_always_truthy_concise_message = |diagnostic: &mut LintDiagnosticGuard| {
                if should_quote_test_expression()
                    && let source = source_text(db, self.file())
                    && !source.contains_line_break(test.range())
                {
                    diagnostic.set_concise_message(format_args!(
                        "{} `{}` is always truthy (has type `{}`)",
                        if matches!(test, ast::Expr::Name(_)) {
                            "Variable"
                        } else {
                            "Expression"
                        },
                        &source[test.range()],
                        test_type.display(db, env)
                    ));
                } else {
                    diagnostic.set_concise_message(format_args!(
                        "Object of type `{}` is always truthy",
                        test_type.display(db, env)
                    ));
                }
            };

            let function_info = match test_type {
                Type::FunctionLiteral(function) => Some(FunctionInfo::Function(
                    function.signature(db),
                    function.name(db),
                )),
                Type::BoundMethod(method) => {
                    let function = method.function(db);
                    Some(FunctionInfo::Method(
                        method.bound_signatures(db),
                        CallableDescription::defining_class(db, *test_type)
                            .map(|class| {
                                Cow::Owned(format!("{}.{}", class.name(db), function.name(db)))
                            })
                            .unwrap_or(Cow::Borrowed(&**function.name(db))),
                    ))
                }
                Type::Callable(callable) if callable.is_function_like(db) => {
                    Some(FunctionInfo::Lambda(callable.signatures(db)))
                }
                _ => None,
            };

            if let Some(function) = function_info {
                let mut diagnostic =
                    builder.into_diagnostic(format_args!("{function} is always truthy"));

                // Add a suggestion and fix that they might have meant to call (and possibly
                // also await) this function.
                //
                // It's true that calling the function might not actually fix this diagnostic
                // if the function returns something that is always truthy. They still probably
                // meant to call the function, though, so it's still a useful suggestion/fix!

                // A coroutine return type establishes that calling and awaiting the function
                // is appropriate. `Any`, `Unknown`, and `Never` do not establish this, even
                // though they are assignable to `CoroutineType`.
                // Use the top materialization so the unspecified generic arguments do not
                // prevent concrete coroutine types from being subtypes.
                let coroutine = KnownClass::CoroutineType
                    .to_instance(db, env)
                    .top_materialization(db, env);

                let is_awaitable_coro_function = self.can_await_here(test)
                    && function.signature().iter().all(|signature| {
                        !signature.return_ty.is_equivalent_to(db, env, Type::Never)
                            && signature.return_ty.is_subtype_of(db, env, coroutine)
                    });

                let kind = function.kind();

                if is_awaitable_coro_function {
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Did you mean to `await` and call this {kind}?",
                    ));
                } else {
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Did you mean to call this {kind}?"
                    ));
                }

                if matches!(test, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
                    let (call, applicability) = if function.signature().has_parameters() {
                        ("(...)", Applicability::DisplayOnly)
                    } else {
                        ("()", Applicability::Unsafe)
                    };
                    let call_edit = Edit::insertion(call.to_string(), test.end());

                    let fix = if is_awaitable_coro_function {
                        Fix::applicable_edits(
                            Edit::insertion("await ".to_string(), test.start()),
                            [call_edit],
                            applicability,
                        )
                    } else {
                        Fix::applicable_edit(call_edit, applicability)
                    };
                    diagnostic.set_fix(fix);
                }

                diagnostic
            } else if let Some(tuple_spec) = test_type.tuple_instance_spec(db, env)
                && tuple_spec.len().minimum() > 0
            {
                // This error message might not be 100% accurate for a tuple subclass
                // that overrides `__len__` or `__bool__` in a way that's inconsistent
                // with the tuple's inherited tuple spec, but you just shouldn't do that anyway.

                let length = tuple_spec.len();

                let message = match length {
                    TupleLength::Fixed(size) => {
                        format!("A {size}-element tuple is always truthy")
                    }
                    TupleLength::Variable(min, _) => format!(
                        "A tuple with >={min} element{maybe_s} is always truthy",
                        maybe_s = if min == 1 { "" } else { "s" }
                    ),
                };

                let mut diagnostic = builder.into_diagnostic(&message);

                // If the tuple has a small number of fixed elements,
                // describe the whole type of the tuple in the concise message.
                // Otherwise, avoid printing the full tuple type in the concise message here
                // (since it can be very long in some cases)
                if tuple_spec.fixed_elements().len() <= 8 {
                    add_always_truthy_concise_message(&mut diagnostic);
                } else {
                    diagnostic.set_concise_message(&message);
                }

                annotate_inferred_type(&mut diagnostic);
                self.diagnose_single_length_tuple(length, test, *test_type, &mut diagnostic);

                diagnostic
            } else if let Type::TypedDict(typed_dict) = test_type
                && let Some(field) = typed_dict
                    .items(db)
                    .iter()
                    .find_map(|(_, field)| field.is_required().then_some(field))
            {
                let num_required_keys = typed_dict
                    .items(db)
                    .iter()
                    .filter(|(_, field)| field.is_required())
                    .count();
                let maybe_s = if num_required_keys == 1 { "" } else { "s" };
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "A TypedDict with {num_required_keys} required field{maybe_s} is always truthy"
                ));
                if let Some(class) = typed_dict.defining_class() {
                    diagnostic.set_concise_message(format_args!(
                            "TypedDict `{}` with {num_required_keys} required field{maybe_s} is always truthy",
                            class.name(db)
                        ));
                } else {
                    diagnostic.set_concise_message(format_args!(
                            "A TypedDict with {num_required_keys} required field{maybe_s} is always truthy"
                        ));
                }
                annotate_inferred_type(&mut diagnostic);
                if let Some(defining_class) = typed_dict.defining_class()
                    && let Some(typed_dict_definition) = defining_class.definition(db)
                    && let Some(field_definition) = field.first_declaration()
                {
                    let typed_dict_module =
                        parsed_module(db, typed_dict_definition.python_file(db)).load(db);
                    let field_module = parsed_module(db, field_definition.python_file(db)).load(db);
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(
                            typed_dict_definition.focus_range(db, &typed_dict_module),
                        ))
                        .message(format_args!("`{}` defined here", defining_class.name(db))),
                    );
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(
                            field_definition.full_range(db, &field_module),
                        ))
                        .message(if num_required_keys == 1 {
                            "Required field declared here"
                        } else {
                            "First required field defined here"
                        }),
                    );
                }
                diagnostic
            } else if test_type.as_nominal_instance().is_some_and(|instance| {
                instance
                    .class(db, env)
                    .is_known(db, KnownClass::GeneratorType)
            }) {
                let mut diagnostic = builder.into_diagnostic("A generator is always truthy");
                add_always_truthy_concise_message(&mut diagnostic);
                annotate_inferred_type(&mut diagnostic);
                diagnostic.help("Did you mean to use `any()`?");
                if SemanticModel::new(db, self.program_file())
                    .definitely_has_builtin_binding("any", ast::AnyNodeRef::from(*test))
                {
                    // display-only edits rather than unsafe edits
                    // because we don't know what the user *really* wanted here!
                    // Collecting the result into a `tuple` is also a very plausible thing
                    // they might have wanted to do (a lot of folks think that generator expressions
                    // are actually "tuple comprehensions").
                    diagnostic.set_fix(Fix::display_only_edits(
                        Edit::insertion("any(".to_string(), test.start()),
                        [Edit::insertion(")".to_string(), test.end())],
                    ));
                }
                diagnostic
            } else if test_type.is_string_literal()
                || test_type
                    .as_union()
                    .is_some_and(|union| union.elements(db).iter().all(Type::is_string_literal))
            {
                let mut diagnostic = builder.into_diagnostic("A nonempty string is always truthy");
                let source = source_text(db, self.file());
                if source.contains_line_break(test.range()) {
                    diagnostic.set_concise_message(format_args!(
                        "Nonempty string of type `{}` is always truthy",
                        test_type.display(db, env)
                    ));
                } else if test.is_string_literal_expr() {
                    diagnostic.set_concise_message(format_args!(
                        "String literal {} is always truthy",
                        &source[test.range()]
                    ));
                } else {
                    diagnostic.set_concise_message(format_args!(
                        "{} `{}` is always truthy (has type `{}`)",
                        if matches!(test, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
                            "Nonempty string"
                        } else {
                            "Expression"
                        },
                        &source[test.range()],
                        test_type.display(db, env)
                    ));
                }
                annotate_inferred_type(&mut diagnostic);
                diagnostic
            } else if describe_as_condition
                && let Some((subexpr, subexpr_type, subexpr_length)) =
                    self.length_test_against_type_with_known_length(test)
            {
                self.report_redundant_length_comparison(
                    test,
                    subexpr,
                    subexpr_type,
                    subexpr_length,
                    builder,
                )
            } else if describe_as_condition {
                let message = "Condition is always true";
                let mut diagnostic = builder.into_diagnostic(message);
                describe_condition(&mut diagnostic);
                diagnostic
            } else {
                let mut diagnostic = builder.into_diagnostic("Condition is always truthy");
                add_always_truthy_concise_message(&mut diagnostic);
                annotate_inferred_type(&mut diagnostic);
                if test_type.try_await(db, env).is_ok() && self.can_await_here(test) {
                    diagnostic.help("Did you mean to `await` this expression?");

                    let fix = if test.precedence() <= ast::OperatorPrecedence::Await {
                        Fix::unsafe_edits(
                            Edit::insertion("await (".to_string(), test.start()),
                            [Edit::insertion(")".to_string(), test.end())],
                        )
                    } else {
                        Fix::unsafe_edit(Edit::insertion("await ".to_string(), test.start()))
                    };

                    diagnostic.set_fix(fix);
                } else if let Type::NominalInstance(instance) = test_type {
                    let class = instance.class(db, env);
                    if class.is_final(db)
                        && !class.is_known(db, KnownClass::CoroutineType)
                        && ["__bool__", "__len__"].into_iter().all(|name| {
                            test_type
                                .member_lookup_with_policy(
                                    db,
                                    env,
                                    name,
                                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                                )
                                .is_undefined()
                        })
                    {
                        let class_name = class.name(db);

                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!(
                                "`{class_name}` instances are always truthy because \
                                    `{class_name}` cannot be subclassed and does not define \
                                    `__bool__` or `__len__`",
                            ),
                        );

                        let class_literal = class.class_literal(db);
                        let header_range = class_literal.header_range(db);

                        let final_decorator_range = class_literal
                            .as_static()
                            .and_then(|static_class| {
                                static_class.find_known_decorator_span(db, KnownFunction::Final)
                            })
                            .and_then(|span| span.range());

                        let range = final_decorator_range
                            .map(|decorator_range| header_range.cover(decorator_range))
                            .unwrap_or(header_range);

                        sub.annotate(
                            Annotation::primary(
                                Span::from(class_literal.file(db)).with_range(range),
                            )
                            .message(format_args!("`{class_name}` defined here")),
                        );

                        diagnostic.sub(sub);

                        if final_decorator_range.is_none()
                            && enum_metadata(db, class_literal)
                                .is_some_and(|metadata| !metadata.members.is_empty())
                        {
                            diagnostic.info(format_args!(
                                "`{class_name}` cannot be subclassed \
                                because it is an `Enum` subclass and defines enum members"
                            ));
                        }
                    }
                }
                diagnostic
            }
        } else {
            let add_always_falsy_concise_message = |diagnostic: &mut LintDiagnosticGuard| {
                if should_quote_test_expression()
                    && let source = source_text(db, self.file())
                    && !source.contains_line_break(test.range())
                {
                    diagnostic.set_concise_message(format_args!(
                        "{} `{}` is always falsy (has type `{}`)",
                        if matches!(test, ast::Expr::Name(_)) {
                            "Variable"
                        } else {
                            "Expression"
                        },
                        &source[test.range()],
                        test_type.display(db, env)
                    ));
                } else if test_type.is_none(db) {
                    diagnostic.set_concise_message("`None` is always falsy");
                } else {
                    diagnostic.set_concise_message(format_args!(
                        "Object of type `{}` is always falsy",
                        test_type.display(db, env)
                    ));
                }
            };

            if test_type.is_none(db) {
                let mut diagnostic = builder.into_diagnostic("`None` is always falsy");
                add_always_falsy_concise_message(&mut diagnostic);
                diagnostic
            } else if let Some(tuple) = test_type.tuple_instance_spec(db, env)
                && tuple.len() == TupleLength::Fixed(0)
            {
                // This error message might not be 100% accurate for a tuple subclass
                // that overrides `__len__` or `__bool__` in a way that's inconsistent
                // with the tuple's inherited tuple spec, but you just shouldn't do that anyway.
                let mut diagnostic = builder.into_diagnostic("An empty tuple is always falsy");
                add_always_falsy_concise_message(&mut diagnostic);
                annotate_inferred_type(&mut diagnostic);
                diagnostic
            } else if test_type.is_string_literal() {
                let mut diagnostic = builder.into_diagnostic("An empty string is always falsy");
                add_always_falsy_concise_message(&mut diagnostic);
                annotate_inferred_type(&mut diagnostic);
                diagnostic
            } else if describe_as_condition
                && let Some((subexpr, subexpr_type, subexpr_length)) =
                    self.length_test_against_type_with_known_length(test)
            {
                self.report_redundant_length_comparison(
                    test,
                    subexpr,
                    subexpr_type,
                    subexpr_length,
                    builder,
                )
            } else {
                let message = if describe_as_condition {
                    "Condition is always false"
                } else {
                    "Condition is always falsy"
                };
                let mut diagnostic = builder.into_diagnostic(message);
                if describe_as_condition {
                    describe_condition(&mut diagnostic);
                } else {
                    add_always_falsy_concise_message(&mut diagnostic);
                    annotate_inferred_type(&mut diagnostic);
                }
                diagnostic
            }
        };

        Some(diagnostic)
    }

    /// Return `Some((expr, expr_ty, expr_length))`, where `expr_ty` is the type of an object
    /// being tested for length, and `expr_length` is its known length.
    ///
    /// Returns `None` if `test` is not a comparison of the form `len(x) == i` or `len(x) != i`,
    /// where `x` has a known length.
    fn length_test_against_type_with_known_length<'a>(
        &self,
        test: &'a ast::Expr,
    ) -> Option<(&'a ast::Expr, Type<'db>, i64)> {
        let db = self.db();
        let env = self.program_environment();

        if let ast::Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            ..
        }) = test
            && let [single_op] = &**ops
            && let [single_comparator] = &**comparators
            && let (ast::Expr::Call(call), other) | (other, ast::Expr::Call(call)) =
                (&**left, single_comparator)
            && matches!(single_op, ast::CmpOp::Eq | ast::CmpOp::NotEq)
            && let ast::Arguments { args, keywords, .. } = &call.arguments
            && keywords.is_empty()
            && let [single_arg] = &**args
            && let Type::FunctionLiteral(function) = self.expression_type(&call.func)
            && function.is_known(db, KnownFunction::Len)
            && self.expression_type(other).is_int_literal()
        {
            let arg_type = self.expression_type(single_arg);
            let length = arg_type.len(db, env)?.as_int_literal()?;
            Some((single_arg, arg_type, length))
        } else {
            None
        }
    }

    fn report_redundant_length_comparison<'a>(
        &self,
        test: &ast::Expr,
        test_subexpression: &ast::Expr,
        subexpression_type: Type<'db>,
        length: i64,
        builder: LintDiagnosticGuardBuilder<'a, 'a>,
    ) -> LintDiagnosticGuard<'a, 'a> {
        let db = self.db();
        let env = self.program_environment();

        let source = source_text(db, self.file());
        let mut diagnostic = if !source.contains_line_break(test.range()) {
            builder.into_diagnostic(format_args!(
                "`{}` always has length {length}",
                &source[test_subexpression.range()]
            ))
        } else {
            let mut diag =
                builder.into_diagnostic(format_args!("Value always has length {length}"));
            diag.set_concise_message(format_args!(
                "Object of type `{}` always has length {length}",
                subexpression_type.display(db, env)
            ));
            diag
        };
        diagnostic.annotate(
            self.context
                .secondary(test_subexpression)
                .message(format_args!(
                    "Has type `{}`",
                    subexpression_type.display(db, env)
                )),
        );
        if let Ok(length) = usize::try_from(length) {
            self.diagnose_single_length_tuple(
                TupleLength::Fixed(length),
                test_subexpression,
                subexpression_type,
                &mut diagnostic,
            );
        }
        diagnostic
    }

    fn diagnose_single_length_tuple(
        &self,
        length: TupleLength,
        node: &ast::Expr,
        node_type: Type<'db>,
        diagnostic: &mut Diagnostic,
    ) {
        let db = self.db();
        let env = self.program_environment();

        // The ellipsis suggestion is for `tuple[T]`, not named tuples or other
        // subclasses whose fixed length is part of their definition.
        if length == TupleLength::Fixed(1)
            && let Some(tuple_spec) = node_type.tuple_instance_spec(db, env)
            && let Tuple::Fixed(fixed_length_tuple) = &*tuple_spec
            && matches!(node, ast::Expr::Name(_) | ast::Expr::Attribute(_))
        {
            if node_type.exact_tuple_instance_spec(db).is_none() {
                if let Some(definition) = node_type.definition(db, env)
                    && let Some(definition) = definition.definition()
                {
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(definition.focus_range(db, &module)))
                            .message(format_args!(
                                "`{}` defined here",
                                node_type.display(db, env)
                            )),
                    );
                }
                return;
            }

            let definition_info =
                condition_definition_info(db, self.program_file(), node, |expr| {
                    self.expression_type(expr)
                });

            if let Some(single_definition) = definition_info.single_definition {
                let file = single_definition.python_file(db);
                let program_file = single_definition.program_file(db);
                let module = parsed_module(db, file).load(db);
                let annotation_info = match single_definition.kind(db) {
                    DefinitionKind::AnnotatedAssignment(assignment) => {
                        let annotation = assignment.annotation(&module);
                        infer_definition_types(db, single_definition)
                            .try_expression_type(annotation)
                            .map(|annotation_type| (annotation, annotation_type))
                    }
                    DefinitionKind::Parameter(parameter) => {
                        parameter.annotation(&module).and_then(|annotation| {
                            let scope = single_definition.scope(db).scope(db).parent()?;
                            let annotation_type = infer_scope_types(
                                db,
                                scope.to_scope_id(db, program_file),
                                TypeContext::default(),
                            )
                            .try_expression_type(annotation)?;
                            Some((annotation, annotation_type))
                        })
                    }
                    _ => None,
                };
                if let Some((annotation, annotation_type)) = annotation_info
                    && annotation_type == node_type
                {
                    let file = single_definition.file(db);
                    let diagnostic_annotation =
                        || Annotation::secondary(Span::from(file).with_range(annotation.range()));
                    diagnostic.annotate(
                        diagnostic_annotation()
                            .message("Inferred as a 1-element tuple due to this annotation"),
                    );

                    let sole_element = fixed_length_tuple.elements_slice()[0];
                    let suggested_type = Type::homogeneous_tuple(db, env, sole_element)
                        .display(db, env)
                        .to_string_parts();

                    if suggested_type.is_valid_syntax {
                        let resolver_file = single_definition.program_file(db).resolver_file(db);
                        let annotated_in_first_party_code = file == self.file()
                            || file_to_module(db, resolver_file)
                                .and_then(|module| module.search_path(db))
                                .is_some_and(SearchPath::is_first_party);

                        let maybe_star = if annotation.is_starred_expr() {
                            "*"
                        } else {
                            ""
                        };

                        let annotation = if annotated_in_first_party_code {
                            diagnostic_annotation().message(format_args!(
                                "Did you mean `{maybe_star}{}`?",
                                suggested_type.label
                            ))
                        } else {
                            diagnostic_annotation().message(format_args!(
                                "The author of this code might have meant `{maybe_star}{}`?",
                                suggested_type.label
                            ))
                        };

                        diagnostic.annotate(annotation);
                    }
                }
            }
        }
    }

    /// Returns `true` if adding `await` at `expression` would produce valid Python.
    ///
    /// Accounts for asynchronous functions, notebook cells, annotation restrictions, enclosing
    /// scopes, and the different scoping behavior of comprehensions and generator expressions.
    fn can_await_here(&self, expression: &ast::Expr) -> bool {
        let Some(expression_scope) = self.index.try_expression_scope_id(expression) else {
            return false;
        };
        let annotation_parent_scope = self
            .index
            .annotation_parent_scope_id(self.module(), expression);

        let db = self.db();

        let mut in_eager_comprehension = false;

        for (scope_id, scope) in self.index.ancestor_scopes(expression_scope) {
            // The first iterable of a comprehension stays in the annotation's enclosing scope.
            // Eager comprehensions also inherit the restriction, but a generator body can allow
            // `await` before we reach the scope enclosing its annotation.
            // Conservatively reject annotations on every Python version, even though some allow
            // `await` before Python 3.14 without `from __future__ import annotations`. Avoiding
            // invalid syntax matters more than offering every possible fix in this rare context.
            if Some(scope_id) == annotation_parent_scope {
                return false;
            }

            // Before Python 3.11, awaiting in a nested list, set, or dict comprehension cannot
            // implicitly make its containing comprehension or generator expression asynchronous.
            if in_eager_comprehension
                && scope.kind() == ScopeKind::Comprehension
                && self.program_environment().python_version(db) < PythonVersion::PY311
                && !scope_id.is_async_comprehension(self.index)
            {
                return false;
            }

            match scope.node() {
                NodeWithScopeKind::Function(function) => {
                    return function.node(self.module()).is_async;
                }
                NodeWithScopeKind::Lambda(_)
                | NodeWithScopeKind::Class(_)
                | NodeWithScopeKind::ClassTypeParameters(_)
                | NodeWithScopeKind::FunctionTypeParameters(_)
                | NodeWithScopeKind::TypeAliasTypeParameters(_)
                | NodeWithScopeKind::TypeAlias(_) => {
                    return false;
                }
                NodeWithScopeKind::GeneratorExpression(_) => {
                    return true;
                }
                NodeWithScopeKind::Module => {
                    return source_text(db, self.file()).is_notebook();
                }
                NodeWithScopeKind::DictComprehension(_)
                | NodeWithScopeKind::ListComprehension(_)
                | NodeWithScopeKind::SetComprehension(_) => {
                    in_eager_comprehension = true;
                }
            }
        }

        false
    }

    pub(super) fn annotate_redundant_if_or_elif(
        &self,
        condition: &RedundantCondition<'_, 'db>,
        diagnostic: &mut Diagnostic,
        if_stmt: &ast::StmtIf,
    ) {
        let RedundantCondition {
            expression: test,
            value_type: _,
            is_truthy,
            kind,
        } = condition;

        if *is_truthy
            && *kind == ConditionKind::Boolean
            && let Some(clause) = if_stmt.elif_else_clauses.last()
            && clause.test.as_ref() == Some(test)
            && !diagnostic.has_applicable_fix(Applicability::DisplayOnly)
            && let Some(fix) = self.add_assert_never_else(clause, test)
        {
            diagnostic.help("Add an `else` branch that calls `assert_never`");
            diagnostic.set_fix(fix);
        }
    }

    /// Add an explicit exhaustiveness check after a redundant final `elif`.
    ///
    /// Only read a plain variable whose type is a union before the chain, and which narrows
    /// to `Never` when the condition is false. Repeating attribute access or a function call
    /// could have side effects. Returns `None`
    /// when no such variable or unshadowed runtime import is available.
    /// The fix is unsafe because the new branch raises if the static assumptions fail at runtime.
    fn add_assert_never_else(&self, clause: &ast::ElifElseClause, test: &ast::Expr) -> Option<Fix> {
        let db = self.db();
        let first_statement = clause.body.first()?;
        let source = source_text(db, self.file());
        let indentation = indentation_at_offset(clause.start(), &source)?;
        let argument = self.assert_never_argument(test)?;

        let module = typing_module_for_fix(&self.context, "assert_never", PythonVersion::PY311)?;
        let importer = self.context.importer();

        let action = importer.import_for_diagnostic(
            ImportRequest::import_from(module.as_str(), "assert_never"),
            self.scope().file_scope_id(db),
            clause.start(),
        )?;

        let body_indentation = indentation_at_offset(first_statement.start(), &source)
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(format!("{indentation}{}", importer.indentation())));

        let line_ending = find_newline(&source)
            .map(|(_, ending)| ending)
            .unwrap_or_default()
            .as_str();

        let mut end = logical_line_end(&source, self.module().tokens(), clause.end());

        // Keep trailing body comments with the `elif`, including those after a nested statement.
        for line in UniversalNewlineIterator::with_offset(&source[usize::from(end)..], end) {
            if line.trim().is_empty() {
                continue;
            }
            if line.starts_with(body_indentation.as_ref()) && line.trim_start().starts_with('#') {
                end = line.full_end();
            } else {
                break;
            }
        }

        let leading_newline = if source.line_start(end) == end {
            ""
        } else {
            line_ending
        };

        Some(Fix::unsafe_edits(
            Edit::insertion(
                format!(
                    "{leading_newline}{indentation}else:{line_ending}{body_indentation}{}({}){line_ending}",
                    action.symbol_text(),
                    argument.id,
                ),
                end,
            ),
            action.import().cloned(),
        ))
    }

    /// Find a variable tested directly, by a comparison, or by a narrowing function.
    /// More complex conditions cannot provide an argument without repeating their evaluation.
    fn assert_never_argument<'a>(&self, test: &'a ast::Expr) -> Option<&'a ast::ExprName> {
        if any_over_expr(test, ast::Expr::is_named_expr) {
            return None;
        }

        let mut operand = test;

        while let ast::Expr::UnaryOp(unary) = operand
            && unary.op == ast::UnaryOp::Not
        {
            operand = &unary.operand;
        }

        let candidates = match operand {
            ast::Expr::Name(_) => [Some(operand), None],
            ast::Expr::Compare(compare) if compare.ops.len() == 1 => {
                [Some(compare.left.as_ref()), compare.comparators.first()]
            }
            ast::Expr::Call(call) => [call.arguments.args.first(), None],
            _ => return None,
        };

        let db = self.db();
        let env = self.program_environment();
        let places = self.index.place_table(self.scope().file_scope_id(db));

        let predicate = Predicate {
            node: PredicateNode::Expression(self.index.expression(test)),
            is_positive: false,
        };

        candidates.into_iter().flatten().find_map(|candidate| {
            let name = candidate.as_name_expr()?;
            let ty = self.expression_type(candidate);
            if ty.is_never() || !self.type_before_if_chain(name)?.is_union() {
                return None;
            }
            let place = places.symbol_id(&name.id)?;
            let (constraint, _) = infer_narrowing_constraints(db, predicate, place.into());
            NarrowingConstraint::intersection(ty)
                .merge_constraint_and(constraint?)
                .evaluate_constraint_type(db, env)
                .is_never()
                .then_some(name)
        })
    }

    /// Resolve a name using the bindings and constraints that precede its `if` chain.
    /// This preserves earlier narrowing, including constraints on captured variables.
    fn type_before_if_chain(&self, name: &ast::ExprName) -> Option<Type<'db>> {
        let db = self.db();
        let env = self.program_environment();
        let use_def = self.index.use_def_map(self.scope().file_scope_id(db));
        let snapshot =
            use_def.if_chain_start_for_use(name.scoped_use_id(db, self.program_file()))?;
        let mut resolution = resolve_place_load(
            db,
            self.index,
            self.scope(),
            PlaceExpr::from_expr_name(name),
            PlaceLoadMode::AtNameSnapshot(snapshot),
        );
        let mut place = PlaceAndQualifiers::from(Place::Undefined);
        while let Some(PlaceLoadResolutionStep::Source(source)) = resolution.next() {
            let constraints = resolution.narrowing_constraints_for(&source);
            place = place.or_fall_back_to(db, env, || {
                self.infer_place_load_source(resolution.place_expr(), source, constraints)
            });
            if place.place.is_definitely_bound() {
                break;
            }
        }
        place.place.ignore_possibly_undefined()
    }
}

/// Returns the end of the logical line at `offset`, including its newline.
/// A trailing backslash can extend the line beyond its last AST node's physical line.
fn logical_line_end(source: &str, tokens: &Tokens, offset: TextSize) -> TextSize {
    tokens
        .after(offset)
        .iter()
        .find(|token| token.kind() == TokenKind::Newline)
        .map_or_else(|| source.full_line_end(offset), Ranged::end)
}
