use crate::ast::helpers::identifier_range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

use ruff_macros::derive_message_formats;
use rustpython_ast::{ExcepthandlerKind, Stmt, StmtKind};

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

fn get_num_statements(stmts: &[Stmt]) -> usize {
    let mut count: usize = 0;
    for stmt in stmts {
        // TODO: Match will need be added once it's in ast.rs
        match &stmt.node {
            StmtKind::If { body, orelse, .. } => {
                count += 1;
                count += get_num_statements(body);
                if !orelse.is_empty() {
                    // else:
                    //   if:
                    // same as elif:
                    // but otherwise else: counts as its own statement
                    // necessary to avoid counting else and if separately when they appear after each other
                    match orelse.first().unwrap().node {
                        StmtKind::If { .. } => {}
                        _ => {
                            count += 1;
                        }
                    }
                    count += get_num_statements(orelse);
                }
            }
            StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
                count += get_num_statements(body);
                count += get_num_statements(orelse);
            }
            StmtKind::While {
                test: _,
                body,
                orelse,
            } => {
                count += 1;
                count += get_num_statements(body);
                count += get_num_statements(orelse);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                count += 1;
                count += get_num_statements(body);
                if !orelse.is_empty() {
                    count += 1 + get_num_statements(orelse);
                }
                if !finalbody.is_empty() {
                    // weird, but in making samples for pylint, the finally statement counts as 2
                    count += 2 + get_num_statements(finalbody);
                }
                if handlers.len() > 1 {
                    count += 1;
                }
                for handler in handlers {
                    count += 1;
                    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
                    count += get_num_statements(body);
                }
            }
            StmtKind::FunctionDef { body, .. }
            | StmtKind::AsyncFunctionDef { body, .. }
            | StmtKind::With { body, .. } => {
                count += 1;
                count += get_num_statements(body);
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
    let statements: usize = get_num_statements(body) + 1;
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

    use super::get_num_statements;

    #[test]
    fn test_pass() -> Result<()> {
        let source: &str = r#"
def f():
    pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 2);
        Ok(())
    }

    #[test]
    fn test_if_else() -> Result<()> {
        let source: &str = r#"
def f():
    if a:
        print()
    else:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn test_if_else_if_corner() -> Result<()> {
        let source: &str = r#"
def f():
    if a:
        print()
    else:
        if a:
            print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn test_if_elif() -> Result<()> {
        let source: &str = r#"
def f():
    if a:
        print()
    elif a:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn test_if_elif_else() -> Result<()> {
        let source: &str = r#"
def f():
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
        assert_eq!(get_num_statements(&stmts), 9); // counter-intuitive, but elif counts as 2 statements according to pylint itself
        Ok(())
    }

    #[test]
    fn many_statements() -> Result<()> {
        let source: &str = r#"
async def f():
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
        assert_eq!(get_num_statements(&stmts), 19);
        Ok(())
    }

    #[test]
    fn test_for() -> Result<()> {
        let source: &str = r#"
def f():  # 2
    for i in range(10):
        pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 2);
        Ok(())
    }

    #[test]
    fn test_for_else() -> Result<()> {
        let source: &str = r#"
def f():  # 3
    for i in range(10):
        print()
    else:
        print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 3);
        Ok(())
    }

    #[test]
    fn test_nested_def() -> Result<()> {
        let source: &str = r#"
def f():
    def g():
        print()
        print()

    print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn test_nested_class() -> Result<()> {
        let source: &str = r#"
def f():
    class A:
        def __init__(self):
            pass

        def f(self):
            pass

    print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 3);
        Ok(())
    }

    #[test]
    fn test_return_not_counted() -> Result<()> {
        let source: &str = r#"
def f():
    return
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 1);
        Ok(())
    }

    #[test]
    fn test_with() -> Result<()> {
        let source: &str = r#"
def f():  # 6
    with a:
        if a:
            print()
        else:
            print()

"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 6);
        Ok(())
    }

    #[test]
    fn test_try_except() -> Result<()> {
        let source: &str = r#"
def f():  # 5
    try:
        print()
    except Exception:
        raise
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 5);
        Ok(())
    }

    #[test]
    fn test_try_except_else() -> Result<()> {
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
        assert_eq!(get_num_statements(&stmts), 7);
        Ok(())
    }

    #[test]
    fn test_try_except_else_finally() -> Result<()> {
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
        assert_eq!(get_num_statements(&stmts), 10);
        Ok(())
    }

    #[test]
    fn test_try_except_except() -> Result<()> {
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
        assert_eq!(get_num_statements(&stmts), 8);
        Ok(())
    }

    #[test]
    fn test_try_except_except_finally() -> Result<()> {
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
        assert_eq!(get_num_statements(&stmts), 11);
        Ok(())
    }

    #[test]
    fn test_yield() -> Result<()> {
        let source: &str = r#"
def f():
    for i in range(10):
        yield i
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 2);
        Ok(())
    }
}
