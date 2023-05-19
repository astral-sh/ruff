use rustpython_parser::ast::{self, Excepthandler, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for functions or methods with too many statements.
///
/// By default, this rule allows up to 50 statements, as configured by the
/// `pylint.max-statements` option.
///
/// ## Why is this bad?
/// Functions or methods with many statements are harder to understand
/// and maintain.
///
/// Instead, consider refactoring the function or method into smaller
/// functions or methods, or identifying generalizable patterns and
/// replacing them with generic logic or abstractions.
///
/// ## Example
/// ```python
/// def is_even(number: int) -> bool:
///     if number == 0:
///         return True
///     elif number == 1:
///         return False
///     elif number == 2:
///         return True
///     elif number == 3:
///         return False
///     elif number == 4:
///         return True
///     elif number == 5:
///         return False
///     else:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// def is_even(number: int) -> bool:
///     return number % 2 == 0
/// ```
///
/// ## Options
/// - `pylint.max-statements`
#[violation]
pub struct TooManyStatements {
    statements: usize,
    max_statements: usize,
}

impl Violation for TooManyStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyStatements {
            statements,
            max_statements,
        } = self;
        format!("Too many statements ({statements} > {max_statements})")
    }
}

fn num_statements(stmts: &[Stmt]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        match stmt {
            Stmt::If(ast::StmtIf { body, orelse, .. }) => {
                count += 1;
                count += num_statements(body);
                if let Some(stmt) = orelse.first() {
                    // `elif:` and `else: if:` have the same AST representation.
                    // Avoid treating `elif:` as two statements.
                    if !matches!(stmt, Stmt::If(_)) {
                        count += 1;
                    }
                    count += num_statements(orelse);
                }
            }
            Stmt::For(ast::StmtFor { body, orelse, .. })
            | Stmt::AsyncFor(ast::StmtAsyncFor { body, orelse, .. }) => {
                count += num_statements(body);
                count += num_statements(orelse);
            }
            Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                count += 1;
                count += num_statements(body);
                count += num_statements(orelse);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                count += 1;
                for case in cases {
                    count += num_statements(&case.body);
                }
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                range: _,
            })
            | Stmt::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                range: _,
            }) => {
                count += 1;
                count += num_statements(body);
                if !orelse.is_empty() {
                    count += 1 + num_statements(orelse);
                }
                if !finalbody.is_empty() {
                    // Unclear why, but follow Pylint's convention.
                    count += 2 + num_statements(finalbody);
                }
                if handlers.len() > 1 {
                    count += 1;
                }
                for handler in handlers {
                    count += 1;
                    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler {
                        body, ..
                    }) = handler;
                    count += num_statements(body);
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef { body, .. })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
            | Stmt::With(ast::StmtWith { body, .. }) => {
                count += 1;
                count += num_statements(body);
            }
            Stmt::Return(_) => {}
            _ => {
                count += 1;
            }
        }
    }
    count
}

/// PLR0915
pub(crate) fn too_many_statements(
    stmt: &Stmt,
    body: &[Stmt],
    max_statements: usize,
    locator: &Locator,
) -> Option<Diagnostic> {
    let statements = num_statements(body) + 1;
    if statements > max_statements {
        Some(Diagnostic::new(
            TooManyStatements {
                statements,
                max_statements,
            },
            identifier_range(stmt, locator),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser as parser;

    use super::num_statements;

    #[test]
    fn pass() -> Result<()> {
        let source: &str = r#"
def f():  # 2
    pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 2);
        Ok(())
    }

    #[test]
    fn if_else() -> Result<()> {
        let source: &str = r#"
def f():
    if a:
        print()
    else:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn if_else_if_corner() -> Result<()> {
        let source: &str = r#"
def f():
    if a:
        print()
    else:
        if a:
            print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn if_elif() -> Result<()> {
        let source: &str = r#"
def f():  # 5
    if a:
        print()
    elif a:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn if_elif_else() -> Result<()> {
        let source: &str = r#"
def f():  # 9
    if a:
        print()
    elif a == 2:
        print()
    elif a == 3:
        print()
    else:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 9);
        Ok(())
    }

    #[test]
    fn many_statements() -> Result<()> {
        let source: &str = r#"
async def f():  # 19
    a = 1
    b = 2
    c = 3
    await some_other_func()
    if a == 1:
        print('hello')
    else:
        other_func()
    count = 1
    while True:
        count += 1
        if count > 20:
            break;

    with open(f):
        with open(e):
            a -= 1
            import time
            pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 19);
        Ok(())
    }

    #[test]
    fn for_() -> Result<()> {
        let source: &str = r#"
def f():  # 2
    for i in range(10):
        pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 2);
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source: &str = r#"
def f():  # 3
    for i in range(10):
        print()
    else:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 3);
        Ok(())
    }

    #[test]
    fn nested_def() -> Result<()> {
        let source: &str = r#"
def f():  # 5
    def g():
        print()
        print()

    print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn nested_class() -> Result<()> {
        let source: &str = r#"
def f():  # 3
    class A:
        def __init__(self):
            pass

        def f(self):
            pass

    print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 3);
        Ok(())
    }

    #[test]
    fn return_not_counted() -> Result<()> {
        let source: &str = r#"
def f():
    return
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 1);
        Ok(())
    }

    #[test]
    fn with() -> Result<()> {
        let source: &str = r#"
def f():  # 6
    with a:
        if a:
            print()
        else:
            print()

"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 6);
        Ok(())
    }

    #[test]
    fn try_except() -> Result<()> {
        let source: &str = r#"
def f():  # 5
    try:
        print()
    except Exception:
        raise
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn try_except_else() -> Result<()> {
        let source: &str = r#"
def f():  # 7
    try:
        print()
    except ValueError:
        pass
    else:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 7);
        Ok(())
    }

    #[test]
    fn try_except_else_finally() -> Result<()> {
        let source: &str = r#"
def f():  # 10
    try:
        print()
    except ValueError:
        pass
    else:
        print()
    finally:
        pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 10);
        Ok(())
    }

    #[test]
    fn try_except_except() -> Result<()> {
        let source: &str = r#"
def f():  # 8
    try:
        print()
    except ValueError:
        pass
    except Exception:
        raise
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 8);
        Ok(())
    }

    #[test]
    fn try_except_except_finally() -> Result<()> {
        let source: &str = r#"
def f():  # 11
    try:
        print()
    except:
        pass
    except:
        pass
    finally:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 11);
        Ok(())
    }

    #[test]
    fn yield_() -> Result<()> {
        let source: &str = r#"
def f():  # 2
    for i in range(10):
        yield i
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_statements(&stmts), 2);
        Ok(())
    }
}
