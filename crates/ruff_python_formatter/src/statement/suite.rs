use crate::context::NodeLevel;
use crate::prelude::*;
use ruff_formatter::{format_args, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use rustpython_parser::ast::{Stmt, Suite};

/// Level at which the [`Suite`] appears in the source code.
#[derive(Copy, Clone, Debug)]
pub enum SuiteLevel {
    /// Statements at the module level / top level
    TopLevel,

    /// Statements in a nested body
    Nested,
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
        let mut joiner = f.join_nodes(match self.level {
            SuiteLevel::TopLevel => NodeLevel::TopLevel,
            SuiteLevel::Nested => NodeLevel::Statement,
        });

        let mut iter = statements.iter();
        let Some(first) = iter.next() else {
            return Ok(())
        };

        // First entry has never any separator, doesn't matter which one we take;
        joiner.entry(first, &first.format());

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
                        );
                    }
                    SuiteLevel::Nested => {
                        joiner
                            .entry_with_separator(&format_args![empty_line()], &statement.format());
                    }
                }
            } else {
                joiner.entry(statement, &statement.format());
            }

            is_last_function_or_class_definition = is_current_function_or_class_definition;
        }

        joiner.finish()
    }
}

const fn is_class_or_function_definition(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_)
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
    use ruff_formatter::{format, SimpleFormatOptions};
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

        let context =
            PyFormatContext::new(SimpleFormatOptions::default(), source, Comments::default());

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
    pass"#
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
    pass"#
        );
    }
}
