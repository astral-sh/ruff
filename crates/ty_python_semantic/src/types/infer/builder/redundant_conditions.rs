//! Analysis of whether a boolean test should be reported as being unintentionally
//! always-true or always-false.

use ruff_db::{parsed::parsed_module, source::source_text};
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::{self as ast, helpers::any_over_expr};
use ruff_text_size::Ranged;
use ty_module_resolver::KnownModule;
use ty_python_core::{Truthiness, definition::Definition, scope::NodeWithScopeKind};

use crate::{
    Db, ImportAliasResolution, SemanticModel, definitions_for_expression,
    types::{
        KnownClass, Type,
        diagnostic::{REDUNDANT_CONDITION, REDUNDANT_CONDITION_STRICT},
        infer::{InferenceFlags, TypeInferenceBuilder},
        infer_definition_types,
        tuple::TupleLength,
    },
};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn should_check_condition_redundancy(&self) -> bool {
        if !self.db().should_check_file(self.file()) {
            return false;
        }

        self.context.is_lint_enabled(&REDUNDANT_CONDITION)
            || self.context.is_lint_enabled(&REDUNDANT_CONDITION_STRICT)
    }

    pub(super) fn check_condition_redundancy(
        &self,
        test: &ast::Expr,
        test_type: Type<'db>,
        test_truthiness: Truthiness,
    ) {
        if test_truthiness == Truthiness::Ambiguous {
            return;
        }

        let db = self.db();
        let env = self.program_environment();
        let int_instance = KnownClass::Int.to_instance(db, env);

        match test {
            // Python checks the truthiness of all but the final `and`/`or` operand to decide
            // whether to short-circuit. If evaluation reaches the final operand, its value is
            // simply returned. Accordingly, `infer_boolean_expression` passes the earlier
            // operands to this method, but never passes the complete expression it is inferring.
            //
            // Receiving the complete `ast::Expr::BoolOp` expression here means a surrounding
            // context, such as an `if`, a `while`, or an outer `and`/`or`, is checking its
            // truthiness. This distinction determines whether the final operand also needs
            // checking:
            //
            // - In `result = flag and func`, `func` is merely a possible result. Its truthiness
            //   is not checked, so it should not produce a diagnostic.
            // - In `if True and func`, the `if` checks the result's truthiness. The result is
            //   `func`, so the uncalled function should produce a diagnostic.
            //
            // Check the final operand whenever the complete expression reaches this method;
            // `infer_boolean_expression` has already checked the earlier operands. For values
            // handled by `redundant-condition`, these operand checks are sufficient: checking the
            // complete expression again would duplicate a diagnostic. Values assignable to `int`,
            // including booleans, use `redundant-condition-strict` instead. That rule suppresses
            // diagnostics on subexpressions of conditions, so the complete expression still needs
            // to be checked.
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                if let Some(last) = values.last() {
                    let ty = self.expression_type(last);
                    self.check_condition_redundancy(last, ty, ty.bool(db, env));
                }

                if !test_type.is_assignable_to(db, env, int_instance) {
                    return;
                }
            }

            // A negated condition reaches this method twice: `infer_unary_expression_type`
            // checks the operand, and the enclosing `if` or `while` checks the whole condition.
            // Whether the second check should produce a diagnostic depends on the operand:
            //
            // - For `if not func`, the operand check already reports the uncalled function under
            //   `redundant-condition`. Checking the boolean `not func` as well would add a
            //   duplicate `redundant-condition-strict` diagnostic.
            // - For `if not False` or `if not 0`, the operand would use the strict rule. That rule
            //   skips subexpressions of conditions to avoid reporting both a condition and its
            //   parts, so the operand check emits nothing. Checking the whole `not` expression is
            //   therefore necessary to report the redundant condition.
            //
            // Check the whole condition only when the original operand is boolean- or
            // integer-like. Unwrap every `not` first: in `if not not func`, the immediate operand
            // has type `bool`, but the original operand is still `func` and was already reported.
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) => {
                let mut original_operand = operand;
                while let ast::Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand,
                    ..
                }) = &**original_operand
                {
                    original_operand = operand;
                }

                if !self
                    .expression_type(original_operand)
                    .is_assignable_to(db, env, int_instance)
                {
                    return;
                }
            }
            _ => {}
        }

        let rule = if test_type.is_assignable_to(db, env, int_instance) {
            if self
                .index
                .is_assertion_test_or_compound_condition_subexpression(
                    self.scope().file_scope_id(db),
                    test.range(),
                )
            {
                return;
            }
            if !self.context.is_lint_enabled(&REDUNDANT_CONDITION_STRICT) {
                return;
            }
            &REDUNDANT_CONDITION_STRICT
        } else {
            if !self.context.is_lint_enabled(&REDUNDANT_CONDITION) {
                return;
            }
            &REDUNDANT_CONDITION
        };

        let model = SemanticModel::new(db, self.program_file());

        if any_over_expr(test, |expression| {
            is_special_cased_condition_expression(db, &model, expression, |expr| {
                self.expression_type(expr)
            })
        }) {
            return;
        }

        match test_truthiness {
            Truthiness::AlwaysTrue => {
                if let Some(builder) = self.context.report_lint(rule, test) {
                    if let Type::FunctionLiteral(function) = test_type {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Function `{}` is always truthy",
                            function.name(db)
                        ));
                        let signatures = function.signature(db);
                        if signatures.iter().all(|signature| {
                            signature.return_ty.bool(db, env) == Truthiness::Ambiguous
                        }) {
                            diagnostic.set_primary_annotation_message(
                                "Did you mean to call this function?",
                            );
                            if matches!(test, ast::Expr::Name(_) | ast::Expr::Attribute(_))
                                && !signatures.has_parameters()
                            {
                                diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                                    "()".to_string(),
                                    test.end(),
                                )));
                            }
                        }
                    } else if let Some(tuple_spec) = test_type.tuple_instance_spec(db, env) {
                        let message = match tuple_spec.len() {
                            TupleLength::Fixed(size) => {
                                format!("A {size}-element tuple is always truthy")
                            }
                            TupleLength::Variable(min, _) => {
                                format!(
                                    "A tuple with >={min} element{maybe_s} is always truthy",
                                    maybe_s = if min == 1 { "" } else { "s" }
                                )
                            }
                        };
                        let mut diagnostic = builder.into_diagnostic(&message);
                        diagnostic.set_concise_message(format_args!(
                            "Object of type `{}` is always truthy",
                            test_type.display(db, env)
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                    } else if test_type.as_nominal_instance().is_some_and(|instance| {
                        instance
                            .class(db, env)
                            .is_known(db, KnownClass::GeneratorType)
                    }) {
                        let mut diagnostic =
                            builder.into_diagnostic("A generator is always truthy");
                        diagnostic.set_concise_message(format_args!(
                            "Object of type `{}` is always truthy",
                            test_type.display(db, env)
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                        diagnostic.help("Did you mean to collect the generator into a tuple?");
                        diagnostic.set_fix(Fix::display_only_edits(
                            Edit::insertion("tuple(".to_string(), test.start()),
                            [Edit::insertion(")".to_string(), test.end())],
                        ));
                    } else if test_type.is_string_literal()
                        || test_type.as_union().is_some_and(|union| {
                            union.elements(db).iter().all(Type::is_string_literal)
                        })
                    {
                        let mut diagnostic =
                            builder.into_diagnostic("A nonempty string is always truthy");
                        diagnostic.set_concise_message(format_args!(
                            "Object of type `{}` is always truthy",
                            test_type.display(db, env)
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                    } else if test_type.is_subtype_of(
                        db,
                        env,
                        KnownClass::Bool.to_instance(db, env),
                    ) {
                        let message = "Condition is always true";
                        let mut diagnostic = builder.into_diagnostic(message);
                        diagnostic.set_concise_message(message);
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                    } else {
                        let mut diagnostic = builder.into_diagnostic("Condition is always truthy");
                        diagnostic.set_concise_message(format_args!(
                            "Object of type `{}` is always truthy",
                            test_type.display(db, env)
                        ));
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                        if test_type.try_await(db, env).is_ok() && self.can_await_here() {
                            diagnostic.help("Did you mean to `await` this expression?");

                            let fix = if test.precedence() <= ast::OperatorPrecedence::Await {
                                Fix::unsafe_edits(
                                    Edit::insertion("await (".to_string(), test.start()),
                                    [Edit::insertion(")".to_string(), test.end())],
                                )
                            } else {
                                Fix::unsafe_edit(Edit::insertion(
                                    "await ".to_string(),
                                    test.start(),
                                ))
                            };

                            diagnostic.set_fix(fix);
                        }
                    }
                }
            }
            Truthiness::AlwaysFalse => {
                if let Some(builder) = self.context.report_lint(rule, test) {
                    if test_type.is_none(db) {
                        builder.into_diagnostic("`None` is always falsy");
                    } else if let Some(tuple) = test_type.tuple_instance_spec(db, env)
                        && tuple.len() == TupleLength::Fixed(0)
                    {
                        let message = "An empty tuple is always falsy";
                        let mut diagnostic = builder.into_diagnostic(message);
                        diagnostic.set_concise_message(message);
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                    } else if test_type.is_string_literal() {
                        let message = "An empty string is always falsy";
                        let mut diagnostic = builder.into_diagnostic(message);
                        diagnostic.set_concise_message(message);
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                    } else {
                        let is_bool =
                            test_type.is_subtype_of(db, env, KnownClass::Bool.to_instance(db, env));
                        let message = if is_bool {
                            "Condition is always false"
                        } else {
                            "Condition is always falsy"
                        };
                        let mut diagnostic = builder.into_diagnostic(message);
                        if is_bool {
                            diagnostic.set_concise_message(message);
                        } else {
                            diagnostic.set_concise_message(format_args!(
                                "Object of type `{}` is always falsy",
                                test_type.display(db, env)
                            ));
                        }
                        diagnostic.set_primary_annotation_message(format_args!(
                            "Inferred type is `{}`",
                            test_type.display(db, env)
                        ));
                    }
                }
            }
            Truthiness::Ambiguous => {}
        }
    }

    fn can_await_here(&self) -> bool {
        // Python forbids `await` in annotation nodes.
        if self
            .inference_flags()
            .contains(InferenceFlags::IN_ANNOTATION)
        {
            return false;
        }

        let db = self.db();

        for (_, scope) in self.index.ancestor_scopes(self.scope().file_scope_id(db)) {
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
                | NodeWithScopeKind::SetComprehension(_) => continue,
            }
        }

        false
    }

    pub(super) fn check_suite_for_redundant_if_statements(&self, suite: &[ast::Stmt]) {
        let db = self.db();
        let env = self.program_environment();

        for (i, statement) in suite.iter().enumerate() {
            let ast::Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) = statement
            else {
                continue;
            };

            let test_type = self.expression_type(test);
            let test_truthiness = test_type.bool(db, env);

            match test_truthiness {
                Truthiness::Ambiguous => {}
                Truthiness::AlwaysFalse => {
                    if !self.is_deliberately_unreachable_suite(body) {
                        self.check_condition_redundancy(test, test_type, test_truthiness);
                    }
                }
                Truthiness::AlwaysTrue => match elif_else_clauses.as_slice() {
                    [single] => {
                        if !(single.test.is_none()
                            && self.is_deliberately_unreachable_suite(&single.body))
                        {
                            self.check_condition_redundancy(test, test_type, test_truthiness);
                        }
                    }
                    [] => {
                        if !self.is_deliberately_unreachable_suite(&suite[i + 1..]) {
                            self.check_condition_redundancy(test, test_type, test_truthiness);
                        }
                    }
                    _ => {
                        self.check_condition_redundancy(test, test_type, test_truthiness);
                    }
                },
            }

            for (elif_i, elif_else) in elif_else_clauses.iter().enumerate() {
                let ast::ElifElseClause {
                    body,
                    test: Some(test),
                    ..
                } = elif_else
                else {
                    break;
                };

                let test_type = self.expression_type(test);
                let test_truthiness = test_type.bool(db, env);

                match test_truthiness {
                    Truthiness::Ambiguous => continue,
                    Truthiness::AlwaysFalse => {
                        if self.is_deliberately_unreachable_suite(body) {
                            continue;
                        }
                    }
                    Truthiness::AlwaysTrue => match elif_else_clauses.get(elif_i + 1) {
                        Some(clause) => {
                            if clause.test.is_none()
                                && self.is_deliberately_unreachable_suite(&clause.body)
                            {
                                continue;
                            }
                        }
                        None => {
                            if self.is_deliberately_unreachable_suite(&suite[i + 1..]) {
                                continue;
                            }
                        }
                    },
                }

                self.check_condition_redundancy(test, test_type, test_truthiness);
            }
        }
    }

    fn is_deliberately_unreachable_suite(&self, suite: &[ast::Stmt]) -> bool {
        if suite.iter().all(|stmt| {
            stmt.as_expr_stmt()
                .is_some_and(|stmt_expr| stmt_expr.value.is_string_literal_expr())
        }) {
            return false;
        }

        let db = self.db();
        let env = self.program_environment();

        let not_implemented = KnownClass::NotImplementedType.to_instance(db, env);

        suite.iter().all(|stmt| match stmt {
            ast::Stmt::Raise(_) => true,
            ast::Stmt::Assert(ast::StmtAssert { test, .. }) => {
                self.expression_type(test).bool(db, env).may_be_false()
            }
            ast::Stmt::Expr(ast::StmtExpr { value, .. }) => match &**value {
                ast::Expr::StringLiteral(..) => true,
                ast::Expr::Call(..) => {
                    self.expression_type(value)
                        .is_equivalent_to(db, env, Type::Never)
                }
                _ => false,
            },
            ast::Stmt::Return(ast::StmtReturn {
                value: Some(expr), ..
            }) => self
                .expression_type(expr)
                .is_assignable_to(db, env, not_implemented),
            _ => false,
        })
    }
}

/// Recognizes environment-dependent conditions, including constants reached through aliases.
fn is_special_cased_condition_expression<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    expression: &ast::Expr,
    expression_type: impl Fn(&ast::Expr) -> Type<'db>,
) -> bool {
    match expression {
        ast::Expr::Name(ast::ExprName { id, .. }) if id == "TYPE_CHECKING" => return true,
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => match &**attr {
            "TYPE_CHECKING" => return true,
            "name" => {
                if let Type::ModuleLiteral(module) = expression_type(value)
                    && module.module(db).is_known(db, KnownModule::Os)
                {
                    return true;
                }
            }
            "version_info" | "platform" => {
                if let Type::ModuleLiteral(module) = expression_type(value)
                    && module.module(db).is_known(db, KnownModule::Sys)
                {
                    return true;
                }
            }
            _ => {}
        },
        _ => {}
    }

    if !matches!(expression, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
        return false;
    }

    definitions_for_expression(
        model,
        expression.into(),
        ImportAliasResolution::ResolveAliases,
    )
    .into_iter()
    .flatten()
    .filter_map(|resolved| resolved.definition())
    .any(|definition| definition_contains_special_cased_condition(db, definition))
}

/// Follows assignment aliases without making callers depend directly on another file's AST.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| false,
    heap_size = ruff_memory_usage::heap_size
)]
fn definition_contains_special_cased_condition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> bool {
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let Some(value) = definition.kind(db).value(&module) else {
        return false;
    };

    let program_file = definition.program_file(db);
    let model = SemanticModel::new(db, program_file);
    let inference = infer_definition_types(db, definition);

    any_over_expr(value, |expression| {
        is_special_cased_condition_expression(db, &model, expression, |expr| {
            inference.expression_type(expr)
        })
    })
}
