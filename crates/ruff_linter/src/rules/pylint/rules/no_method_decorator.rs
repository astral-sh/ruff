use std::collections::HashMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprKind, NameId, Stmt, StmtKind};
use ruff_python_trivia::indentation_at_offset;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for the use of a classmethod being made without the decorator.
///
/// ## Why is this bad?
/// When it comes to consistency and readability, it's preferred to use the decorator.
///
/// ## Example
///
/// ```python
/// class Foo:
///     def bar(cls): ...
///
///     bar = classmethod(bar)
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo:
///     @classmethod
///     def bar(cls): ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.7", category = Category::Style)]
pub(crate) struct NonDecoratorClassmethod;

impl AlwaysFixableViolation for NonDecoratorClassmethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class method defined without decorator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add @classmethod decorator".to_string()
    }
}

/// ## What it does
/// Checks for the use of a staticmethod being made without the decorator.
///
/// ## Why is this bad?
/// When it comes to consistency and readability, it's preferred to use the decorator.
///
/// ## Example
///
/// ```python
/// class Foo:
///     def bar(arg1, arg2): ...
///
///     bar = staticmethod(bar)
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo:
///     @staticmethod
///     def bar(arg1, arg2): ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.7", category = Category::Style)]
pub(crate) struct NonDecoratorStaticmethod;

impl AlwaysFixableViolation for NonDecoratorStaticmethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Static method defined without decorator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add @staticmethod decorator".to_string()
    }
}

enum MethodType {
    Classmethod,
    Staticmethod,
}

/// PLR0202
pub(crate) fn no_classmethod_decorator(checker: &Checker, stmt: Stmt<'_>) {
    get_undecorated_methods(checker, stmt, &MethodType::Classmethod);
}

/// PLR0203
pub(crate) fn no_staticmethod_decorator(checker: &Checker, stmt: Stmt<'_>) {
    get_undecorated_methods(checker, stmt, &MethodType::Staticmethod);
}

fn get_undecorated_methods(checker: &Checker, class_stmt: Stmt<'_>, method_type: &MethodType) {
    let StmtKind::ClassDef(class_def) = class_stmt.classify() else {
        return;
    };

    let mut explicit_decorator_calls: HashMap<NameId, Stmt<'_>> = HashMap::default();

    let method_name = match method_type {
        MethodType::Classmethod => "classmethod",
        MethodType::Staticmethod => "staticmethod",
    };

    // gather all explicit *method calls
    for stmt in class_def.body() {
        if let StmtKind::Assign(assign) = stmt.classify()
            && let Some(call) = assign.value().as_call_expr()
            && let Some(func) = call.func().as_name_expr()
            && func.id() == method_name
            && checker.semantic().has_builtin_binding(method_name)
            && let Some([target]) = assign.targets().as_array()
            && let Some(target) = target.as_name_expr()
            && let Some([arg]) = call.arguments().args().as_array()
            && let Some(arg) = arg.as_name_expr()
            && arg.name_id() == target.name_id()
        {
            explicit_decorator_calls.insert(arg.name_id(), stmt);
        }
    }

    if explicit_decorator_calls.is_empty() {
        return;
    }

    for stmt in class_def.body() {
        if let StmtKind::FunctionDef(function_def) = stmt.classify() {
            let name = function_def.name();
            let decorator_list = function_def.decorator_list();
            let Some(decorator_call_statement) = explicit_decorator_calls.get(&name.name_id())
            else {
                continue;
            };

            // if we find the decorator we're looking for, skip
            if decorator_list.iter().any(|decorator| {
                if let ExprKind::Name(name_node) = decorator.expression().classify() {
                    let id = name_node.id();
                    if id == method_name && checker.semantic().has_builtin_binding(method_name) {
                        return true;
                    }
                }

                false
            }) {
                continue;
            }

            let range = TextRange::new(stmt.range().start(), stmt.range().start());
            let mut diagnostic = match method_type {
                MethodType::Classmethod => {
                    checker.report_diagnostic(NonDecoratorClassmethod, range)
                }
                MethodType::Staticmethod => {
                    checker.report_diagnostic(NonDecoratorStaticmethod, range)
                }
            };

            let indentation = indentation_at_offset(stmt.range().start(), checker.source());

            match indentation {
                Some(indentation) => {
                    diagnostic.set_fix(Fix::safe_edits(
                        Edit::insertion(
                            format!("@{method_name}\n{indentation}"),
                            stmt.range().start(),
                        ),
                        [fix::edits::delete_stmt(
                            *decorator_call_statement,
                            Some(class_stmt),
                            checker.locator(),
                            checker.indexer(),
                        )],
                    ));
                }
                None => {
                    continue;
                }
            }
        }
    }
}
