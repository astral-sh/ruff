use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::{identifier_range, ReturnStatementVisitor};
use crate::ast::visitor::Visitor;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

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

/// Count the number of return statements in a function or method body.
fn num_returns(body: &[Stmt]) -> usize {
    let mut visitor = ReturnStatementVisitor::default();
    visitor.visit_body(body);
    visitor.returns.len()
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
