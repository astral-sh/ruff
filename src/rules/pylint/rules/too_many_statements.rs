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
        format!("Too many arguments to function call ({statements}/{max_statements})")
    }
}

fn get_num_statements(stmts: &[Stmt]) -> usize {
    let mut count: usize = 0;
    for stmt in stmts {
        match &stmt.node {
            StmtKind::If { body, orelse, .. } => {
                count += 1;
                count += get_num_statements(body);
                if orelse.len() > 0 {
                    count += 1 + get_num_statements(orelse);
                }
            }
            StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
                count += 1;
                count += get_num_statements(body);
                count += get_num_statements(orelse);
            }
            StmtKind::While { test: _, body, orelse } => {
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
                count += get_num_statements(orelse);
                count += get_num_statements(finalbody);
                println!("{} {} {}", body.len(), orelse.len(), finalbody.len());
                for handler in handlers {
                    count += 1;
                    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
                    count += get_num_statements(body);
                }
            }
            StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } | StmtKind::With { body, .. } => {
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
    a = 1
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
    fn test_if_elif_else() -> Result<()> {
        let source: &str = r#"
def f():
    a = 1
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
        assert_eq!(get_num_statements(&stmts), 12); // counter-intuitive, but elif counts as 2 statements according to pylint itself
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
        assert_eq!(get_num_statements(&stmts), 18);
        Ok(())
    }

    #[test]
    fn with_elses() -> Result<()> {
        let source: &str = r#"
def f():
    a = 1
    for i in range(10):
        print(i)
        if a == 1:
            break
    else:
        print("broke")

    return
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 6);
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
    fn test_with_class_def() -> Result<()> {
        let source: &str = r#"
def f():
    class A:
        def __init__(self):
            pass

        def f():
            pass

    print()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_num_statements(&stmts), 3);
        Ok(())
    }

    #[test]
    fn test_with_exception() -> Result<()> {
        let source: &str = r#"
def f():
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
}
