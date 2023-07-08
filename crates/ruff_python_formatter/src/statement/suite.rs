use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::lines_before;
use ruff_formatter::{
    format_args, write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions,
};
use rustpython_parser::ast::{Ranged, Stmt, Suite};

/// Level at which the [`Suite`] appears in the source code.
#[derive(Copy, Clone, Debug)]
pub enum SuiteLevel {
    /// Statements at the module level / top level
    TopLevel,

    /// Statements in a nested body
    Nested,
}

impl SuiteLevel {
    const fn is_nested(self) -> bool {
        matches!(self, SuiteLevel::Nested)
    }
}

#[derive(Debug)]
pub struct FormatSuite {
    level: SuiteLevel,
}

impl Default for FormatSuite {
    fn default() -> Self {
        FormatSuite {
            level: SuiteLevel::Nested,
        }
    }
}

impl FormatRule<Suite, PyFormatContext<'_>> for FormatSuite {
    fn fmt(&self, statements: &Suite, f: &mut PyFormatter) -> FormatResult<()> {
        let node_level = match self.level {
            SuiteLevel::TopLevel => NodeLevel::TopLevel,
            SuiteLevel::Nested => NodeLevel::CompoundStatement,
        };

        let comments = f.context().comments().clone();
        let source = f.context().contents();

        let saved_level = f.context().node_level();
        f.context_mut().set_node_level(node_level);

        let mut joiner = f.join_nodes(node_level);

        let mut iter = statements.iter();
        let Some(first) = iter.next() else {
            return Ok(());
        };

        // First entry has never any separator, doesn't matter which one we take;
        joiner.entry(first, &first.format());

        let mut last = first;
        let mut is_last_function_or_class_definition = is_class_or_function_definition(first);

        for statement in iter {
            let is_current_function_or_class_definition =
                is_class_or_function_definition(statement);

            if is_last_function_or_class_definition || is_current_function_or_class_definition {
                match self.level {
                    SuiteLevel::TopLevel => {
                        joiner.entry_with_separator(
                            &format_args![empty_line(), empty_line()],
                            &statement.format(),
                            statement,
                        );
                    }
                    SuiteLevel::Nested => {
                        joiner.entry_with_separator(&empty_line(), &statement.format(), statement);
                    }
                }
            } else if is_compound_statement(last) {
                // Handles the case where a body has trailing comments. The issue is that RustPython does not include
                // the comments in the range of the suite. This means, the body ends right after the last statement in the body.
                // ```python
                // def test():
                //      ...
                //      # The body of `test` ends right after `...` and before this comment
                //
                // # leading comment
                //
                //
                // a = 10
                // ```
                // Using `lines_after` for the node doesn't work because it would count the lines after the `...`
                // which is 0 instead of 1, the number of lines between the trailing comment and
                // the leading comment. This is why the suite handling counts the lines before the
                // start of the next statement or before the first leading comments for compound statements.
                let separator = format_with(|f| {
                    let start =
                        if let Some(first_leading) = comments.leading_comments(statement).first() {
                            first_leading.slice().start()
                        } else {
                            statement.start()
                        };

                    match lines_before(start, source) {
                        0 | 1 => hard_line_break().fmt(f),
                        2 => empty_line().fmt(f),
                        3.. => {
                            if self.level.is_nested() {
                                empty_line().fmt(f)
                            } else {
                                write!(f, [empty_line(), empty_line()])
                            }
                        }
                    }
                });

                joiner.entry_with_separator(&separator, &statement.format(), statement);
            } else {
                joiner.entry(statement, &statement.format());
            }

            is_last_function_or_class_definition = is_current_function_or_class_definition;
            last = statement;
        }

        let result = joiner.finish();

        f.context_mut().set_node_level(saved_level);

        result
    }
}

const fn is_class_or_function_definition(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_)
    )
}

const fn is_compound_statement(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::FunctionDef(_)
            | Stmt::AsyncFunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::While(_)
            | Stmt::For(_)
            | Stmt::AsyncFor(_)
            | Stmt::Match(_)
            | Stmt::With(_)
            | Stmt::AsyncWith(_)
            | Stmt::If(_)
            | Stmt::Try(_)
            | Stmt::TryStar(_)
    )
}

impl FormatRuleWithOptions<Suite, PyFormatContext<'_>> for FormatSuite {
    type Options = SuiteLevel;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.level = options;
        self
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Suite {
    type Format<'a> = FormatRefWithRule<'a, Suite, FormatSuite, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatSuite::default())
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Suite {
    type Format = FormatOwnedWithRule<Suite, FormatSuite, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatSuite::default())
    }
}

#[cfg(test)]
mod tests {
    use crate::comments::Comments;
    use crate::prelude::*;
    use crate::statement::suite::SuiteLevel;
    use crate::PyFormatOptions;
    use ruff_formatter::format;
    use rustpython_parser::ast::Suite;
    use rustpython_parser::Parse;

    fn format_suite(level: SuiteLevel) -> String {
        let source = r#"
a = 10



three_leading_newlines = 80


two_leading_newlines = 20

one_leading_newline = 10
no_leading_newline = 30
class InTheMiddle:
    pass
trailing_statement = 1
def func():
    pass
def trailing_func():
    pass
"#;

        let statements = Suite::parse(source, "test.py").unwrap();

        let context = PyFormatContext::new(PyFormatOptions::default(), source, Comments::default());

        let test_formatter =
            format_with(|f: &mut PyFormatter| statements.format().with_options(level).fmt(f));

        let formatted = format!(context, [test_formatter]).unwrap();
        let printed = formatted.print().unwrap();

        printed.as_code().to_string()
    }

    #[test]
    fn top_level() {
        let formatted = format_suite(SuiteLevel::TopLevel);

        assert_eq!(
            formatted,
            r#"a = 10


three_leading_newlines = 80


two_leading_newlines = 20

one_leading_newline = 10
no_leading_newline = 30


class InTheMiddle:
    pass


trailing_statement = 1


def func():
    pass


def trailing_func():
    pass
"#
        );
    }

    #[test]
    fn nested_level() {
        let formatted = format_suite(SuiteLevel::Nested);

        assert_eq!(
            formatted,
            r#"a = 10

three_leading_newlines = 80

two_leading_newlines = 20

one_leading_newline = 10
no_leading_newline = 30

class InTheMiddle:
    pass

trailing_statement = 1

def func():
    pass

def trailing_func():
    pass
"#
        );
    }
}
