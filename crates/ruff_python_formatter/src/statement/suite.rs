use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::helpers::is_compound_statement;
use ruff_python_ast::str::is_implicit_concatenation;
use ruff_python_ast::{self as ast, Expr, Ranged, Stmt, Suite};
use ruff_python_ast::{Constant, ExprConstant};
use ruff_python_trivia::{lines_after_ignoring_trivia, lines_before};
use ruff_source_file::Locator;

use crate::comments::{leading_comments, trailing_comments};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::expr_constant::ExprConstantLayout;
use crate::expression::string::StringLayout;
use crate::prelude::*;

/// Level at which the [`Suite`] appears in the source code.
#[derive(Copy, Clone, Debug)]
pub enum SuiteKind {
    /// Statements at the module level / top level
    TopLevel,

    /// Statements in a function body.
    Function,

    /// Statements in a class body.
    Class,

    /// Statements in any other body (e.g., `if` or `while`).
    Other,
}

#[derive(Debug)]
pub struct FormatSuite {
    kind: SuiteKind,
}

impl Default for FormatSuite {
    fn default() -> Self {
        FormatSuite {
            kind: SuiteKind::Other,
        }
    }
}

impl FormatRule<Suite, PyFormatContext<'_>> for FormatSuite {
    fn fmt(&self, statements: &Suite, f: &mut PyFormatter) -> FormatResult<()> {
        let node_level = match self.kind {
            SuiteKind::TopLevel => NodeLevel::TopLevel,
            SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                NodeLevel::CompoundStatement
            }
        };

        let comments = f.context().comments().clone();
        let source = f.context().source();

        let mut iter = statements.iter();
        let Some(first) = iter.next() else {
            return Ok(());
        };

        let mut f = WithNodeLevel::new(node_level, f);

        // Format the first statement in the body, which often has special formatting rules.
        let mut last = first;
        match self.kind {
            SuiteKind::Other => {
                if is_class_or_function_definition(first) && !comments.has_leading_comments(first) {
                    // Add an empty line for any nested functions or classes defined within
                    // non-function or class compound statements, e.g., this is stable formatting:
                    // ```python
                    // if True:
                    //
                    //     def test():
                    //         ...
                    // ```
                    write!(f, [empty_line()])?;
                }
                write!(f, [first.format()])?;
            }
            SuiteKind::Function => {
                if let Some(constant) = get_docstring(first, &f.context().locator()) {
                    write!(
                        f,
                        [
                            leading_comments(comments.leading_comments(first)),
                            constant
                                .format()
                                .with_options(ExprConstantLayout::String(StringLayout::DocString)),
                            trailing_comments(comments.trailing_comments(first)),
                        ]
                    )?;
                } else {
                    write!(f, [first.format()])?;
                }
            }
            SuiteKind::Class => {
                if let Some(constant) = get_docstring(first, &f.context().locator()) {
                    if !comments.has_leading_comments(first)
                        && lines_before(first.start(), source) > 1
                    {
                        // Allow up to one empty line before a class docstring, e.g., this is
                        // stable formatting:
                        // ```python
                        // class Test:
                        //
                        //     """Docstring"""
                        // ```
                        write!(f, [empty_line()])?;
                    }
                    write!(
                        f,
                        [
                            leading_comments(comments.leading_comments(first)),
                            constant
                                .format()
                                .with_options(ExprConstantLayout::String(StringLayout::DocString)),
                            trailing_comments(comments.trailing_comments(first)),
                        ]
                    )?;

                    // Enforce an empty line after a class docstring, e.g., these are both stable
                    // formatting:
                    // ```python
                    // class Test:
                    //     """Docstring"""
                    //
                    //     ...
                    //
                    //
                    // class Test:
                    //
                    //     """Docstring"""
                    //
                    //     ...
                    // ```
                    if let Some(second) = iter.next() {
                        // Format the subsequent statement immediately. This rule takes precedence
                        // over the rules in the loop below (and most of them won't apply anyway,
                        // e.g., we know the first statement isn't an import).
                        write!(f, [empty_line(), second.format()])?;
                        last = second;
                    }
                } else {
                    // No docstring, use normal formatting

                    write!(f, [first.format()])?;
                }
            }
            SuiteKind::TopLevel => {
                write!(f, [first.format()])?;
            }
        }

        for statement in iter {
            if is_class_or_function_definition(last) || is_class_or_function_definition(statement) {
                match self.kind {
                    SuiteKind::TopLevel => {
                        write!(f, [empty_line(), empty_line(), statement.format()])?;
                    }
                    SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                        write!(f, [empty_line(), statement.format()])?;
                    }
                }
            } else if is_import_definition(last) && !is_import_definition(statement) {
                write!(f, [empty_line(), statement.format()])?;
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
                let start =
                    if let Some(first_leading) = comments.leading_comments(statement).first() {
                        first_leading.slice().start()
                    } else {
                        statement.start()
                    };

                match lines_before(start, source) {
                    0 | 1 => write!(f, [hard_line_break()])?,
                    2 => write!(f, [empty_line()])?,
                    3.. => match self.kind {
                        SuiteKind::TopLevel => write!(f, [empty_line(), empty_line()])?,
                        SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                            write!(f, [empty_line()])?;
                        }
                    },
                }

                write!(f, [statement.format()])?;
            } else {
                // Insert the appropriate number of empty lines based on the node level, e.g.:
                // * [`NodeLevel::Module`]: Up to two empty lines
                // * [`NodeLevel::CompoundStatement`]: Up to one empty line
                // * [`NodeLevel::Expression`]: No empty lines

                let count_lines = |offset| {
                    // It's necessary to skip any trailing line comment because RustPython doesn't include trailing comments
                    // in the node's range
                    // ```python
                    // a # The range of `a` ends right before this comment
                    //
                    // b
                    // ```
                    //
                    // Simply using `lines_after` doesn't work if a statement has a trailing comment because
                    // it then counts the lines between the statement and the trailing comment, which is
                    // always 0. This is why it skips any trailing trivia (trivia that's on the same line)
                    // and counts the lines after.
                    lines_after_ignoring_trivia(offset, source)
                };

                match node_level {
                    NodeLevel::TopLevel => match count_lines(last.end()) {
                        0 | 1 => write!(f, [hard_line_break()])?,
                        2 => write!(f, [empty_line()])?,
                        _ => write!(f, [empty_line(), empty_line()])?,
                    },
                    NodeLevel::CompoundStatement => match count_lines(last.end()) {
                        0 | 1 => write!(f, [hard_line_break()])?,
                        _ => write!(f, [empty_line()])?,
                    },
                    NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression => {
                        write!(f, [hard_line_break()])?;
                    }
                }

                write!(f, [statement.format()])?;
            }

            last = statement;
        }

        Ok(())
    }
}

/// Returns `true` if a [`Stmt`] is a class or function definition.
const fn is_class_or_function_definition(stmt: &Stmt) -> bool {
    matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_))
}

/// Returns `true` if a [`Stmt`] is an import.
const fn is_import_definition(stmt: &Stmt) -> bool {
    matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_))
}

/// Checks if the statement is a simple string that can be formatted as a docstring
fn get_docstring<'a>(stmt: &'a Stmt, locator: &Locator) -> Option<&'a ExprConstant> {
    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return None;
    };

    let Expr::Constant(constant) = value.as_ref() else {
        return None;
    };
    if let ExprConstant {
        value: Constant::Str(..),
        range,
        ..
    } = constant
    {
        if is_implicit_concatenation(locator.slice(*range)) {
            return None;
        }
        return Some(constant);
    }
    None
}

impl FormatRuleWithOptions<Suite, PyFormatContext<'_>> for FormatSuite {
    type Options = SuiteKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.kind = options;
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
    use ruff_formatter::format;
    use ruff_python_parser::parse_suite;

    use crate::comments::Comments;
    use crate::prelude::*;
    use crate::statement::suite::SuiteKind;
    use crate::PyFormatOptions;

    fn format_suite(level: SuiteKind) -> String {
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

        let statements = parse_suite(source, "test.py").unwrap();

        let context = PyFormatContext::new(PyFormatOptions::default(), source, Comments::default());

        let test_formatter =
            format_with(|f: &mut PyFormatter| statements.format().with_options(level).fmt(f));

        let formatted = format!(context, [test_formatter]).unwrap();
        let printed = formatted.print().unwrap();

        printed.as_code().to_string()
    }

    #[test]
    fn top_level() {
        let formatted = format_suite(SuiteKind::TopLevel);

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
        let formatted = format_suite(SuiteKind::Other);

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
