use anyhow::{anyhow, Result};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::whitespace::indentation;
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_python_semantic::context::Context;

use crate::autofix::actions::get_or_import_symbol;
use crate::checkers::ast::Checker;
use crate::importer::Importer;
use crate::registry::Rule;

#[violation]
pub struct AbstractBaseClassWithoutAbstractMethod {
    pub name: String,
}

impl Violation for AbstractBaseClassWithoutAbstractMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AbstractBaseClassWithoutAbstractMethod { name } = self;
        format!("`{name}` is an abstract base class, but it has no abstract methods")
    }
}

#[violation]
pub struct EmptyMethodWithoutAbstractDecorator {
    pub name: String,
}

impl AlwaysAutofixableViolation for EmptyMethodWithoutAbstractDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyMethodWithoutAbstractDecorator { name } = self;
        format!(
            "`{name}` is an empty method in an abstract base class, but has no abstract decorator"
        )
    }

    fn autofix_title(&self) -> String {
        "Add the `@abstractmethod` decorator".to_string()
    }
}

fn is_abc_class(context: &Context, bases: &[Expr], keywords: &[Keyword]) -> bool {
    keywords.iter().any(|keyword| {
        keyword
            .node
            .arg
            .as_ref()
            .map_or(false, |arg| arg == "metaclass")
            && context
                .resolve_call_path(&keyword.node.value)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["abc", "ABCMeta"]
                })
    }) || bases.iter().any(|base| {
        context
            .resolve_call_path(base)
            .map_or(false, |call_path| call_path.as_slice() == ["abc", "ABC"])
    })
}

fn is_empty_body(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| match &stmt.node {
        StmtKind::Pass => true,
        StmtKind::Expr { value } => match &value.node {
            ExprKind::Constant { value, .. } => {
                matches!(value, Constant::Str(..) | Constant::Ellipsis)
            }
            _ => false,
        },
        _ => false,
    })
}

fn fix_abstractmethod_missing(
    context: &Context,
    importer: &Importer,
    locator: &Locator,
    stylist: &Stylist,
    stmt: &Stmt,
) -> Result<Fix> {
    let indent = indentation(locator, stmt).ok_or(anyhow!("Unable to detect indentation"))?;
    let (import_edit, binding) =
        get_or_import_symbol("abc", "abstractmethod", context, importer, locator)?;
    let reference_edit = Edit::insertion(
        format!(
            "@{binding}{line_ending}{indent}",
            line_ending = stylist.line_ending().as_str(),
        ),
        stmt.range().start(),
    );
    Ok(Fix::from_iter([import_edit, reference_edit]))
}

/// B024
/// B027
pub fn abstract_base_class(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
    body: &[Stmt],
) {
    if bases.len() + keywords.len() != 1 {
        return;
    }
    if !is_abc_class(&checker.ctx, bases, keywords) {
        return;
    }

    let mut has_abstract_method = false;
    for stmt in body {
        // https://github.com/PyCQA/flake8-bugbear/issues/293
        // Ignore abc's that declares a class attribute that must be set
        if let StmtKind::AnnAssign { .. } | StmtKind::Assign { .. } = &stmt.node {
            has_abstract_method = true;
            continue;
        }

        let (
            StmtKind::FunctionDef {
                decorator_list,
                body,
                name: method_name,
                ..
            } | StmtKind::AsyncFunctionDef {
                decorator_list,
                body,
                name: method_name,
                ..
            }
        ) = &stmt.node else {
            continue;
        };

        let has_abstract_decorator = is_abstract(&checker.ctx, decorator_list);
        has_abstract_method |= has_abstract_decorator;

        if !checker
            .settings
            .rules
            .enabled(Rule::EmptyMethodWithoutAbstractDecorator)
        {
            continue;
        }

        if !has_abstract_decorator
            && is_empty_body(body)
            && !is_overload(&checker.ctx, decorator_list)
        {
            let mut diagnostic = Diagnostic::new(
                EmptyMethodWithoutAbstractDecorator {
                    name: format!("{name}.{method_name}"),
                },
                stmt.range(),
            );
            if checker.patch(Rule::EmptyMethodWithoutAbstractDecorator) {
                diagnostic.try_set_fix(|| {
                    fix_abstractmethod_missing(
                        &checker.ctx,
                        &checker.importer,
                        checker.locator,
                        checker.stylist,
                        stmt,
                    )
                });
            }
            checker.diagnostics.push(diagnostic);
        }
    }
    if checker
        .settings
        .rules
        .enabled(Rule::AbstractBaseClassWithoutAbstractMethod)
    {
        if !has_abstract_method {
            checker.diagnostics.push(Diagnostic::new(
                AbstractBaseClassWithoutAbstractMethod {
                    name: name.to_string(),
                },
                stmt.range(),
            ));
        }
    }
}
