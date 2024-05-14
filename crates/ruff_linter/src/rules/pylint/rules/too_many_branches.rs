use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;

/// ## What it does
/// Checks for functions or methods with too many branches,
/// including (nested) `if`, `elif`, `else`, `for` loops, 
/// `try`/`except` clauses and `match`/`case` statements.
///
/// By default, this rule allows up to 12 branches. This can be configured
/// using the [`lint.pylint.max-branches`] option.
///
/// ## Why is this bad?
/// Functions or methods with many branches are harder to understand
/// and maintain than functions or methods with fewer branches.
///
/// ## Example
/// ```python
/// def grades_to_average_number(grades):
///     numbers = []
///     for grade in grades:  # 1st branch
///         if len(grade) not in {1, 2}:
///             raise ValueError(f"Invalid grade: {grade}")
///
///         if len(grade) == 2 and grade[1] not in {"+", "-"}:
///             raise ValueError(f"Invalid grade: {grade}")
///
///         letter = grade[0]
///
///         if letter in {"F", "E"}:
///             number = 0.0
///         elif letter == "D":
///             number = 1.0
///         elif letter == "C":
///             number = 2.0
///         elif letter == "B":
///             number = 3.0
///         elif letter == "A":
///             number = 4.0
///         else:
///             raise ValueError(f"Invalid grade: {grade}")
///
///         modifier = 0.0
///         if letter != "F" and grade[-1] == "+":
///             modifier = 0.3
///         elif letter != "F" and grade[-1] == "-":
///             modifier = -0.3
///
///         numbers.append(max(0.0, min(number + modifier, 4.0)))
///
///     try:
///         return sum(numbers) / len(numbers)
///     except ZeroDivisionError:  # 13th branch
///         return 0
/// ```
///
/// Use instead:
/// ```python
/// def grades_to_average_number(grades):
///     grade_values = {"F": 0.0, "E": 0.0, "D": 1.0, "C": 2.0, "B": 3.0, "A": 4.0}
///     modifier_values = {"+": 0.3, "-": -0.3}
///
///     numbers = []
///     for grade in grades:
///         if len(grade) not in {1, 2}:
///             raise ValueError(f"Invalid grade: {grade}")
///
///         letter = grade[0]
///         if letter not in grade_values:
///             raise ValueError(f"Invalid grade: {grade}")
///         number = grade_values[letter]
///
///         if len(grade) == 2 and grade[1] not in modifier_values:
///             raise ValueError(f"Invalid grade: {grade}")
///         modifier = modifier_values.get(grade[-1], 0.0)
/// 
///         if letter == "F":
///             numbers.append(0.0)
///         else:
///             numbers.append(max(0.0, min(number + modifier, 4.0)))
/// 
///     try:
///         return sum(numbers) / len(numbers)
///     except ZeroDivisionError:
///         return 0
/// ```
///
/// ## Options
/// - `lint.pylint.max-branches`
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
            Stmt::With(ast::StmtWith { body, .. }) => num_branches(body), // The `with` statement
            // is not considered a branch but the statements inside the `with` should be counted
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
        let branches = parse_suite(source)?;
        assert_eq!(num_branches(&branches), expected_num_branches);
        Ok(())
    }

    #[test]
    fn if_else_nested_if_else() -> Result<()> {
        let source: &str = r"
if x == 0:  # 3
    return
else:
    if x == 1:
        pass
    else:
        pass
";
        test_helper(source, 4)?;
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source: &str = r"
for _ in range(x):  # 2
    pass
else:
    pass
";

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn while_if_else_if() -> Result<()> {
        let source: &str = r"
while x < 1:  # 4
    if x:
        pass
else:
    if x:
        pass
";

        test_helper(source, 4)?;
        Ok(())
    }

    #[test]
    fn nested_def() -> Result<()> {
        let source: &str = r"
if x:  # 2
    pass
else:
    pass

def g(x):
    if x:
        pass

return 1
";

        test_helper(source, 2)?;
        Ok(())
    }

    #[test]
    fn try_except_except_else_finally() -> Result<()> {
        let source: &str = r"
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
";

        test_helper(source, 5)?;
        Ok(())
    }

    #[test]
    fn with_statement() -> Result<()> {
        let source: &str = r"
with suppress(Exception):
    if x == 0:  # 2
        return
    else:
        return
";

        test_helper(source, 2)?;
        Ok(())
    }
}
