use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    Expr, ExprAttribute, ExprCall, ExprName, Stmt, StmtClassDef, StmtExpr, StmtFunctionDef,
};

use crate::checkers::ast::Checker;

/// ### What it does
/// Checks to see another argument other than the current class is given as the first argument of the super builtin
///
/// ### Why is this bad?
/// In Python 2.7, `super()` has to be called with its own class and `self`
/// as arguments `(super(Cat, self))`, which can lead to a mix up of parent and child class in the code.
///
/// In Python 3 the recommended way is to call `super()` without arguments
/// (see also [`super-with-arguments`](https://pylint.readthedocs.io/en/latest/user_guide/messages/refactor/super-with-arguments.html)).
///
/// ## Problematic code
/// ```python
/// class Animal:
///     pass
///
///
/// class Tree:
///     pass
///
///
/// class Cat(Animal):
///     def __init__(self):
///         super(Tree, self).__init__()  # [bad-super-call]
///         super(Animal, self).__init__()
/// ```
///
/// ## Correct code
/// ```python
/// class Animal:
///     pass
///    
///  
/// class Tree:
///     pass
///    
///    
/// class Cat(Animal):
///     def __init__(self):
///         super(Animal, self).__init__()
/// ```
///
/// ## References
/// - [Python documentation: `class super`](https://docs.python.org/3/library/functions.html#super)
#[violation]
pub struct BadSuperCall {
    bad_super_arg: String,
}

impl Violation for BadSuperCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { bad_super_arg } = self;
        format!("Bad first argument '{bad_super_arg}' given to super()")
    }
}

fn is_bad_super_call<'a>(first_arg: &'a Expr, current_class_name: &str) -> Option<&'a str> {
    let Expr::Name(ExprName { id, .. }) = first_arg else {
        return None;
    };
    (id != current_class_name).then_some(id)
}

fn check_expr(expr: &Expr, current_class_name: &str) -> Option<Diagnostic> {
    if let Expr::Call(ExprCall {
        range,
        func,
        arguments,
    }) = expr
    {
        if let Expr::Name(ExprName { id, .. }) = func.as_ref() {
            if id == "super" {
                if let Some(first) = arguments.args.first() {
                    return is_bad_super_call(first, current_class_name).map(|id| {
                        Diagnostic::new(
                            BadSuperCall {
                                bad_super_arg: id.to_owned(),
                            },
                            range.to_owned(),
                        )
                    });
                }
            }
        }
        return check_expr(func, current_class_name);
    } else if let Expr::Attribute(ExprAttribute { value, .. }) = expr {
        return check_expr(value, current_class_name);
    }
    None
}

fn traverse_body(body: &[Stmt], current_class_name: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    for item in body {
        if let Stmt::FunctionDef(StmtFunctionDef { body, .. }) = item {
            out.extend(traverse_body(body, current_class_name));
        } else if let Stmt::Expr(StmtExpr { range: _, value }) = item {
            if let Some(diagnostic) = check_expr(value, current_class_name) {
                out.push(diagnostic);
            }
        }
    }
    out
}

/// PLE1003
pub(crate) fn bad_super_call(
    checker: &mut Checker,
    StmtClassDef {
        name,
        arguments,
        body,
        ..
    }: &StmtClassDef,
) {
    if arguments.is_some() {
        let out = traverse_body(body, name.as_str());
        if !out.is_empty() {
            checker.diagnostics.extend(out);
        }
    }
}
