use ast::StmtFunctionDef;
use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for too many instance attributes in a class constructor.
///
/// ## Why is this bad?
/// Having too many instance attributes in a class constructor can make the
/// class harder to understand and maintain.
///
/// ## Example
/// ```python
/// class Fruit:  # [too-many-instance-attributes]
///     def __init__(self):
///         # max of 7 attributes by default, can be configured
///         self.worm_name = "Jimmy"
///         self.worm_type = "Codling Moths"
///         self.worm_color = "light brown"
///         self.fruit_name = "Little Apple"
///         self.fruit_color = "Bright red"
///         self.fruit_vitamins = ["A", "B1"]
///         self.fruit_antioxidants = None
///         self.secondary_worm_name = "Kim"
///         self.secondary_worm_type = "Apple maggot"
///         self.secondary_worm_color = "Whitish"
/// ```
///
/// Use instead:
/// ```python
/// import dataclasses
///
///
/// @dataclasses.dataclass
/// class Worm:
///     name: str
///     type: str
///     color: str
///
///
/// class Fruit:
///     def __init__(self):
///         self.name = "Little Apple"
///         self.color = "Bright red"
///         self.vitamins = ["A", "B1"]
///         self.antioxidants = None
///         self.worms = [
///             Worm(name="Jimmy", type="Codling Moths", color="light brown"),
///             Worm(name="Kim", type="Apple maggot", color="Whitish"),
///         ]
/// ```
///
#[violation]
pub struct TooManyInstanceAttributes {
    current_amount: usize,
    max_amount: usize,
}

impl Violation for TooManyInstanceAttributes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyInstanceAttributes {
            current_amount,
            max_amount,
        } = self;
        format!("Too many instance attributes ({current_amount}/{max_amount})")
    }
}

/// R0902
pub(crate) fn too_many_instance_attributes(checker: &mut Checker, function_def: &StmtFunctionDef) {
    let StmtFunctionDef { name, body, .. } = function_def;

    if name != "__init__" {
        return;
    }

    let mut instance_attributes: Vec<String> = Vec::new();

    for stmt in body {
        if let Stmt::Assign(assign) = stmt {
            for target in &assign.targets {
                check_expr(target, &mut instance_attributes);
            }
        }
    }

    if instance_attributes.len() > checker.settings.pylint.max_instance_attributes {
        let eol = checker.locator().line_end(function_def.start());

        let range = TextRange::new(function_def.start(), eol);

        let diagnostic = Diagnostic::new(
            TooManyInstanceAttributes {
                current_amount: instance_attributes.len(),
                max_amount: checker.settings.pylint.max_instance_attributes,
            },
            range,
        );
        checker.diagnostics.push(diagnostic);
    }
}

fn check_expr(expr: &Expr, names: &mut Vec<String>) {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for target in elts {
                check_expr(target, names);
            }
        }
        Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                if id == "self" {
                    let attr = attr.to_string();
                    if names.contains(&attr) {
                        return;
                    }
                    names.push(attr);
                }
            }
        }

        _ => {}
    }
}
