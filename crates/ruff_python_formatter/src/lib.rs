use anyhow::Result;
use ruff_formatter::{format, Formatted, IndentStyle, SimpleFormatOptions};
use rustpython_parser::lexer::LexResult;

use crate::attachment::attach;
use crate::context::ASTFormatContext;
use crate::core::locator::Locator;
use crate::core::rustpython_helpers;
use crate::cst::Stmt;
use crate::newlines::normalize_newlines;
use crate::parentheses::normalize_parentheses;

mod attachment;
pub mod builders;
pub mod cli;
pub mod context;
mod core;
mod cst;
mod format;
mod newlines;
mod parentheses;
pub mod shared_traits;
#[cfg(test)]
mod test;
pub mod trivia;

pub fn fmt(contents: &str) -> Result<Formatted<ASTFormatContext>> {
    // Tokenize once.
    let tokens: Vec<LexResult> = rustpython_helpers::tokenize(contents);

    // Extract trivia.
    let trivia = trivia::extract_trivia_tokens(&tokens);

    // Parse the AST.
    let python_ast = rustpython_helpers::parse_program_tokens(tokens, "<filename>")?;

    // Convert to a CST.
    let mut python_cst: Vec<Stmt> = python_ast.into_iter().map(Into::into).collect();

    // Attach trivia.
    attach(&mut python_cst, trivia);
    normalize_newlines(&mut python_cst);
    normalize_parentheses(&mut python_cst);

    format!(
        ASTFormatContext::new(
            SimpleFormatOptions {
                indent_style: IndentStyle::Space(4),
                line_width: 88.try_into().unwrap(),
            },
            Locator::new(contents)
        ),
        [format::builders::block(&python_cst)]
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::fmt;
    use crate::test::test_resource_path;

    #[test_case(Path::new("simple_cases/beginning_backslash.py"); "beginning_backslash")]
    #[test_case(Path::new("simple_cases/class_blank_parentheses.py"); "class_blank_parentheses")]
    #[test_case(Path::new("simple_cases/class_methods_new_line.py"); "class_methods_new_line")]
    #[test_case(Path::new("simple_cases/import_spacing.py"); "import_spacing")]
    #[test_case(Path::new("simple_cases/one_element_subscript.py"); "one_element_subscript")]
    #[test_case(Path::new("simple_cases/power_op_spacing.py"); "power_op_spacing")]
    #[test_case(Path::new("simple_cases/remove_newline_after_code_block_open.py"); "remove_newline_after_code_block_open")]
    #[test_case(Path::new("simple_cases/slices.py"); "slices")]
    #[test_case(Path::new("simple_cases/tricky_unicode_symbols.py"); "tricky_unicode_symbols")]
    // Passing except that `1, 2, 3,` should be `(1, 2, 3)`.
    #[test_case(Path::new("simple_cases/tupleassign.py"); "tupleassign")]
    fn passing(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.display());
        let content = std::fs::read_to_string(test_resource_path(
            Path::new("fixtures/black").join(path).as_path(),
        ))?;
        let formatted = fmt(&content)?;
        insta::assert_display_snapshot!(snapshot, formatted.print()?.as_code());
        Ok(())
    }

    #[test_case(Path::new("simple_cases/collections.py"); "collections")]
    #[test_case(Path::new("simple_cases/bracketmatch.py"); "bracketmatch")]
    fn passing_modulo_string_normalization(path: &Path) -> Result<()> {
        fn adjust_quotes(contents: &str) -> String {
            // Replace all single quotes with double quotes.
            contents.replace('\'', "\"")
        }

        let snapshot = format!("{}", path.display());
        let content = std::fs::read_to_string(test_resource_path(
            Path::new("fixtures/black").join(path).as_path(),
        ))?;
        let formatted = fmt(&content)?;
        insta::assert_display_snapshot!(snapshot, adjust_quotes(formatted.print()?.as_code()));
        Ok(())
    }

    #[ignore]
    // Lots of deviations, _mostly_ related to string normalization and wrapping.
    #[test_case(Path::new("simple_cases/expression.py"); "expression")]
    // Passing apart from a trailing end-of-line comment within an if statement, which is being
    // inappropriately associated with the if statement rather than the line it's on.
    #[test_case(Path::new("simple_cases/comments.py"); "comments")]
    #[test_case(Path::new("simple_cases/function.py"); "function")]
    #[test_case(Path::new("simple_cases/function2.py"); "function2")]
    #[test_case(Path::new("simple_cases/function_trailing_comma.py"); "function_trailing_comma")]
    #[test_case(Path::new("simple_cases/composition.py"); "composition")]
    fn failing(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.display());
        let content = std::fs::read_to_string(test_resource_path(
            Path::new("fixtures/black").join(path).as_path(),
        ))?;
        let formatted = fmt(&content)?;
        insta::assert_display_snapshot!(snapshot, formatted.print()?.as_code());
        Ok(())
    }

    /// Use this test to debug the formatting of some snipped
    #[ignore]
    #[test]
    fn quick_test() {
        let src = r#"
{
    k: v for k, v in a_very_long_variable_name_that_exceeds_the_line_length_by_far_keep_going
}
"#;
        let formatted = fmt(src).unwrap();

        // Uncomment the `dbg` to print the IR.
        // Use `dbg_write!(f, []) instead of `write!(f, [])` in your formatting code to print some IR
        // inside of a `Format` implementation
        // dbg!(formatted.document());

        let printed = formatted.print().unwrap();

        assert_eq!(
            printed.as_code(),
            r#"{
    k: v
    for k, v in a_very_long_variable_name_that_exceeds_the_line_length_by_far_keep_going
}"#
        );
    }
}
