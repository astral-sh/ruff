use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExcepthandlerKind, Stmt, StmtKind};

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct TooManyStatements {
        pub statements: usize,
        pub max_statements: usize,
    }
);
impl Violation for TooManyStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyStatements {
            statements,
            max_statements,
        } = self;
        format!("Too many statements ({statements}/{max_statements})")
    }
}

fn num_statements(stmts: &[Stmt]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        // TODO(charlie): Account for pattern match statement.
        match &stmt.node {
            StmtKind::If { body, orelse, .. } => {
                count += 1;
                count += num_statements(body);
                if let Some(stmt) = orelse.first() {
                    // `elif:` and `else: if:` have the same AST representation.
                    // Avoid treating `elif:` as two statements.
                    if !matches!(stmt.node, StmtKind::If { .. }) {
                        count += 1;
                    }
                    count += num_statements(orelse);
                }
            }
            StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
                count += num_statements(body);
                count += num_statements(orelse);
            }
            StmtKind::While { body, orelse, .. } => {
                count += 1;
                count += num_statements(body);
                count += num_statements(orelse);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
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
                    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
                    count += num_statements(body);
                }
            }
            StmtKind::FunctionDef { body, .. }
            | StmtKind::AsyncFunctionDef { body, .. }
            | StmtKind::With { body, .. } => {
                count += 1;
                count += num_statements(body);
            }
            StmtKind::Return { .. } => {}
            _ => {
                count += 1;
            }
        }
    }
    count
}

/// PLR0915
pub fn too_many_statements(
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
    use rustpython_parser::parser;

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
