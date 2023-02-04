use crate::ast::helpers::identifier_range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

use ruff_macros::derive_message_formats;
use rustpython_ast::{ExcepthandlerKind, Stmt, StmtKind};

define_violation!(
    pub struct TooManyReturnStatements {
        pub returns: usize,
        pub max_returns: usize,
    }
);
impl Violation for TooManyReturnStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyReturnStatements {
            returns,
            max_returns,
        } = self;
        format!("Too many return statements ({returns}/{max_returns})")
    }
}

fn num_returns(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| match &stmt.node {
            StmtKind::If { body, orelse, .. }
            | StmtKind::For { body, orelse, .. }
            | StmtKind::AsyncFor { body, orelse, .. }
            | StmtKind::While { body, orelse, .. } => num_returns(body) + num_returns(orelse),
            StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => num_returns(body),
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                num_returns(body)
                    + num_returns(orelse)
                    + num_returns(finalbody)
                    + handlers
                        .iter()
                        .map(|handler| {
                            let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
                            num_returns(body)
                        })
                        .sum::<usize>()
            }
            StmtKind::Match { .. } => {
                // TODO: Uncomment when pattern matching is available, is in rustpython so have to match
                // Add the 'cases' field when it is
                /*
                cases.iter().map(|case|
                    num_returns(&case.body)
                ).sum::<usize>()
                */
                unimplemented!("Match case is unimplemented")
            }
            StmtKind::Return { .. } => 1,
            StmtKind::ImportFrom { .. }
            | StmtKind::Import { .. }
            | StmtKind::Global { .. }
            | StmtKind::Nonlocal { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. }
            | StmtKind::Assert { .. }
            | StmtKind::Raise { .. }
            | StmtKind::Assign { .. }
            | StmtKind::AugAssign { .. }
            | StmtKind::AnnAssign { .. }
            | StmtKind::Delete { .. }
            | StmtKind::Expr { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass => 0,
        })
        .sum()
}

/// PLR0911
pub fn too_many_return_statements(
    stmt: &Stmt,
    body: &[Stmt],
    max_returns: usize,
    locator: &Locator,
) -> Option<Diagnostic> {
    let returns = num_returns(body);
    if returns > max_returns {
        Some(Diagnostic::new(
            TooManyReturnStatements {
                returns,
                max_returns,
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

    use super::num_returns;

    fn test_helper(source: &str, expected: usize) -> Result<()> {
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_returns(&stmts), expected);
        Ok(())
    }

    #[test]
    fn if_() -> Result<()> {
        let source = r#"
x = 1
if x == 1:  # 9
    return
if x == 2:
    return
if x == 3:
    return
if x == 4:
    return
if x == 5:
    return
if x == 6:
    return
if x == 7:
    return
if x == 8:
    return
if x == 9:
    return            
"#;

        test_helper(source, 9)?;
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source = r#"
for _i in range(10):
    return
else:
    return
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn async_for_else() -> Result<()> {
        let source = r#"
async for _i in range(10):
    return
else:
    return
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn nested_def_ignored() -> Result<()> {
        let source = r#"
def f():
    return

x = 1
if x == 1:
    print()
else:
    print()
"#;

        test_helper(source, 0)?;
        Ok(())
    }

    #[test]
    fn while_nested_if() -> Result<()> {
        let source = r#"
x = 1
while x < 10:
    print()
    if x == 3:
        return
    x += 1
return
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn with_if() -> Result<()> {
        let source = r#"
with a as f:
    return
    if f == 1:
        return
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn async_with_if() -> Result<()> {
        let source = r#"
async with a as f:
    return
    if f == 1:
        return
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn try_except_except_else_finally() -> Result<()> {
        let source = r#"
try:
    print()
    return
except ValueError:
    return
except Exception:
    return
else:
    return
finally:
    return
"#;

        test_helper(source, 5)?;
        Ok(())
    }

    #[test]
    fn class_def_ignored() -> Result<()> {
        let source = r#"
class A:
    def f(self):
        return

    def g(self):
        return
"#;

        test_helper(source, 0)?;
        Ok(())
    }
}
