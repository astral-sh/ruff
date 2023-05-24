use anyhow::{anyhow, Context, Result};
use rustpython_parser::ast::Mod;
use rustpython_parser::lexer::lex;
use rustpython_parser::{parse_tokens, Mode};

use ruff_formatter::{
    format, FormatResult, Formatted, IndentStyle, Printed, SimpleFormatOptions, SourceCode,
};
use ruff_python_ast::source_code::{CommentRanges, CommentRangesBuilder, Locator};

use crate::attachment::attach;
use crate::comments::Comments;
use crate::context::ASTFormatContext;
use crate::cst::Stmt;
use crate::newlines::normalize_newlines;
use crate::parentheses::normalize_parentheses;
use crate::trivia::TriviaToken;

mod attachment;
pub mod cli;
mod comments;
pub mod context;
mod cst;
mod format;
mod newlines;
mod parentheses;
mod prelude;
mod trivia;

include!("../../ruff_formatter/shared_traits.rs");

pub fn fmt(contents: &str) -> Result<Printed> {
    // Tokenize once
    let mut tokens = Vec::new();
    let mut comment_ranges = CommentRangesBuilder::default();

    for result in lex(contents, Mode::Module) {
        let (token, range) = match result {
            Ok((token, range)) => (token, range),
            Err(err) => return Err(anyhow!("Source contains syntax errors {err:?}")),
        };

        comment_ranges.visit_token(&token, range);
        tokens.push(Ok((token, range)));
    }

    let comment_ranges = comment_ranges.finish();

    let trivia = trivia::extract_trivia_tokens(&tokens);

    // Parse the AST.
    let python_ast = parse_tokens(tokens, Mode::Module, "<filename>").unwrap();

    let formatted = format_node(&python_ast, &comment_ranges, contents, trivia)?;

    formatted
        .print()
        .with_context(|| "Failed to print the formatter IR")
}

pub(crate) fn format_node<'a>(
    root: &'a Mod,
    comment_ranges: &'a CommentRanges,
    source: &'a str,
    trivia: Vec<TriviaToken>,
) -> FormatResult<Formatted<ASTFormatContext<'a>>> {
    let comments = Comments::from_ast(root, SourceCode::new(source), comment_ranges);

    let module = root.as_module().unwrap();

    let locator = Locator::new(source);

    // Convert to a CST.
    let mut python_cst: Vec<Stmt> = module
        .body
        .iter()
        .cloned()
        .map(|stmt| (stmt, &locator).into())
        .collect();

    // Attach trivia.
    attach(&mut python_cst, trivia);
    normalize_newlines(&mut python_cst);
    normalize_parentheses(&mut python_cst, &locator);

    format!(
        ASTFormatContext::new(
            SimpleFormatOptions {
                indent_style: IndentStyle::Space(4),
                line_width: 88.try_into().unwrap(),
            },
            locator.contents(),
            comments
        ),
        [format::builders::statements(&python_cst)]
    )
}

#[cfg(test)]
mod tests {
    use std::fmt::{Formatter, Write};
    use std::fs;
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_snapshot;
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};

    use ruff_python_ast::source_code::CommentRangesBuilder;
    use ruff_testing_macros::fixture;
    use similar::TextDiff;

    use crate::{fmt, format_node, trivia};

    #[fixture(
        pattern = "resources/test/fixtures/black/**/*.py",
        // Excluded tests because they reach unreachable when attaching tokens
        exclude = [
            "*comments8.py",
        ])
    ]
    #[test]
    fn black_test(input_path: &Path) -> Result<()> {
        let content = fs::read_to_string(input_path)?;

        let printed = fmt(&content)?;

        let expected_path = input_path.with_extension("py.expect");
        let expected_output = fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("Expected Black output file '{expected_path:?}' to exist"));

        let formatted_code = printed.as_code();

        if formatted_code == expected_output {
            // Black and Ruff formatting matches. Delete any existing snapshot files because the Black output
            // already perfectly captures the expected output.
            // The following code mimics insta's logic generating the snapshot name for a test.
            let workspace_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let snapshot_name = insta::_function_name!()
                .strip_prefix(&format!("{}::", module_path!()))
                .unwrap();
            let module_path = module_path!().replace("::", "__");

            let snapshot_path = Path::new(&workspace_path)
                .join("src/snapshots")
                .join(format!(
                    "{module_path}__{}.snap",
                    snapshot_name.replace(&['/', '\\'][..], "__")
                ));

            if snapshot_path.exists() && snapshot_path.is_file() {
                // SAFETY: This is a convenience feature. That's why we don't want to abort
                // when deleting a no longer needed snapshot fails.
                fs::remove_file(&snapshot_path).ok();
            }

            let new_snapshot_path = snapshot_path.with_extension("snap.new");
            if new_snapshot_path.exists() && new_snapshot_path.is_file() {
                // SAFETY: This is a convenience feature. That's why we don't want to abort
                // when deleting a no longer needed snapshot fails.
                fs::remove_file(&new_snapshot_path).ok();
            }
        } else {
            // Black and Ruff have different formatting. Write out a snapshot that covers the differences
            // today.
            let mut snapshot = String::new();
            write!(snapshot, "{}", Header::new("Input"))?;
            write!(snapshot, "{}", CodeFrame::new("py", &content))?;

            write!(snapshot, "{}", Header::new("Black Differences"))?;

            let diff = TextDiff::from_lines(expected_output.as_str(), formatted_code)
                .unified_diff()
                .header("Black", "Ruff")
                .to_string();

            write!(snapshot, "{}", CodeFrame::new("diff", &diff))?;

            write!(snapshot, "{}", Header::new("Ruff Output"))?;
            write!(snapshot, "{}", CodeFrame::new("py", formatted_code))?;

            write!(snapshot, "{}", Header::new("Black Output"))?;
            write!(snapshot, "{}", CodeFrame::new("py", &expected_output))?;

            insta::with_settings!({ omit_expression => false, input_file => input_path }, {
                insta::assert_snapshot!(snapshot);
            });
        }

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
        // Tokenize once
        let mut tokens = Vec::new();
        let mut comment_ranges = CommentRangesBuilder::default();

        for result in lex(src, Mode::Module) {
            let (token, range) = result.unwrap();
            comment_ranges.visit_token(&token, range);
            tokens.push(Ok((token, range)));
        }

        let comment_ranges = comment_ranges.finish();

        let trivia = trivia::extract_trivia_tokens(&tokens);

        // Parse the AST.
        let python_ast = parse_tokens(tokens, Mode::Module, "<filename>").unwrap();

        let formatted = format_node(&python_ast, &comment_ranges, src, trivia).unwrap();

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

    #[test]
    fn string_processing() {
        use crate::prelude::*;
        use ruff_formatter::{format, format_args, write};

        struct FormatString<'a>(&'a str);

        impl Format<SimpleFormatContext> for FormatString<'_> {
            fn fmt(
                &self,
                f: &mut ruff_formatter::formatter::Formatter<SimpleFormatContext>,
            ) -> FormatResult<()> {
                let format_str = format_with(|f| {
                    write!(f, [text("\"")])?;

                    let mut words = self.0.split_whitespace().peekable();
                    let mut fill = f.fill();

                    let separator = format_with(|f| {
                        group(&format_args![
                            if_group_breaks(&text("\"")),
                            soft_line_break_or_space(),
                            if_group_breaks(&text("\" "))
                        ])
                        .fmt(f)
                    });

                    while let Some(word) = words.next() {
                        let is_last = words.peek().is_none();
                        let format_word = format_with(|f| {
                            write!(f, [dynamic_text(word, None)])?;

                            if is_last {
                                write!(f, [text("\"")])?;
                            }

                            Ok(())
                        });

                        fill.entry(&separator, &format_word);
                    }

                    fill.finish()
                });

                write!(
                    f,
                    [group(&format_args![
                        if_group_breaks(&text("(")),
                        soft_block_indent(&format_str),
                        if_group_breaks(&text(")"))
                    ])]
                )
            }
        }

        // 77 after g group (leading quote)
        let fits =
            r#"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg h"#;
        let breaks =
            r#"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg hh"#;

        let output = format!(
            SimpleFormatContext::default(),
            [FormatString(fits), hard_line_break(), FormatString(breaks)]
        )
        .expect("Formatting to succeed");

        assert_snapshot!(output.print().expect("Printing to succeed").as_code());
    }

    struct Header<'a> {
        title: &'a str,
    }

    impl<'a> Header<'a> {
        fn new(title: &'a str) -> Self {
            Self { title }
        }
    }

    impl std::fmt::Display for Header<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "## {}", self.title)?;
            writeln!(f)
        }
    }

    struct CodeFrame<'a> {
        language: &'a str,
        code: &'a str,
    }

    impl<'a> CodeFrame<'a> {
        fn new(language: &'a str, code: &'a str) -> Self {
            Self { language, code }
        }
    }

    impl std::fmt::Display for CodeFrame<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "```{}", self.language)?;
            write!(f, "{}", self.code)?;
            writeln!(f, "```")?;
            writeln!(f)
        }
    }
}
