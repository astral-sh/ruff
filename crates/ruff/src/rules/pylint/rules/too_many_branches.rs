use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExcepthandlerKind, Stmt, StmtKind};

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct TooManyBranches {
        pub branches: usize,
        pub max_branches: usize,
    }
);

impl Violation for TooManyBranches {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBranches {
            branches,
            max_branches,
        } = self;
        format!("Too many branches ({branches}/{max_branches})")
    }
}

fn num_branches(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| {
            // TODO(charlie): Account for pattern match statement.
            match &stmt.node {
                StmtKind::If { body, orelse, .. } => {
                    1 + num_branches(body)
                        + (if let Some(stmt) = orelse.first() {
                            // `elif:` and `else: if:` have the same AST representation.
                            // Avoid treating `elif:` as two statements.
                            usize::from(!matches!(stmt.node, StmtKind::If { .. }))
                        } else {
                            0
                        })
                        + num_branches(orelse)
                }
                StmtKind::For { body, orelse, .. }
                | StmtKind::AsyncFor { body, orelse, .. }
                | StmtKind::While { body, orelse, .. } => {
                    1 + num_branches(body)
                        + (if orelse.is_empty() {
                            0
                        } else {
                            1 + num_branches(orelse)
                        })
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    1 + num_branches(body)
                        + (if orelse.is_empty() {
                            0
                        } else {
                            1 + num_branches(orelse)
                        })
                        + (if finalbody.is_empty() {
                            0
                        } else {
                            1 + num_branches(finalbody)
                        })
                        + handlers
                            .iter()
                            .map(|handler| {
                                1 + {
                                    let ExcepthandlerKind::ExceptHandler { body, .. } =
                                        &handler.node;
                                    num_branches(body)
                                }
                            })
                            .sum::<usize>()
                }
                _ => 0,
            }
        })
        .sum()
}

/// PLR0912
pub fn too_many_branches(
    stmt: &Stmt,
    body: &[Stmt],
    max_branches: usize,
    locator: &Locator,
) -> Option<Diagnostic> {
    let branches = num_branches(body);
    if branches > max_branches {
        Some(Diagnostic::new(
            TooManyBranches {
                branches,
                max_branches,
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

    use super::num_branches;

    fn test_helper(source: &str, expected_num_branches: usize) -> Result<()> {
        let branches = parser::parse_program(source, "<filename>")?;
        assert_eq!(num_branches(&branches), expected_num_branches);
        Ok(())
    }

    #[test]
    fn if_else_nested_if_else() -> Result<()> {
        let source: &str = r#"
if x == 0:  # 3
    return
else:
    if x == 1:
        pass
    else:
        pass
"#;

        test_helper(source, 3)?;
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source: &str = r#"
for _ in range(x):  # 2
    pass
else:
    pass
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn while_if_else_if() -> Result<()> {
        let source: &str = r#"
while x < 1:  # 4
    if x:
        pass
else:
    if x:
        pass
"#;

        test_helper(source, 4)?;
        Ok(())
    }

    #[test]
    fn nested_def() -> Result<()> {
        let source: &str = r#"
if x:  # 2
    pass
else:
    pass

def g(x):
    if x:
        pass

return 1
"#;

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn try_except_except_else_finally() -> Result<()> {
        let source: &str = r#"
try:
    pass
except:
    pass
except:
    pass
else:
    pass
finally:
    pass
"#;

        test_helper(source, 5)?;
        Ok(())
    }
}
