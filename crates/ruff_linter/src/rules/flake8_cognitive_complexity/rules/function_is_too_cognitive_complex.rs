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
                    complexity += 1 + nesting;

                    complexity += get_cognitive_complexity_number(&clause.body, nesting + 1);
                }
            }
            Stmt::For(ast::StmtFor { body, orelse, .. }) => {
                complexity += 1 + nesting;
                complexity += get_cognitive_complexity_number(body, nesting + 1);
                complexity += get_cognitive_complexity_number(orelse, nesting + 1);
            }
            Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
                complexity += get_cognitive_complexity_number(body, nesting);
            }
            Stmt::ClassDef(ast::StmtClassDef { body, .. }) => {
                complexity += get_cognitive_complexity_number(body, nesting);
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

    #[test]
    fn if_elif_else() -> Result<()> {
        let source = r#"
def if_elif_else(n):
    if n == 3:
        return "three"
    elif n == 4:
        return "four"
    else:
        return "something else"
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_cognitive_complexity_number(&stmts, 0), 3);
        Ok(())
    }

    #[test]
    fn if_elif_elif_else() -> Result<()> {
        let source = r#"
def if_elif_elif_else(n):
    if n == 3:
        return "three"
    elif n == 4:
        return "four"
    elif n == 5:
        return "five"
    else:
        return "something else"
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_cognitive_complexity_number(&stmts, 0), 4);
        Ok(())
    }

    #[test]
    fn for_loop_if_else() -> Result<()> {
        let source = r#"
def for_loop_if_else():
    for i in range(10):
        if i == 7:
            print("seven")
        else:
            print("something else")
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_cognitive_complexity_number(&stmts, 0), 5);
        Ok(())
    }

    #[test]
    fn for_for_if_else() -> Result<()> {
        let source = r#"
def for_for_if_else():
    for i in range(10):
        for j in range(10):
            if i == j:
                print("i = j")
            else:
                print("i != j")
"#;
        let stmts = parse_suite(source)?;
        assert_eq!(get_cognitive_complexity_number(&stmts, 0), 9);
        Ok(())
    }

    #[test]
    fn for_loop() -> Result<()> {
        let source = r"
def for_loop():
    for i in range(10):
        print(i)
";
        let stmts = parse_suite(source)?;
        assert_eq!(get_cognitive_complexity_number(&stmts, 0), 1);
        Ok(())
    }
}
