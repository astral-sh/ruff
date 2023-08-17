use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;

/// ## What it does
/// Checks for functions or methods with too many branches.
///
/// By default, this rule allows up to 12 branches. This can be configured
/// using the [`pylint.max-branches`] option.
///
/// ## Why is this bad?
/// Functions or methods with many branches are harder to understand
/// and maintain than functions or methods with fewer branches.
///
/// ## Example
/// ```python
/// def capital(country):
///     if country == "Australia":
///         return "Canberra"
///     elif country == "Brazil":
///         return "Brasilia"
///     elif country == "Canada":
///         return "Ottawa"
///     elif country == "England":
///         return "London"
///     elif country == "France":
///         return "Paris"
///     elif country == "Germany":
///         return "Berlin"
///     elif country == "Poland":
///         return "Warsaw"
///     elif country == "Romania":
///         return "Bucharest"
///     elif country == "Spain":
///         return "Madrid"
///     elif country == "Thailand":
///         return "Bangkok"
///     elif country == "Turkey":
///         return "Ankara"
///     elif country == "United States":
///         return "Washington"
///     else:
///         return "Unknown"  # 13th branch
/// ```
///
/// Use instead:
/// ```python
/// def capital(country):
///     capitals = {
///         "Australia": "Canberra",
///         "Brazil": "Brasilia",
///         "Canada": "Ottawa",
///         "England": "London",
///         "France": "Paris",
///         "Germany": "Berlin",
///         "Poland": "Warsaw",
///         "Romania": "Bucharest",
///         "Spain": "Madrid",
///         "Thailand": "Bangkok",
///         "Turkey": "Ankara",
///         "United States": "Washington",
///     }
///     city = capitals.get(country, "Unknown")
///     return city
/// ```
///
/// ## Options
/// - `pylint.max-branches`
#[violation]
pub struct TooManyBranches {
    branches: usize,
    max_branches: usize,
}

impl Violation for TooManyBranches {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBranches {
            branches,
            max_branches,
        } = self;
        format!("Too many branches ({branches} > {max_branches})")
    }
}

fn num_branches(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                1 + num_branches(body)
                    + elif_else_clauses.len()
                    + elif_else_clauses
                        .iter()
                        .map(|clause| num_branches(&clause.body))
                        .sum::<usize>()
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                1 + cases
                    .iter()
                    .map(|case| num_branches(&case.body))
                    .sum::<usize>()
            }
            Stmt::For(ast::StmtFor { body, orelse, .. })
            | Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                1 + num_branches(body)
                    + (if orelse.is_empty() {
                        0
                    } else {
                        1 + num_branches(orelse)
                    })
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
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
                                let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                                    body,
                                    ..
                                }) = handler;
                                num_branches(body)
                            }
                        })
                        .sum::<usize>()
            }
            _ => 0,
        })
        .sum()
}

/// PLR0912
pub(crate) fn too_many_branches(
    stmt: &Stmt,
    body: &[Stmt],
    max_branches: usize,
) -> Option<Diagnostic> {
    let branches = num_branches(body);
    if branches > max_branches {
        Some(Diagnostic::new(
            TooManyBranches {
                branches,
                max_branches,
            },
            stmt.identifier(),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_python_parser::parse_suite;

    use super::num_branches;

    fn test_helper(source: &str, expected_num_branches: usize) -> Result<()> {
        let branches = parse_suite(source, "<filename>")?;
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
        test_helper(source, 4)?;
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
