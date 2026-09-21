//! Diagnostic messages and fixes for conditions selected by the redundant-condition checker.

use std::borrow::Cow;

use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity},
    files::{FilePath, FileRange},
    parsed::{ParsedModuleRef, parsed_module, parsed_string_annotation},
    source::{line_index, source_text},
};
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::{
    self as ast, PythonVersion,
    helpers::any_over_expr,
    token::{TokenKind, Tokens, parenthesized_range},
};
use ruff_python_trivia::indentation_at_offset;
use ruff_source_file::{LineRanges, UniversalNewlineIterator, find_newline};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_module_resolver::{SearchPath, file_to_module};
use ty_python_core::{
    Truthiness,
    ast_ids::HasScopedUseId,
    definition::{Definition, DefinitionKind},
    place::PlaceExpr,
    predicate::{Predicate, PredicateNode},
    scope::FileScopeId,
    semantic_index,
};

use crate::{
    SemanticModel,
    importer::ImportRequest,
    place::{Place, PlaceAndQualifiers},
    place_load::{PlaceLoadMode, PlaceLoadResolutionStep, resolve_place_load},
    reachability::is_range_reachable,
    types::{
        KnownClass, LintDiagnosticGuard, LintDiagnosticGuardBuilder, MemberLookupPolicy, Type,
        TypeContext,
        call::bind::CallableDescription,
        context::InferContext,
        diagnostic::typing_module_for_fix,
        enum_metadata,
        function::KnownFunction,
        infer::{
            TypeInferenceBuilder,
            builder::redundant_conditions::{
                SuiteExitKind, is_trivial_statement, suite_ends_with_exit,
            },
        },
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
            Method(&'db CallableSignature<'db>, Option<Cow<'db, str>>),
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
                    FunctionInfo::Method(_, Some(name)) => write!(f, "Method `{name}`"),
                    FunctionInfo::Method(_, None) => write!(f, "Method"),
                    FunctionInfo::Lambda(_) => write!(f, "Function object"),
                }
            }
        }

        let RedundantCondition {
            expression: test,
            value_type: test_type,
            truthiness,
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
            if test_type.is_bool_literal() || test_type.bool(db, env).is_ambiguous() {
                diagnostic.set_primary_annotation_message(format_args!(
                    "Inferred type is `{}`",
                    test_type.display(db, env)
                ));
            } else {
                let is_truthy = if truthiness.is_always_true() {
                    "truthy"
                } else {
                    "falsy"
                };
                diagnostic.set_primary_annotation_message(format_args!(
                    "Inferred type `{}` is always {is_truthy}",
                    test_type.display(db, env)
                ));
            }
        };

        let describe_condition = |diagnostic: &mut LintDiagnosticGuard| {
            let source = source_text(db, self.file());
            let is_truthy = if truthiness.is_always_true() {
                "true"
            } else {
                "false"
            };
            if source.contains_line_break(test.range()) {
                diagnostic.set_concise_message(format_args!("Condition is always {is_truthy}"));
            } else {
                diagnostic.set_concise_message(format_args!(
                    "Condition `{}` is always {is_truthy}",
                    &source[test.range()]
                ));
            }

            if let ast::Expr::Compare(compare) = test
                && let Some((left, _, single_comparator)) = compare.as_single()
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
        let describe_as_condition = !truthiness.is_ambiguous()
            && (test_type.is_subtype_of(db, env, KnownClass::Bool.to_instance(db, env))
                || test_type.bool(db, env) != *truthiness);

        let builder = self.context.report_lint(rule, test)?;

        let diagnostic = if let ConditionKind::Callable(callables) = kind {
            let mut diagnostic = builder.into_diagnostic("Suspicious boolean test of a `Callable`");
            let source = source_text(db, self.file());

            if source.contains_line_break(test.range()) {
                diagnostic.set_concise_message(format_args!(
                    "Object of type `{}` might always be truthy (did you mean to call it?)",
                    test_type.display(db, env)
                ));
            } else if matches!(test, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
                diagnostic.set_concise_message(format_args!(
                    "Callable `{}` might always be truthy \
                        (has type `{}` -- did you mean to call it?)",
                    &source[test.range()],
                    test_type.display(db, env)
                ));
            } else {
                diagnostic.set_concise_message(format_args!(
                    "Expression `{}` of type `{}` might always be truthy \
                        (did you mean to call it?)",
                    &source[test.range()],
                    test_type.display(db, env)
                ));
            }
            diagnostic.set_primary_annotation_message(format_args!(
                "`{}` object tested for truthiness",
                test_type.display(db, env)
            ));
            diagnostic
                .info("Callable objects are usually functions, and functions are always truthy");

            let should_await = self.should_await_call(
                test,
                callables.iter().flat_map(|callable| {
                    callable
                        .signatures(db)
                        .iter()
                        .map(|signature| signature.return_ty)
                }),
            );

            if should_await {
                diagnostic.help("Did you mean to call and await this callable?");
            } else {
                diagnostic.help("Did you mean to call this callable?");
            }

            let uncalled_function = UncalledFunction {
                has_parameters: callables
                    .iter()
                    .any(|callable| callable.signatures(db).has_parameters()),
                should_await,
            };

            uncalled_function.suggest_call(&self.context, &mut diagnostic, test);

            diagnostic
        } else if kind == &ConditionKind::Iterable {
            let mut diagnostic =
                builder.into_diagnostic("Suspicious boolean test of an `Iterable`");
            let source = source_text(db, self.file());

            if source.contains_line_break(test.range()) {
                diagnostic.set_concise_message(format_args!(
                    "Object might be truthy even if its length is 0 (has type `{}`)",
                    test_type.display(db, env)
                ));
            } else {
                let kind = if test.is_name_expr() {
                    "Variable"
                } else {
                    "Expression"
                };
                diagnostic.set_concise_message(format_args!(
                    "{kind} `{}` might be truthy even if its length is 0 (has type `{}`)",
                    &source[test.range()],
                    test_type.display(db, env)
                ));
            }
            diagnostic.set_primary_annotation_message(format_args!(
                "`{}` object tested for truthiness",
                test_type.display(db, env)
            ));

            let definition_info =
                condition_definition_info(db, self.program_file(), test, |expr| {
                    self.expression_type(expr)
                });

            let mut first_party_annotation = None;

            if let Some(single_definition) = definition_info.single_definition {
                let file = single_definition.python_file(db);
                let module = parsed_module(db, file).load(db);

                if let Some(annotation) =
                    self.matching_type_annotation(single_definition, &module, *test_type)
                {
                    let file = single_definition.file(db);

                    diagnostic.annotate(
                        Annotation::secondary(Span::from(file).with_range(annotation.range()))
                            .message(format_args!(
                                "Inferred as `{}` due to this annotation",
                                test_type.display(db, env)
                            )),
                    );

                    if test_type
                        .known_specialization(db, env, KnownClass::Iterable)
                        .is_some()
                        && self.is_first_party_definition(single_definition)
                    {
                        first_party_annotation = Some(IterableAnnotation {
                            range: FileRange::new(file, annotation.range()),
                            scope: semantic_index(db, single_definition.program_file(db))
                                .expression_scope_id(annotation),
                            iterable_ranges: self
                                .iterable_annotation_ranges(single_definition, annotation),
                        });
                    }
                }
            }

            diagnostic.info(format_args!(
                "`{}` objects can be generators, \
                and generators are truthy even when empty",
                test_type.display(db, env)
            ));

            if let Some(IterableAnnotation {
                range,
                scope,
                iterable_ranges,
            }) = first_party_annotation
            {
                let line = line_index(db, range.file()).line_index(range.start());
                let collection_module = if env.python_version(db) >= PythonVersion::PY39 {
                    "collections.abc"
                } else {
                    "typing"
                };

                let annotation_source = source_text(db, range.file());
                let suggestion = iterable_ranges.map(|ranges| &annotation_source[ranges.element]);

                if range.file() == self.file() {
                    if let Some(suggestion) = suggestion {
                        diagnostic.help(format_args!(
                            "Use `{collection_module}.Collection[{suggestion}]` as the annotation \
                            on line {line}"
                        ));
                        if let Some(ranges) = iterable_ranges
                            && let Some(action) = self.context.importer().import_for_diagnostic(
                                ImportRequest::import_from(collection_module, "Collection"),
                                scope,
                                range.start(),
                            )
                        {
                            diagnostic.set_fix(Fix::unsafe_edits(
                                Edit::range_replacement(
                                    action.symbol_text().to_string(),
                                    ranges.origin,
                                ),
                                action.import().cloned(),
                            ));
                        }
                    } else {
                        diagnostic.help(format_args!(
                            "Consider reworking the annotation on line {line} \
                            to use `{collection_module}.Collection`",
                        ));
                    }
                } else {
                    let path = range.file().path(db);
                    let rendered_path = match path {
                        FilePath::System(path) => path
                            .strip_prefix(db.system().current_directory())
                            .unwrap_or(path)
                            .as_str(),
                        FilePath::Vendored(_) | FilePath::SystemVirtual(_) => path.as_str(),
                    };
                    if let Some(suggestion) = suggestion {
                        diagnostic.help(format_args!(
                            "Consider using `{collection_module}.Collection[{suggestion}]` \
                            in the annotation on line {line} of {rendered_path}"
                        ));
                    } else {
                        diagnostic.help(format_args!(
                            "Consider reworking the annotation on line {line} of {rendered_path} \
                            to use `{collection_module}.Collection`",
                        ));
                    }
                }
                diagnostic.info(
                    "A `Collection` must define `__len__`, \
                    so `Collection` excludes generators",
                );
                diagnostic.help(
                    "Alternatively, test the length of the iterable \
                    instead of its truthiness",
                );
            } else {
                diagnostic.help("Test the length of the iterable instead of its truthiness");
            }

            if diagnostic.fix().is_none() {
                let semantic_model = SemanticModel::new(db, self.program_file());
                let test_ref = ast::AnyNodeRef::from(*test);
                if semantic_model.definitely_has_builtin_binding("len", test_ref)
                    && semantic_model.definitely_has_builtin_binding("tuple", test_ref)
                {
                    diagnostic.set_fix(Fix::display_only_edits(
                        Edit::insertion("len(tuple(".to_string(), test.start()),
                        [Edit::insertion("))".to_string(), test.end())],
                    ));
                }
            }

            diagnostic
        } else if truthiness.is_always_true() {
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
                Type::BoundMethod(method) if let Some(signatures) = method.bound_signatures(db) => {
                    Some(FunctionInfo::Method(
                        signatures,
                        method.function(db).map(|function| {
                            CallableDescription::defining_class(db, *test_type)
                                .map(|class| {
                                    Cow::Owned(format!("{}.{}", class.name(db), function.name(db)))
                                })
                                .unwrap_or(Cow::Borrowed(&**function.name(db)))
                        }),
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

                let should_await = self.should_await_call(
                    test,
                    function
                        .signature()
                        .iter()
                        .map(|signature| signature.return_ty),
                );

                let kind = function.kind();

                if should_await {
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Did you mean to `await` and call this {kind}?",
                    ));
                } else {
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Did you mean to call this {kind}?"
                    ));
                }

                let uncalled_function = UncalledFunction {
                    has_parameters: function.signature().has_parameters(),
                    should_await,
                };

                uncalled_function.suggest_call(&self.context, &mut diagnostic, test);

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
                if test_type.try_await(db, env).is_ok()
                    && let Some(fix) = self.await_expression_fix(test)
                {
                    diagnostic.help("Did you mean to `await` this expression?");
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

        if let ast::Expr::Compare(compare) = test
            && let Some((left, single_op, single_comparator)) = compare.as_single()
            && let (ast::Expr::Call(call), other) | (other, ast::Expr::Call(call)) =
                (left, single_comparator)
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

    /// Find a definition's annotation if it matches the type inferred at the use site.
    fn matching_type_annotation<'ast>(
        &self,
        definition: Definition<'db>,
        module: &'ast ParsedModuleRef,
        inferred_type: Type<'db>,
    ) -> Option<&'ast ast::Expr> {
        let db = self.db();
        let annotation = match definition.kind(db) {
            DefinitionKind::AnnotatedAssignment(assignment) => assignment.annotation(module),
            DefinitionKind::Parameter(parameter) => parameter.annotation(module)?,
            _ => return None,
        };
        (self.annotation_expression_type(definition, annotation)? == inferred_type)
            .then_some(annotation)
    }

    fn annotation_expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Option<Type<'db>> {
        let db = self.db();
        match definition.kind(db) {
            DefinitionKind::AnnotatedAssignment(_) => {
                infer_definition_types(db, definition).try_expression_type(expression)
            }
            DefinitionKind::Parameter(_) => {
                let scope = definition.scope(db).scope(db).parent()?;
                let scope_id = scope.to_scope_id(db, definition.program_file(db));
                infer_scope_types(db, scope_id, TypeContext::default())
                    .try_expression_type(expression)
            }
            _ => None,
        }
    }

    /// Locate the iterable name and its element annotation so a fix can change only the name.
    /// Keeping the rest of the source preserves aliases, forward references, and version-specific
    /// syntax. Parsed string annotations retain source offsets, including nested quotes.
    fn iterable_annotation_ranges(
        &self,
        definition: Definition<'db>,
        annotation: &ast::Expr,
    ) -> Option<IterableAnnotationRanges> {
        let db = self.db();
        match annotation {
            ast::Expr::StringLiteral(string) => {
                let source = source_text(db, definition.file(db));
                let parsed =
                    parsed_string_annotation(&source, string.as_single_part_string()?).ok()?;
                self.iterable_annotation_ranges(definition, parsed.expr())
            }
            ast::Expr::Subscript(subscript) => {
                let known_class = self
                    .annotation_expression_type(definition, &subscript.value)?
                    .as_class_literal()?
                    .known(db)?;

                (known_class == KnownClass::Iterable).then_some(IterableAnnotationRanges {
                    origin: subscript.value.range(),
                    element: subscript.slice.range(),
                })
            }
            _ => None,
        }
    }

    fn is_first_party_definition(&self, definition: Definition<'db>) -> bool {
        let db = self.db();
        definition.file(db) == self.file()
            || file_to_module(db, definition.program_file(db).resolver_file(db))
                .and_then(|module| module.search_path(db))
                .is_some_and(SearchPath::is_first_party)
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
                let module = parsed_module(db, file).load(db);
                if let Some(annotation) =
                    self.matching_type_annotation(single_definition, &module, node_type)
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
                        let maybe_star = if annotation.is_starred_expr() {
                            "*"
                        } else {
                            ""
                        };

                        let annotation = if self.is_first_party_definition(single_definition) {
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

    /// Whether every callable return type is known to be awaitable in this context.
    fn should_await_call(
        &self,
        expression: &ast::Expr,
        return_types: impl IntoIterator<Item = Type<'db>>,
    ) -> bool {
        if !self.can_await_here(expression) {
            return false;
        }

        let db = self.db();
        let env = self.program_environment();

        // Use the top materialization so concrete return types can be subtypes regardless of
        // the awaitable's generic arguments. `Any`, `Unknown`, and `Never` do not establish
        // that a call returns an awaitable.
        let awaitable = KnownClass::Awaitable
            .to_instance(db, env)
            .top_materialization(db, env);

        return_types.into_iter().all(|return_ty| {
            !return_ty.is_equivalent_to(db, env, Type::Never)
                && return_ty.is_subtype_of(db, env, awaitable)
        })
    }

    /// Return a [`TextRange`] spanning from `branch_start` up to and including
    /// the offset of the first newline character after the start of `first_statement`.
    ///
    /// For example, given this code:
    ///
    /// ```py
    /// if foo:                       # line 1
    ///     pass                      # line 2
    /// elif bar:                     # line 3
    ///     for i in range(10):       # line 4
    ///         for j in range(5):    # line 5
    ///             pass              # line 6
    /// ```
    ///
    /// if this method is passed `branch_start` pointing to the start of the `elif`
    /// node on line 3, and `first_statement` pointing to the start of the `for` loop
    /// on line 4, this method would return a [`TextRange`] spanning from the start of
    /// line 3 up to the end of line 4.
    fn branch_range_until_first_newline(
        &self,
        branch_start: TextSize,
        first_statement: &ast::Stmt,
    ) -> TextRange {
        TextRange::new(
            branch_start,
            source_text(self.db(), self.file()).line_end(first_statement.start()),
        )
    }

    fn is_unreachable(&self, stmt: &ast::Stmt) -> bool {
        !is_range_reachable(
            self.db(),
            self.index,
            self.scope().file_scope_id(self.db()),
            stmt.range(),
        )
    }

    pub(super) fn add_secondary_annotations_for_redundant_while(
        &self,
        diagnostic: &mut Diagnostic,
        full_condition_truthiness: Truthiness,
        while_statement: &ast::StmtWhile,
        following_suite: &[ast::Stmt],
    ) {
        if full_condition_truthiness.is_always_true() {
            let mut else_branch_is_unreachable = false;

            if let Some(stmt) = first_nontrivial_statement(&while_statement.orelse)
                && self.is_unreachable(stmt)
            {
                // E.g. for
                //
                // ```py
                // def example(nonempty: tuple[int, int]):
                //     while nonempty:
                //         break
                //     else:
                //         print("unreachable")
                // ```
                //
                // Since `nonempty` is always truthy, the loop can *only* ever terminate due to
                // control flow encountering a `break` statement in the loop body. This means
                // that the `else` suite is unreachable, since `else` suites for `while` and `for`
                // statements are *only* executed if the control flow never hit a `break`.
                diagnostic.annotate(
                    self.context
                        .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                        .message("This statement is unreachable"),
                );
                else_branch_is_unreachable = true;
            }

            if !suite_ends_with_exit(self, following_suite, SuiteExitKind::Defensive)
                && let Some(stmt) = first_nontrivial_statement(following_suite)
                && self.is_unreachable(stmt)
            {
                // E.g. in both cases here, the statement after the `while` loop is unreachable:
                // the loop condition is always truthy and the loop contains no `break`, so it
                // can never terminate.
                //
                // ```py
                // def example(nonempty: tuple[int, int]):
                //     while nonempty:
                //         print("loop body")
                //
                //     print("unreachable")
                //
                // def example2(nonempty: tuple[int, int]):
                //     while nonempty:
                //         print("loop body")
                //     else:
                //         print("unreachable else")
                //
                //     print("unreachable following statement")
                // ```
                diagnostic.annotate(
                    self.context
                        .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                        .message(if else_branch_is_unreachable {
                            "This following statement is also unreachable"
                        } else {
                            "This following statement is unreachable"
                        }),
                );
            }
        } else if full_condition_truthiness.is_always_false()
            && let Some(stmt) = first_nontrivial_statement(&while_statement.body)
            && self.is_unreachable(stmt)
        {
            // The body of the `while` loop here is unreachable due to the condition being always falsy:
            //
            // ```py
            // def example(empty: tuple[()]):
            //     while empty:
            //         print("unreachable")
            // ```
            diagnostic.annotate(
                self.context
                    .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                    .message("This statement is unreachable"),
            );
        }
    }

    pub(super) fn add_secondary_annotations_for_redundant_assert(
        &self,
        diagnostic: &mut Diagnostic,
        full_condition_truthiness: Truthiness,
        following_suite: &[ast::Stmt],
    ) {
        if full_condition_truthiness.is_always_false()
            && let Some(stmt) = first_nontrivial_statement(following_suite)
            && self.is_unreachable(stmt)
        {
            diagnostic.annotate(
                self.context
                    .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                    .message("This following statement is unreachable"),
            );
        }
    }

    pub(super) fn add_secondary_annotations_for_redundant_match(
        &self,
        diagnostic: &mut Diagnostic,
        full_condition_truthiness: Truthiness,
        case: &ast::MatchCase,
        following_cases: &[ast::MatchCase],
    ) {
        if full_condition_truthiness.is_always_true()
            && case.pattern.is_irrefutable()
            && let Some((next_case, stmt)) = following_cases
                .iter()
                .find_map(|case| first_nontrivial_statement(&case.body).map(|stmt| (case, stmt)))
            && self.is_unreachable(stmt)
        {
            // The second `case` branch here is unreachable because the first `case`
            // has an irrefutable pattern with an always-truthy guard:
            //
            // ```py
            // def example(value: object, nonempty: tuple[int, int]):
            //     match value:
            //         case _ if nonempty:
            //             print("selected")
            //         case str():
            //             print("unreachable")
            // ```
            diagnostic.annotate(
                self.context
                    .secondary(self.branch_range_until_first_newline(next_case.start(), stmt))
                    .message("This following branch is unreachable"),
            );
        } else if full_condition_truthiness.is_always_false()
            && let Some(stmt) = first_nontrivial_statement(&case.body)
            && self.is_unreachable(stmt)
        {
            // The `case` body here is unreachable due to the always-falsy guard:
            //
            // ```py
            // def example(value: object, empty: tuple[()]):
            //     match value:
            //         case str() if empty:
            //             print("unreachable")
            // ```
            diagnostic.annotate(
                self.context
                    .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                    .message("This statement is unreachable"),
            );
        }
    }

    pub(super) fn add_secondary_annotations_for_redundant_if_or_elif(
        &self,
        condition: &RedundantCondition<'_, 'db>,
        diagnostic: &mut Diagnostic,
        full_condition_truthiness: Truthiness,
        if_stmt: &ast::StmtIf,
        branch_index: usize,
        following_suite: &[ast::Stmt],
    ) {
        if full_condition_truthiness.is_ambiguous() {
            return;
        }

        let RedundantCondition {
            expression: test,
            value_type: _,
            truthiness: _,
            kind,
        } = condition;

        let if_elif_else_suites: Vec<&[ast::Stmt]> = std::iter::once(&*if_stmt.body)
            .chain(if_stmt.elif_else_clauses.iter().map(|clause| &*clause.body))
            .collect();

        if full_condition_truthiness.is_always_true() {
            let mut implicit_else_is_unreachable = false;

            // The branch index includes the initial `if`, but `elif_else_clauses` does not.
            if let Some((next_branch, stmt)) = if_stmt.elif_else_clauses[branch_index..]
                .iter()
                .find_map(|clause| {
                    first_nontrivial_statement(&clause.body).map(|stmt| (clause, stmt))
                })
                && self.is_unreachable(stmt)
            {
                // The `elif` branch here is unreachable because the preceding `if` condition is always true:
                //
                // ```py
                // def example(nonempty: tuple[int, int], flag: bool):
                //     if nonempty:
                //         print("selected")
                //     elif flag:
                //         print("unreachable")
                // ```
                diagnostic.annotate(
                    self.context
                        .secondary(self.branch_range_until_first_newline(next_branch.start(), stmt))
                        .message("This following branch is unreachable"),
                );
            } else if branch_index == if_stmt.elif_else_clauses.len()
                && if_elif_else_suites
                    .iter()
                    .all(|suite| suite_ends_with_exit(self, suite, SuiteExitKind::Any))
                && !suite_ends_with_exit(self, following_suite, SuiteExitKind::Defensive)
                && let Some(stmt) = first_nontrivial_statement(following_suite)
                && self.is_unreachable(stmt)
            {
                // The suite following the `if`/`elif`/`else` chain here is unreachable because the final
                // condition in the chain is always true, and every branch exits.
                // This leaves no path to the following suite:
                //
                // ```py
                // def example(value: int | str):
                //     if isinstance(value, int):
                //         return
                //     elif isinstance(value, str):
                //         return
                //
                //     print("unreachable")
                // ```
                implicit_else_is_unreachable = true;
                diagnostic.annotate(
                    self.context
                        .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                        .message("This following statement is unreachable"),
                );
            }

            if !implicit_else_is_unreachable
                && kind.is_boolean()
                && let Some(clause) = if_stmt.elif_else_clauses.last()
                && clause.test.as_ref() == Some(test)
                && !diagnostic.has_applicable_fix(Applicability::DisplayOnly)
            {
                if let Some(fix) = self.add_assert_never_else(clause, test) {
                    diagnostic.help("Add an `else` branch that calls `assert_never`");
                    diagnostic.set_fix(fix);
                } else {
                    diagnostic.help(
                        "Replace this `elif` with an `else` branch \
                        that asserts the condition to be `True`",
                    );
                    if let Some(fix) = self.replace_redundant_elif_with_assertion(clause, test) {
                        diagnostic.set_fix(fix);
                    }
                }
            }
        } else if let Some(stmt) = first_nontrivial_statement(if_elif_else_suites[branch_index])
            && self.is_unreachable(stmt)
        {
            // The `if` body here is unreachable because the condition is always false:
            //
            // ```py
            // def example(empty: tuple[()]):
            //     if empty:
            //         print("unreachable")
            // ```
            diagnostic.annotate(
                self.context
                    .secondary(self.branch_range_until_first_newline(stmt.start(), stmt))
                    .message("This statement is unreachable"),
            );
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
            ast::Expr::Compare(compare) => {
                let (left, _, right) = compare.as_single()?;
                [Some(left), Some(right)]
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

    /// Replaces an always-true final `elif` with an `else` branch and a defensive assertion.
    ///
    /// Preserves the original condition, comments, branch indentation, and file-wide line-ending
    /// style. Bare assignment expressions are parenthesized so they remain valid assertion tests.
    /// Returns `None` when the branch has no body, its first statement cannot accommodate a new
    /// indented assertion, or rewriting the header would discard a comment.
    ///
    /// The fix is unsafe because an incorrect static assumption can cause the new assertion to
    /// fail at runtime, and optimized Python execution may remove the assertion entirely.
    fn replace_redundant_elif_with_assertion(
        &self,
        clause: &ast::ElifElseClause,
        test: &ast::Expr,
    ) -> Option<Fix> {
        let first_statement = clause.body.first()?;
        let source = source_text(self.db(), self.file());
        let tokens = self.module().tokens();
        let first_statement_line_start = source.line_start(first_statement.start());

        if first_statement_line_start < logical_line_end(&source, tokens, clause.start()) {
            return None;
        }

        // An indent token can span backslash continuations. In that case, the first statement's
        // physical indentation may differ from the indentation that determines the body's scope.
        if let Some(token) = tokens.before(first_statement.start()).last()
            && token.kind() == TokenKind::Indent
            && token.start() < first_statement_line_start
        {
            return None;
        }

        let indentation = indentation_at_offset(first_statement.start(), &source)?;
        let parenthesized_test_range = parenthesized_range(test.into(), clause.into(), tokens);
        let test_range = parenthesized_test_range.unwrap_or(test.range());
        let header_prefix_range = TextRange::new(clause.start(), test_range.start());

        // Ruff caches `CommentRanges` in its indexer, but ty does not. Constructing
        // `CommentRanges` here would scan and index every comment in the file just to check
        // this small range, so inspect the existing tokens directly instead.
        if tokens
            .in_range(header_prefix_range)
            .iter()
            .any(|token| token.kind().is_comment())
        {
            return None;
        }

        let condition = &source[test_range];
        let assertion_condition = if test.is_named_expr() && parenthesized_test_range.is_none() {
            format!("({condition})")
        } else {
            condition.to_string()
        };
        let line_ending = find_newline(&source)
            .map(|(_, ending)| ending)
            .unwrap_or_default()
            .as_str();

        Some(Fix::unsafe_edits(
            Edit::range_replacement(
                "else".to_string(),
                TextRange::new(clause.start(), test_range.end()),
            ),
            [Edit::insertion(
                format!("assert {assertion_condition}{line_ending}{indentation}"),
                first_statement.start(),
            )],
        ))
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

/// Return the first "nontrivial" statement in `suite`, if any.
///
/// See [`is_trivial_statement`] for the definition of a trivial statement.
fn first_nontrivial_statement(suite: &[ast::Stmt]) -> Option<&ast::Stmt> {
    suite.iter().find(|stmt| !is_trivial_statement(stmt))
}

#[derive(Debug, Clone, Copy)]
struct UncalledFunction {
    has_parameters: bool,
    should_await: bool,
}

impl UncalledFunction {
    /// Add a suggestion and fix that they might have meant to call (and possibly
    /// also await) this function.
    ///
    /// It's true that calling the function might not actually fix this diagnostic
    /// if the function returns something that is always truthy. They still probably
    /// meant to call the function, though, so it's still a useful suggestion/fix!
    fn suggest_call(self, context: &InferContext, diagnostic: &mut Diagnostic, test: &ast::Expr) {
        if matches!(test, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
            let (call, applicability) = if self.has_parameters {
                ("(...)", Applicability::DisplayOnly)
            } else {
                ("()", Applicability::Unsafe)
            };
            let call_edit = Edit::insertion(call.to_string(), test.end());
            let prefix = if self.should_await { "await " } else { "" };
            let fix = if self.should_await {
                Fix::applicable_edits(
                    Edit::insertion(prefix.to_string(), test.start()),
                    [call_edit],
                    applicability,
                )
            } else {
                Fix::applicable_edit(call_edit, applicability)
            };
            let source = source_text(context.db(), context.file());
            diagnostic.help(format_args!(
                "Replace with `{prefix}{}{call}`",
                &source[test.range()]
            ));
            diagnostic.set_fix(fix);
        }
    }
}

/// A first-party `Iterable` annotation that can be replaced with `Collection`.
#[derive(Debug)]
struct IterableAnnotation {
    range: FileRange,
    /// The scope where the annotation's names are resolved. Import validation uses this scope
    /// because names can be shadowed differently at the truthiness test.
    scope: FileScopeId,
    iterable_ranges: Option<IterableAnnotationRanges>,
}

/// Source ranges for replacing `Iterable` while preserving the element annotation.
///
/// In this function's stringized parameter annotation, `origin` covers `Iterable` inside the
/// string, and `element` covers `P` inside the string:
///
/// ```python
/// from collections.abc import Iterable
/// from pathlib import Path as P
///
/// def check(items: "Iterable[P]"):
///     if items:
///         print("Received paths")
/// ```
///
/// The fix adds a `Collection` import and replaces `origin`, producing `"Collection[P]"` while
/// preserving the original quotes and the alias `P`.
#[derive(Debug, Clone, Copy)]
struct IterableAnnotationRanges {
    origin: TextRange,
    element: TextRange,
}
