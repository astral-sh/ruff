use rustpython_parser::ast::{self, ExcepthandlerKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;

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
        match &stmt.node {
            StmtKind::If(ast::StmtIf { body, orelse, .. }) => {
                count += 1;
                count += num_statements(body);
                if let Some(stmt) = orelse.first() {
                    // `elif:` and `else: if:` have the same AST representation.
                    // Avoid treating `elif:` as two statements.
                    if !matches!(stmt.node, StmtKind::If(_)) {
                        count += 1;
                    }
                    count += num_statements(orelse);
                }
            }
            StmtKind::For(ast::StmtFor { body, orelse, .. })
            | StmtKind::AsyncFor(ast::StmtAsyncFor { body, orelse, .. }) => {
                count += num_statements(body);
                count += num_statements(orelse);
            }
            StmtKind::While(ast::StmtWhile { body, orelse, .. }) => {
                count += 1;
                count += num_statements(body);
                count += num_statements(orelse);
            }
            StmtKind::Match(ast::StmtMatch { cases, .. }) => {
                count += 1;
                for case in cases {
                    count += num_statements(&case.body);
                }
            }
            StmtKind::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
            })
            | StmtKind::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
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
                    let ExcepthandlerKind::ExceptHandler(ast::ExcepthandlerExceptHandler {
                        body,
                        ..
                    }) = &handler.node;
                    count += num_statements(body);
                }
            }
            StmtKind::FunctionDef(ast::StmtFunctionDef { body, .. })
            | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
            | StmtKind::With(ast::StmtWith { body, .. }) => {
                count += 1;
                count += num_statements(body);
            }
            StmtKind::Return(_) => {}
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
