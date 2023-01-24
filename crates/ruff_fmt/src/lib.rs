use anyhow::Result;
use rome_formatter::{format, IndentStyle, Printed, SimpleFormatOptions};
use rustpython_parser::lexer::LexResult;

use crate::attachment::attach;
use crate::context::ASTFormatContext;
use crate::core::locator::Locator;
use crate::core::rustpython_helpers;
use crate::cst::Stmt;
use crate::newlines::normalize_newlines;

mod attachment;
pub mod builders;
pub mod cli;
pub mod context;
mod core;
mod cst;
mod format;
mod newlines;
pub mod shared_traits;
#[cfg(test)]
mod test;
pub mod trivia;

pub fn fmt(contents: &str) -> Result<Printed> {
    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(contents);

    // Extract trivia.
    let trivia = trivia::extract_trivia_tokens(&tokens);

    // Parse the AST.
    let python_ast = rustpython_helpers::parse_program_tokens(tokens, "<filename>")?;

    // Convert to a CST.
    let mut python_cst: Vec<Stmt> = python_ast.into_iter().map(Into::into).collect::<Vec<_>>();

    // Attach trivia.
    attach(&mut python_cst, trivia);
    normalize_newlines(&mut python_cst);

    let elements = format!(
        ASTFormatContext::new(
            SimpleFormatOptions {
                indent_style: IndentStyle::Space(4),
                line_width: 88.try_into().unwrap(),
            },
            Locator::new(contents)
        ),
        [format::builders::block(&python_cst)]
    )?;
    elements.print().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::fmt;
    use crate::test::test_resource_path;

    #[test_case(Path::new("simple_cases/tupleassign.py"); "tupleassign")]
    #[test_case(Path::new("simple_cases/class_blank_parentheses.py"); "class_blank_parentheses")]
    #[test_case(Path::new("simple_cases/class_methods_new_line.py"); "class_methods_new_line")]
    #[test_case(Path::new("simple_cases/beginning_backslash.py"); "beginning_backslash")]
    fn passing(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.display());
        let content = std::fs::read_to_string(test_resource_path(
            Path::new("fixtures/black").join(path).as_path(),
        ))?;
        let printed = fmt(&content)?;
        insta::assert_display_snapshot!(snapshot, printed.as_code());
        Ok(())
    }

    // #[test_case(Path::new("simple_cases/comments.py"); "comments")]
    // #[test_case(Path::new("simple_cases/function.py"); "function")]
    // #[test_case(Path::new("simple_cases/empty_lines.py"); "empty_lines")]
    // #[test_case(Path::new("simple_cases/expression.py"); "expression")]
    fn failing(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.display());
        let content = std::fs::read_to_string(test_resource_path(
            Path::new("fixtures/black").join(path).as_path(),
        ))?;
        let printed = fmt(&content)?;
        insta::assert_display_snapshot!(snapshot, printed.as_code());
        Ok(())
    }
}
