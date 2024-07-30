use ruff_formatter::{write, Argument, Arguments, FormatError};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{
    ElifElseClause, ExceptHandlerExceptHandler, MatchCase, StmtClassDef, StmtFor, StmtFunctionDef,
    StmtIf, StmtMatch, StmtTry, StmtWhile, StmtWith, Suite,
};
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::comments::{leading_alternate_branch_comments, trailing_comments, SourceComment};
use crate::statement::suite::{contains_only_an_ellipsis, SuiteKind};
use crate::verbatim::write_suppressed_clause_header;
use crate::{has_skip_comment, prelude::*};

/// The header of a compound statement clause.
///
/// > A compound statement consists of one or more ‘clauses.’ A clause consists of a header and a ‘suite.’
/// > The clause headers of a particular compound statement are all at the same indentation level.
/// > Each clause header begins with a uniquely identifying keyword and ends with a colon.
///
/// [source](https://docs.python.org/3/reference/compound_stmts.html#compound-statements)
#[derive(Copy, Clone)]
pub(crate) enum ClauseHeader<'a> {
    Class(&'a StmtClassDef),
    Function(&'a StmtFunctionDef),
    If(&'a StmtIf),
    ElifElse(&'a ElifElseClause),
    Try(&'a StmtTry),
    ExceptHandler(&'a ExceptHandlerExceptHandler),
    TryFinally(&'a StmtTry),
    Match(&'a StmtMatch),
    MatchCase(&'a MatchCase),
    For(&'a StmtFor),
    While(&'a StmtWhile),
    With(&'a StmtWith),
    OrElse(ElseClause<'a>),
}

impl<'a> ClauseHeader<'a> {
    /// The range from the clause keyword up to and including the final colon.
    pub(crate) fn range(self, source: &str) -> FormatResult<TextRange> {
        let keyword_range = self.first_keyword_range(source)?;

        let mut last_child_end = None;

        self.visit(&mut |child| last_child_end = Some(child.end()));

        let end = match self {
            ClauseHeader::Class(class) => Some(last_child_end.unwrap_or(class.name.end())),
            ClauseHeader::Function(function) => Some(last_child_end.unwrap_or(function.name.end())),
            ClauseHeader::ElifElse(_)
            | ClauseHeader::Try(_)
            | ClauseHeader::If(_)
            | ClauseHeader::TryFinally(_)
            | ClauseHeader::Match(_)
            | ClauseHeader::MatchCase(_)
            | ClauseHeader::For(_)
            | ClauseHeader::While(_)
            | ClauseHeader::With(_)
            | ClauseHeader::OrElse(_) => last_child_end,

            ClauseHeader::ExceptHandler(handler) => {
                handler.name.as_ref().map(Ranged::end).or(last_child_end)
            }
        };

        let colon = colon_range(end.unwrap_or(keyword_range.end()), source)?;

        Ok(TextRange::new(keyword_range.start(), colon.end()))
    }

    /// Visits the nodes in the case header.
    pub(crate) fn visit<F>(self, visitor: &mut F)
    where
        F: FnMut(AnyNodeRef),
    {
        fn visit<'a, N, F>(node: N, visitor: &mut F)
        where
            N: Into<AnyNodeRef<'a>>,
            F: FnMut(AnyNodeRef<'a>),
        {
            visitor(node.into());
        }

        match self {
            ClauseHeader::Class(StmtClassDef {
                type_params,
                arguments,
                range: _,
                decorator_list: _,
                name: _,
                body: _,
            }) => {
                if let Some(type_params) = type_params.as_deref() {
                    visit(type_params, visitor);
                }

                if let Some(arguments) = arguments {
                    visit(arguments.as_ref(), visitor);
                }
            }
            ClauseHeader::Function(StmtFunctionDef {
                type_params,
                parameters,
                range: _,
                is_async: _,
                decorator_list: _,
                name: _,
                returns,
                body: _,
            }) => {
                if let Some(type_params) = type_params.as_deref() {
                    visit(type_params, visitor);
                }
                visit(parameters.as_ref(), visitor);

                if let Some(returns) = returns.as_deref() {
                    visit(returns, visitor);
                }
            }
            ClauseHeader::If(StmtIf {
                test,
                range: _,
                body: _,
                elif_else_clauses: _,
            }) => {
                visit(test.as_ref(), visitor);
            }
            ClauseHeader::ElifElse(ElifElseClause {
                test,
                range: _,
                body: _,
            }) => {
                if let Some(test) = test.as_ref() {
                    visit(test, visitor);
                }
            }

            ClauseHeader::ExceptHandler(ExceptHandlerExceptHandler {
                type_: type_expr,
                range: _,
                name: _,
                body: _,
            }) => {
                if let Some(type_expr) = type_expr.as_deref() {
                    visit(type_expr, visitor);
                }
            }
            ClauseHeader::Match(StmtMatch {
                subject,
                range: _,
                cases: _,
            }) => {
                visit(subject.as_ref(), visitor);
            }
            ClauseHeader::MatchCase(MatchCase {
                guard,
                pattern,
                range: _,
                body: _,
            }) => {
                visit(pattern, visitor);

                if let Some(guard) = guard.as_deref() {
                    visit(guard, visitor);
                }
            }
            ClauseHeader::For(StmtFor {
                target,
                iter,
                range: _,
                is_async: _,
                body: _,
                orelse: _,
            }) => {
                visit(target.as_ref(), visitor);
                visit(iter.as_ref(), visitor);
            }
            ClauseHeader::While(StmtWhile {
                test,
                range: _,
                body: _,
                orelse: _,
            }) => {
                visit(test.as_ref(), visitor);
            }
            ClauseHeader::With(StmtWith {
                items,
                range: _,
                is_async: _,
                body: _,
            }) => {
                for item in items {
                    visit(item, visitor);
                }
            }
            ClauseHeader::Try(_) | ClauseHeader::TryFinally(_) | ClauseHeader::OrElse(_) => {}
        }
    }

    /// Returns the range of the first keyword that marks the start of the clause header.
    fn first_keyword_range(self, source: &str) -> FormatResult<TextRange> {
        match self {
            ClauseHeader::Class(header) => {
                let start_position = header
                    .decorator_list
                    .last()
                    .map_or_else(|| header.start(), Ranged::end);
                find_keyword(start_position, SimpleTokenKind::Class, source)
            }
            ClauseHeader::Function(header) => {
                let start_position = header
                    .decorator_list
                    .last()
                    .map_or_else(|| header.start(), Ranged::end);
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::Def
                };
                find_keyword(start_position, keyword, source)
            }
            ClauseHeader::If(header) => find_keyword(header.start(), SimpleTokenKind::If, source),
            ClauseHeader::ElifElse(ElifElseClause {
                test: None, range, ..
            }) => find_keyword(range.start(), SimpleTokenKind::Else, source),
            ClauseHeader::ElifElse(ElifElseClause {
                test: Some(_),
                range,
                ..
            }) => find_keyword(range.start(), SimpleTokenKind::Elif, source),
            ClauseHeader::Try(header) => find_keyword(header.start(), SimpleTokenKind::Try, source),
            ClauseHeader::ExceptHandler(header) => {
                find_keyword(header.start(), SimpleTokenKind::Except, source)
            }
            ClauseHeader::TryFinally(header) => {
                let last_statement = header
                    .orelse
                    .last()
                    .map(AnyNodeRef::from)
                    .or_else(|| header.handlers.last().map(AnyNodeRef::from))
                    .or_else(|| header.body.last().map(AnyNodeRef::from))
                    .unwrap();

                find_keyword(last_statement.end(), SimpleTokenKind::Finally, source)
            }
            ClauseHeader::Match(header) => {
                find_keyword(header.start(), SimpleTokenKind::Match, source)
            }
            ClauseHeader::MatchCase(header) => {
                find_keyword(header.start(), SimpleTokenKind::Case, source)
            }
            ClauseHeader::For(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::For
                };
                find_keyword(header.start(), keyword, source)
            }
            ClauseHeader::While(header) => {
                find_keyword(header.start(), SimpleTokenKind::While, source)
            }
            ClauseHeader::With(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::With
                };

                find_keyword(header.start(), keyword, source)
            }
            ClauseHeader::OrElse(header) => match header {
                ElseClause::Try(try_stmt) => {
                    let last_statement = try_stmt
                        .handlers
                        .last()
                        .map(AnyNodeRef::from)
                        .or_else(|| try_stmt.body.last().map(AnyNodeRef::from))
                        .unwrap();

                    find_keyword(last_statement.end(), SimpleTokenKind::Else, source)
                }
                ElseClause::For(StmtFor { body, .. })
                | ElseClause::While(StmtWhile { body, .. }) => {
                    find_keyword(body.last().unwrap().end(), SimpleTokenKind::Else, source)
                }
            },
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ElseClause<'a> {
    Try(&'a StmtTry),
    For(&'a StmtFor),
    While(&'a StmtWhile),
}

pub(crate) struct FormatClauseHeader<'a, 'ast> {
    header: ClauseHeader<'a>,
    /// How to format the clause header
    formatter: Argument<'a, PyFormatContext<'ast>>,

    /// Leading comments coming before the branch, together with the previous node, if any. Only relevant
    /// for alternate branches.
    leading_comments: Option<(&'a [SourceComment], Option<AnyNodeRef<'a>>)>,

    /// The trailing comments coming after the colon.
    trailing_colon_comment: &'a [SourceComment],
}

/// Formats a clause header, handling the case where the clause header is suppressed and should not be formatted.
///
/// Calls the `formatter` to format the content of the `header`, except if the `trailing_colon_comment` is a `fmt: skip` suppression comment.
/// Takes care of formatting the `trailing_colon_comment` and adds the `:` at the end of the header.
pub(crate) fn clause_header<'a, 'ast, Content>(
    header: ClauseHeader<'a>,
    trailing_colon_comment: &'a [SourceComment],
    formatter: &'a Content,
) -> FormatClauseHeader<'a, 'ast>
where
    Content: Format<PyFormatContext<'ast>>,
{
    FormatClauseHeader {
        header,
        formatter: Argument::new(formatter),
        leading_comments: None,
        trailing_colon_comment,
    }
}

impl<'a> FormatClauseHeader<'a, '_> {
    /// Sets the leading comments that precede an alternate branch.
    #[must_use]
    pub(crate) fn with_leading_comments<N>(
        mut self,
        comments: &'a [SourceComment],
        last_node: Option<N>,
    ) -> Self
    where
        N: Into<AnyNodeRef<'a>>,
    {
        self.leading_comments = Some((comments, last_node.map(Into::into)));
        self
    }
}

impl<'ast> Format<PyFormatContext<'ast>> for FormatClauseHeader<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        if let Some((leading_comments, last_node)) = self.leading_comments {
            leading_alternate_branch_comments(leading_comments, last_node).fmt(f)?;
        }

        if has_skip_comment(self.trailing_colon_comment, f.context().source()) {
            write_suppressed_clause_header(self.header, f)?;
        } else {
            // Write a source map entry for the colon for range formatting to support formatting the clause header without
            // the clause body. Avoid computing `self.header.range()` otherwise because it's somewhat involved.
            let clause_end = if f.options().source_map_generation().is_enabled() {
                Some(source_position(
                    self.header.range(f.context().source())?.end(),
                ))
            } else {
                None
            };

            write!(
                f,
                [Arguments::from(&self.formatter), token(":"), clause_end]
            )?;
        }

        trailing_comments(self.trailing_colon_comment).fmt(f)
    }
}

pub(crate) struct FormatClauseBody<'a> {
    body: &'a Suite,
    kind: SuiteKind,
    trailing_comments: &'a [SourceComment],
}

pub(crate) fn clause_body<'a>(
    body: &'a Suite,
    kind: SuiteKind,
    trailing_comments: &'a [SourceComment],
) -> FormatClauseBody<'a> {
    FormatClauseBody {
        body,
        kind,
        trailing_comments,
    }
}

impl Format<PyFormatContext<'_>> for FormatClauseBody<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // In stable, stubs are only collapsed in stub files, in preview stubs in functions
        // or classes are collapsed too
        let should_collapse_stub = f.options().source_type().is_stub()
            || matches!(self.kind, SuiteKind::Function | SuiteKind::Class);

        if should_collapse_stub
            && contains_only_an_ellipsis(self.body, f.context().comments())
            && self.trailing_comments.is_empty()
        {
            write!(
                f,
                [
                    space(),
                    self.body.format().with_options(self.kind),
                    hard_line_break()
                ]
            )
        } else {
            write!(
                f,
                [
                    trailing_comments(self.trailing_comments),
                    block_indent(&self.body.format().with_options(self.kind))
                ]
            )
        }
    }
}

/// Finds the range of `keyword` starting the search at `start_position`. Expects only comments and `(` between
/// the `start_position` and the `keyword` token.
fn find_keyword(
    start_position: TextSize,
    keyword: SimpleTokenKind,
    source: &str,
) -> FormatResult<TextRange> {
    let mut tokenizer = SimpleTokenizer::starts_at(start_position, source).skip_trivia();

    match tokenizer.next() {
        Some(token) if token.kind() == keyword => Ok(token.range()),
        Some(other) => {
            debug_assert!(
                false,
                "Expected the keyword token {keyword:?} but found the token {other:?} instead."
            );
            Err(FormatError::syntax_error(
                "Expected the keyword token but found another token instead.",
            ))
        }
        None => {
            debug_assert!(
                false,
                "Expected the keyword token {keyword:?} but reached the end of the source instead."
            );
            Err(FormatError::syntax_error(
                "Expected the case header keyword token but reached the end of the source instead.",
            ))
        }
    }
}

/// Returns the range of the `:` ending the clause header or `Err` if the colon can't be found.
fn colon_range(after_keyword_or_condition: TextSize, source: &str) -> FormatResult<TextRange> {
    let mut tokenizer = SimpleTokenizer::starts_at(after_keyword_or_condition, source)
        .skip_trivia()
        .skip_while(|token| {
            matches!(
                token.kind(),
                SimpleTokenKind::RParen | SimpleTokenKind::Comma
            )
        });

    match tokenizer.next() {
        Some(SimpleToken {
            kind: SimpleTokenKind::Colon,
            range,
        }) => Ok(range),
        Some(token) => {
            debug_assert!(false, "Expected the colon marking the end of the case header but found {token:?} instead.");
            Err(FormatError::syntax_error("Expected colon marking the end of the case header but found another token instead."))
        }
        None => {
            debug_assert!(false, "Expected the colon marking the end of the case header but found the end of the range.");
            Err(FormatError::syntax_error("Expected the colon marking the end of the case header but found the end of the range."))
        }
    }
}
