use ruff_db::parsed::parsed_module;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::{self as ast, helpers::any_over_expr};
use ruff_text_size::Ranged;
use ty_module_resolver::KnownModule;
use ty_python_core::{Truthiness, definition::Definition};

use crate::{
    Db, ImportAliasResolution, ProgramEnvironment, SemanticModel, definitions_for_expression,
    types::{
        KnownClass, Type,
        diagnostic::{REDUNDANT_CONDITION, REDUNDANT_CONDITION_STRICT},
        infer::TypeInferenceBuilder,
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

        match test {
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                if let Some(last) = values.last() {
                    let ty = self.expression_type(last);
                    self.check_condition_redundancy(last, ty, ty.bool(db, env));
                }
            }
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                ..
            }) => return,
            _ => {}
        }

        let int_instance = KnownClass::Int.to_instance(db, env);

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
            is_special_cased_condition_expression(db, env, &model, expression, |expr| {
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
                        diagnostic
                            .set_primary_annotation_message("Did you mean to call this function?");
                        if !function.signature(db).has_parameters() {
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                                "()".to_string(),
                                test.end(),
                            )));
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
                        if test_type.try_await(db, env).is_ok() {
                            diagnostic.help("Did you mean to `await` this expression?");
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                                "await ".to_string(),
                                test.start(),
                            )));
                        }
                    }
                }
            }
            Truthiness::AlwaysFalse => {
                if let Some(builder) = self.context.report_lint(rule, test) {
                    if test_type.is_none(db) {
                        builder.into_diagnostic("`None` is always falsy");
                    } else if let Some(tuple) = test_type.tuple_instance_spec(db, env) {
                        debug_assert_eq!(tuple.len(), TupleLength::Fixed(0));
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
}

/// Recognizes environment-dependent conditions, including constants reached through aliases.
fn is_special_cased_condition_expression<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
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

    if expression_type(expression).is_subtype_of(
        db,
        env,
        KnownClass::NotImplementedType.to_instance(db, env),
    ) {
        return true;
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
    let env = ProgramEnvironment::from_file(program_file);
    let model = SemanticModel::new(db, program_file);
    let inference = infer_definition_types(db, definition);

    any_over_expr(value, |expression| {
        is_special_cased_condition_expression(db, &env, &model, expression, |expr| {
            inference.expression_type(expr)
        })
    })
}
