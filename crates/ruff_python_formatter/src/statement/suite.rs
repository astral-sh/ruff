use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::helpers::is_compound_statement;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{self as ast, Expr, PySourceType, Stmt, Suite};
use ruff_python_trivia::{lines_after, lines_after_ignoring_end_of_line_trivia, lines_before};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{
    leading_comments, trailing_comments, Comments, LeadingDanglingTrailingComments,
};
use crate::context::{NodeLevel, TopLevelStatementPosition, WithNodeLevel};
use crate::expression::string::StringLayout;
use crate::prelude::*;
use crate::statement::stmt_expr::FormatStmtExpr;
use crate::verbatim::{
    suppressed_node, write_suppressed_statements_starting_with_leading_comment,
    write_suppressed_statements_starting_with_trailing_comment,
};

/// Level at which the [`Suite`] appears in the source code.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SuiteKind {
    /// Statements at the module level / top level
    TopLevel,

    /// Statements in a function body.
    Function,

    /// Statements in a class body.
    Class,

    /// Statements in any other body (e.g., `if` or `while`).
    #[default]
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
        let mut iter = statements.iter();
        let Some(first) = iter.next() else {
            return Ok(());
        };

        let node_level = match self.kind {
            SuiteKind::TopLevel => NodeLevel::TopLevel(
                iter.clone()
                    .next()
                    .map_or(TopLevelStatementPosition::Last, |_| {
                        TopLevelStatementPosition::Other
                    }),
            ),
            SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                NodeLevel::CompoundStatement
            }
        };

        let comments = f.context().comments().clone();
        let source = f.context().source();
        let source_type = f.options().source_type();

        let f = &mut WithNodeLevel::new(node_level, f);

        // Format the first statement in the body, which often has special formatting rules.
        let first = match self.kind {
            SuiteKind::Other => {
                if is_class_or_function_definition(first)
                    && !comments.has_leading(first)
                    && !source_type.is_stub()
                {
                    // Add an empty line for any nested functions or classes defined within
                    // non-function or class compound statements, e.g., this is stable formatting:
                    // ```python
                    // if True:
                    //
                    //     def test():
                    //         ...
                    // ```
                    empty_line().fmt(f)?;
                }

                SuiteChildStatement::Other(first)
            }

            SuiteKind::Function => {
                if let Some(docstring) = DocstringStmt::try_from_statement(first, self.kind) {
                    SuiteChildStatement::Docstring(docstring)
                } else {
                    SuiteChildStatement::Other(first)
                }
            }

            SuiteKind::Class => {
                if let Some(docstring) = DocstringStmt::try_from_statement(first, self.kind) {
                    if !comments.has_leading(first)
                        && lines_before(first.start(), source) > 1
                        && !source_type.is_stub()
                    {
                        // Allow up to one empty line before a class docstring, e.g., this is
                        // stable formatting:
                        // ```python
                        // class Test:
                        //
                        //     """Docstring"""
                        // ```
                        empty_line().fmt(f)?;
                    }

                    SuiteChildStatement::Docstring(docstring)
                } else {
                    SuiteChildStatement::Other(first)
                }
            }
            SuiteKind::TopLevel => SuiteChildStatement::Other(first),
        };

        let first_comments = comments.leading_dangling_trailing(first);

        let (mut preceding, mut empty_line_after_docstring) = if first_comments
            .leading
            .iter()
            .any(|comment| comment.is_suppression_off_comment(source))
        {
            (
                write_suppressed_statements_starting_with_leading_comment(first, &mut iter, f)?,
                false,
            )
        } else if first_comments
            .trailing
            .iter()
            .any(|comment| comment.is_suppression_off_comment(source))
        {
            (
                write_suppressed_statements_starting_with_trailing_comment(first, &mut iter, f)?,
                false,
            )
        } else {
            first.fmt(f)?;

            #[allow(clippy::if_same_then_else)]
            let empty_line_after_docstring = if matches!(first, SuiteChildStatement::Docstring(_))
                && self.kind == SuiteKind::Class
            {
                true
            } else if f.options().preview().is_enabled()
                && self.kind == SuiteKind::TopLevel
                && DocstringStmt::try_from_statement(first.statement(), self.kind).is_some()
            {
                // Only in preview mode, insert a newline after a module level docstring, but treat
                // it as a docstring otherwise. See: https://github.com/psf/black/pull/3932.
                true
            } else {
                false
            };

            (first.statement(), empty_line_after_docstring)
        };

        let mut preceding_comments = comments.leading_dangling_trailing(preceding);

        while let Some(following) = iter.next() {
            if self.kind == SuiteKind::TopLevel && iter.clone().next().is_none() {
                f.context_mut()
                    .set_node_level(NodeLevel::TopLevel(TopLevelStatementPosition::Last));
            }

            let following_comments = comments.leading_dangling_trailing(following);

            let needs_empty_lines = if is_class_or_function_definition(following) {
                // Here we insert empty lines even if the preceding has a trailing own line comment
                true
            } else {
                // Find nested class or function definitions that need an empty line after them.
                //
                // ```python
                // def f():
                //     if True:
                //
                //         def double(s):
                //             return s + s
                //
                //     print("below function")
                // ```
                std::iter::successors(
                    Some(AnyNodeRef::from(preceding)),
                    AnyNodeRef::last_child_in_body,
                )
                .take_while(|last_child|
                    // If there is a comment between preceding and following the empty lines were
                    // inserted before the comment by preceding and there are no extra empty lines
                    // after the comment.
                    // ```python
                    // class Test:
                    //     def a(self):
                    //         pass
                    //         # trailing comment
                    //
                    //
                    // # two lines before, one line after
                    //
                    // c = 30
                    // ````
                    // This also includes nested class/function definitions, so we stop recursing
                    // once we see a node with a trailing own line comment:
                    // ```python
                    // def f():
                    //     if True:
                    //
                    //         def double(s):
                    //             return s + s
                    //
                    //         # nested trailing own line comment
                    //     print("below function with trailing own line comment")
                    // ```
                    !comments.has_trailing_own_line(*last_child))
                .any(|last_child| {
                    matches!(
                        last_child,
                        AnyNodeRef::StmtFunctionDef(_) | AnyNodeRef::StmtClassDef(_)
                    )
                })
            };

            // Add empty lines before and after a function or class definition. If the preceding
            // node is a function or class, and contains trailing comments, then the statement
            // itself will add the requisite empty lines when formatting its comments.
            if needs_empty_lines {
                if source_type.is_stub() {
                    stub_file_empty_lines(
                        self.kind,
                        preceding,
                        following,
                        &preceding_comments,
                        &following_comments,
                        f,
                    )?;
                } else {
                    match self.kind {
                        SuiteKind::TopLevel => {
                            write!(f, [empty_line(), empty_line()])?;
                        }
                        SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                            empty_line().fmt(f)?;
                        }
                    }
                }
            } else if is_import_definition(preceding)
                && (!is_import_definition(following) || following_comments.has_leading())
            {
                // Enforce _at least_ one empty line after an import statement (but allow up to
                // two at the top-level). In this context, "after an import statement" means that
                // that the previous node is an import, and the following node is an import _or_ has
                // a leading comment.
                match self.kind {
                    SuiteKind::TopLevel => {
                        let end = if let Some(last_trailing) = preceding_comments.trailing.last() {
                            last_trailing.end()
                        } else {
                            preceding.end()
                        };
                        match lines_after(end, source) {
                            0..=2 => empty_line().fmt(f)?,
                            _ => match source_type {
                                PySourceType::Stub => {
                                    empty_line().fmt(f)?;
                                }
                                PySourceType::Python | PySourceType::Ipynb => {
                                    write!(f, [empty_line(), empty_line()])?;
                                }
                            },
                        }
                    }
                    SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                        empty_line().fmt(f)?;
                    }
                }
            } else if is_compound_statement(preceding) {
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
                let start = if let Some(first_leading) = following_comments.leading.first() {
                    first_leading.start()
                } else {
                    following.start()
                };

                match lines_before(start, source) {
                    0 | 1 => hard_line_break().fmt(f)?,
                    2 => empty_line().fmt(f)?,
                    _ => match self.kind {
                        SuiteKind::TopLevel => match source_type {
                            PySourceType::Stub => {
                                empty_line().fmt(f)?;
                            }
                            PySourceType::Python | PySourceType::Ipynb => {
                                write!(f, [empty_line(), empty_line()])?;
                            }
                        },
                        SuiteKind::Function | SuiteKind::Class | SuiteKind::Other => {
                            empty_line().fmt(f)?;
                        }
                    },
                }
            } else if empty_line_after_docstring {
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
                empty_line().fmt(f)?;
            } else {
                // Insert the appropriate number of empty lines based on the node level, e.g.:
                // * [`NodeLevel::Module`]: Up to two empty lines
                // * [`NodeLevel::CompoundStatement`]: Up to one empty line
                // * [`NodeLevel::Expression`]: No empty lines

                // It's necessary to skip any trailing line comment because our parser doesn't
                // include trailing comments in the node's range:
                // ```python
                // a # The range of `a` ends right before this comment
                //
                // b
                // ```
                let end = preceding_comments
                    .trailing
                    .last()
                    .map_or(preceding.end(), |comment| comment.slice().end());

                match node_level {
                    NodeLevel::TopLevel(_) => match lines_after(end, source) {
                        0 | 1 => hard_line_break().fmt(f)?,
                        2 => empty_line().fmt(f)?,
                        _ => match source_type {
                            PySourceType::Stub => {
                                empty_line().fmt(f)?;
                            }
                            PySourceType::Python | PySourceType::Ipynb => {
                                write!(f, [empty_line(), empty_line()])?;
                            }
                        },
                    },
                    NodeLevel::CompoundStatement => match lines_after(end, source) {
                        0 | 1 => hard_line_break().fmt(f)?,
                        _ => empty_line().fmt(f)?,
                    },
                    NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression => {
                        hard_line_break().fmt(f)?;
                    }
                }
            }

            if following_comments
                .leading
                .iter()
                .any(|comment| comment.is_suppression_off_comment(source))
            {
                preceding = write_suppressed_statements_starting_with_leading_comment(
                    SuiteChildStatement::Other(following),
                    &mut iter,
                    f,
                )?;
                preceding_comments = comments.leading_dangling_trailing(preceding);
            } else if following_comments
                .trailing
                .iter()
                .any(|comment| comment.is_suppression_off_comment(source))
            {
                preceding = write_suppressed_statements_starting_with_trailing_comment(
                    SuiteChildStatement::Other(following),
                    &mut iter,
                    f,
                )?;
                preceding_comments = comments.leading_dangling_trailing(preceding);
            } else {
                following.format().fmt(f)?;
                preceding = following;
                preceding_comments = following_comments;
            }

            empty_line_after_docstring = false;
        }

        Ok(())
    }
}

/// Stub files have bespoke rules for empty lines.
///
/// These rules are ported from black (preview mode at time of writing) using the stubs test case:
/// <https://github.com/psf/black/blob/c160e4b7ce30c661ac4f2dfa5038becf1b8c5c33/src/black/lines.py#L576-L744>
fn stub_file_empty_lines(
    kind: SuiteKind,
    preceding: &Stmt,
    following: &Stmt,
    preceding_comments: &LeadingDanglingTrailingComments,
    following_comments: &LeadingDanglingTrailingComments,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    let source = f.context().source();
    // Preserve the empty line if the definitions are separated by a comment
    let empty_line_condition = preceding_comments.has_trailing()
        || following_comments.has_leading()
        || !stub_suite_can_omit_empty_line(preceding, following, f);
    match kind {
        SuiteKind::TopLevel => {
            if empty_line_condition {
                empty_line().fmt(f)
            } else {
                hard_line_break().fmt(f)
            }
        }
        SuiteKind::Class | SuiteKind::Other | SuiteKind::Function => {
            if empty_line_condition
                && lines_after_ignoring_end_of_line_trivia(preceding.end(), source) > 1
            {
                empty_line().fmt(f)
            } else {
                hard_line_break().fmt(f)
            }
        }
    }
}

/// Only a function to compute it lazily
fn stub_suite_can_omit_empty_line(preceding: &Stmt, following: &Stmt, f: &PyFormatter) -> bool {
    // Two subsequent class definitions that both have an ellipsis only body
    // ```python
    // class A: ...
    // class B: ...
    //
    // @decorator
    // class C: ...
    // ```
    let class_sequences_with_ellipsis_only = preceding
        .as_class_def_stmt()
        .is_some_and(|class| contains_only_an_ellipsis(&class.body, f.context().comments()))
        && following.as_class_def_stmt().is_some_and(|class| {
            contains_only_an_ellipsis(&class.body, f.context().comments())
                && class.decorator_list.is_empty()
        });

    // Black for some reasons accepts decorators in place of empty lines
    // ```python
    // def _count1(): ...
    // @final
    // class LockType1: ...
    //
    // def _count2(): ...
    //
    // class LockType2: ...
    // ```
    let class_decorator_instead_of_empty_line = preceding.is_function_def_stmt()
        && following
            .as_class_def_stmt()
            .is_some_and(|class| !class.decorator_list.is_empty());

    // A function definition following a stub function definition
    // ```python
    // def test(): ...
    // def b(): a
    // ```
    let function_with_ellipsis = preceding
        .as_function_def_stmt()
        .is_some_and(|function| contains_only_an_ellipsis(&function.body, f.context().comments()))
        && following.is_function_def_stmt();

    class_sequences_with_ellipsis_only
        || class_decorator_instead_of_empty_line
        || function_with_ellipsis
}

/// Returns `true` if a function or class body contains only an ellipsis with no comments.
pub(crate) fn contains_only_an_ellipsis(body: &[Stmt], comments: &Comments) -> bool {
    match body {
        [Stmt::Expr(ast::StmtExpr { value, .. })] => {
            let [node] = body else {
                return false;
            };
            value.is_ellipsis_literal_expr() && !comments.has_leading(node)
        }
        _ => false,
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

/// A statement representing a docstring.
#[derive(Copy, Clone, Debug)]
pub(crate) struct DocstringStmt<'a> {
    /// The [`Stmt::Expr`]
    docstring: &'a Stmt,
    /// The parent suite kind
    suite_kind: SuiteKind,
}

impl<'a> DocstringStmt<'a> {
    /// Checks if the statement is a simple string that can be formatted as a docstring
    fn try_from_statement(stmt: &'a Stmt, suite_kind: SuiteKind) -> Option<DocstringStmt<'a>> {
        let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
            return None;
        };

        match value.as_ref() {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. })
                if !value.is_implicit_concatenated() =>
            {
                Some(DocstringStmt {
                    docstring: stmt,
                    suite_kind,
                })
            }
            _ => None,
        }
    }
}

impl Format<PyFormatContext<'_>> for DocstringStmt<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let node_comments = comments.leading_dangling_trailing(self.docstring);

        if FormatStmtExpr.is_suppressed(node_comments.trailing, f.context()) {
            suppressed_node(self.docstring).fmt(f)
        } else {
            // SAFETY: Safe because `DocStringStmt` guarantees that it only ever wraps a `ExprStmt` containing a `ExprStringLiteral`.
            let string_literal = self
                .docstring
                .as_expr_stmt()
                .unwrap()
                .value
                .as_string_literal_expr()
                .unwrap();

            // We format the expression, but the statement carries the comments
            write!(
                f,
                [
                    leading_comments(node_comments.leading),
                    string_literal
                        .format()
                        .with_options(StringLayout::DocString),
                ]
            )?;

            if self.suite_kind == SuiteKind::Class {
                // Comments after class docstrings need a newline between the docstring and the
                // comment (https://github.com/astral-sh/ruff/issues/7948).
                // ```python
                // class ModuleBrowser:
                //     """Browse module classes and functions in IDLE."""
                //     # ^ Insert a newline above here
                //
                //     def __init__(self, master, path, *, _htest=False, _utest=False):
                //         pass
                // ```
                if let Some(own_line) = node_comments
                    .trailing
                    .iter()
                    .find(|comment| comment.line_position().is_own_line())
                {
                    if lines_before(own_line.start(), f.context().source()) < 2 {
                        empty_line().fmt(f)?;
                    }
                }
            }

            trailing_comments(node_comments.trailing).fmt(f)
        }
    }
}

/// A Child of a suite.
#[derive(Copy, Clone, Debug)]
pub(crate) enum SuiteChildStatement<'a> {
    /// A docstring documenting a class or function definition.
    Docstring(DocstringStmt<'a>),

    /// Any other statement.
    Other(&'a Stmt),
}

impl<'a> SuiteChildStatement<'a> {
    pub(crate) const fn statement(self) -> &'a Stmt {
        match self {
            SuiteChildStatement::Docstring(docstring) => docstring.docstring,
            SuiteChildStatement::Other(statement) => statement,
        }
    }
}

impl Ranged for SuiteChildStatement<'_> {
    fn range(&self) -> TextRange {
        self.statement().range()
    }
}

impl<'a> From<SuiteChildStatement<'a>> for AnyNodeRef<'a> {
    fn from(value: SuiteChildStatement<'a>) -> Self {
        value.statement().into()
    }
}

impl Format<PyFormatContext<'_>> for SuiteChildStatement<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self {
            SuiteChildStatement::Docstring(docstring) => docstring.fmt(f),
            SuiteChildStatement::Other(statement) => statement.format().fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_formatter::format;
    use ruff_python_parser::parse_suite;
    use ruff_python_trivia::CommentRanges;

    use crate::comments::Comments;
    use crate::prelude::*;
    use crate::statement::suite::SuiteKind;
    use crate::PyFormatOptions;

    fn format_suite(level: SuiteKind) -> String {
        let source = r"
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
";

        let statements = parse_suite(source, "test.py").unwrap();

        let comment_ranges = CommentRanges::default();
        let context = PyFormatContext::new(
            PyFormatOptions::default(),
            source,
            Comments::from_ranges(&comment_ranges),
        );

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
            r"a = 10


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
"
        );
    }

    #[test]
    fn nested_level() {
        let formatted = format_suite(SuiteKind::Other);

        assert_eq!(
            formatted,
            r"a = 10

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
"
        );
    }
}
