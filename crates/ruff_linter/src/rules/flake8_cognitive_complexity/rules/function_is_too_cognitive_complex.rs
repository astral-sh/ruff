use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, identifier::Identifier, Stmt};

#[violation]
pub struct CognitiveComplexStructure {
    name: String,
    complexity: usize,
    max_cognitive_complexity: usize,
}

impl Violation for CognitiveComplexStructure {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CognitiveComplexStructure {
            name,
            complexity,
            max_cognitive_complexity,
        } = self;
        format!("`{name}` is too cognitive complex ({complexity} > {max_cognitive_complexity})")
    }
}

fn get_cognitive_complexity_number(stmts: &[Stmt], nesting: usize) -> usize {
    let mut complexity: usize = 0;
    for stmt in stmts {
        match stmt {
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                complexity += 1 + nesting;
                complexity += get_cognitive_complexity_number(body, nesting + 1);
                for clause in elif_else_clauses {
                    if clause.test.is_some() {
                        complexity += 1;
                    }
                    complexity += get_cognitive_complexity_number(&clause.body, nesting + 1);
                }
            }
            _ => {}
        }
    }
    complexity
}

pub(crate) fn function_is_too_cognitive_complex(
    stmt: &Stmt,
    name: &str,
    body: &[Stmt],
    max_cognitive_complexity: usize,
) -> Option<Diagnostic> {
    let complexity = get_cognitive_complexity_number(body, 1);
    if complexity > max_cognitive_complexity {
        Some(Diagnostic::new(
            CognitiveComplexStructure {
                name: name.to_string(),
                complexity,
                max_cognitive_complexity,
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

    use super::get_cognitive_complexity_number;

    #[test]
    fn trivial() -> Result<()> {
        let source = r"
def trivial():
    pass
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_cognitive_complexity_number(&stmts, 0), 0);
        Ok(())
    }
}
