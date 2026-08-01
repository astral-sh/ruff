use ruff_formatter::{Argument, Arguments, FormatError, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{
    ElifElseClause, ExceptHandlerExceptHandler, MatchCase, StmtClassDef, StmtFor, StmtFunctionDef,
    StmtIf, StmtMatch, StmtTry, StmtWhile, StmtWith, Suite,
};
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::comments::{SourceComment, leading_alternate_branch_comments, trailing_comments};
use crate::statement::suite::{SuiteKind, as_only_an_ellipsis};
use crate::verbatim::{verbatim_text, write_suppressed_clause_header};
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
    /// Returns the last child in the clause body immediately following this clause header.
    ///
    /// For most clauses, this is the last statement in
    /// the primary body. For clauses like `try`, it specifically returns the last child
    /// in the `try` body, not the `except`/`else`/`finally` clauses.
    ///
    /// This is similar to [`ruff_python_ast::AnyNodeRef::last_child_in_body`]
    /// but restricted to the clause.
    pub(crate) fn last_child_in_clause(self) -> Option<AnyNodeRef<'a>> {
        match self {
            ClauseHeader::Class(StmtClassDef { body, .. })
            | ClauseHeader::Function(StmtFunctionDef { body, .. })
            | ClauseHeader::If(StmtIf { body, .. })
            | ClauseHeader::ElifElse(ElifElseClause { body, .. })
            | ClauseHeader::Try(StmtTry { body, .. })
            | ClauseHeader::MatchCase(MatchCase { body, .. })
            | ClauseHeader::For(StmtFor { body, .. })
            | ClauseHeader::While(StmtWhile { body, .. })
            | ClauseHeader::With(StmtWith { body, .. })
            | ClauseHeader::ExceptHandler(ExceptHandlerExceptHandler { body, .. })
            | ClauseHeader::OrElse(
                ElseClause::Try(StmtTry { orelse: body, .. })
                | ElseClause::For(StmtFor { orelse: body, .. })
                | ElseClause::While(StmtWhile { orelse: body, .. }),
            )
            | ClauseHeader::TryFinally(StmtTry {
                finalbody: body, ..
            }) => body.last().map(AnyNodeRef::from),
            ClauseHeader::Match(StmtMatch { cases, .. }) => cases
                .last()
                .and_then(|case| case.body.last().map(AnyNodeRef::from)),
        }
    }

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
                node_index: _,
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
                node_index: _,
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
                node_index: _,
                body: _,
                elif_else_clauses: _,
            }) => {
                visit(test.as_ref(), visitor);
            }
            ClauseHeader::ElifElse(ElifElseClause {
                test,
                range: _,
                node_index: _,
                body: _,
            }) => {
                if let Some(test) = test.as_ref() {
                    visit(test, visitor);
                }
            }

            ClauseHeader::ExceptHandler(ExceptHandlerExceptHandler {
                type_: type_expr,
                range: _,
                node_index: _,
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
                node_index: _,
                cases: _,
            }) => {
                visit(subject.as_ref(), visitor);
            }
            ClauseHeader::MatchCase(MatchCase {
                guard,
                pattern,
                range: _,
                node_index: _,
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
                node_index: _,
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
                node_index: _,
                body: _,
                orelse: _,
            }) => {
                visit(test.as_ref(), visitor);
            }
            ClauseHeader::With(StmtWith {
                items,
                range: _,
                node_index: _,
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
                find_keyword(
                    StartPosition::ClauseStart(start_position),
                    SimpleTokenKind::Class,
                    source,
                )
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
                find_keyword(StartPosition::ClauseStart(start_position), keyword, source)
            }
            ClauseHeader::If(header) => find_keyword(
                StartPosition::clause_start(header),
                SimpleTokenKind::If,
                source,
            ),
            ClauseHeader::ElifElse(ElifElseClause {
                test: None, range, ..
            }) => find_keyword(
                StartPosition::clause_start(range),
                SimpleTokenKind::Else,
                source,
            ),
            ClauseHeader::ElifElse(ElifElseClause {
                test: Some(_),
                range,
                ..
            }) => find_keyword(
                StartPosition::clause_start(range),
                SimpleTokenKind::Elif,
                source,
            ),
            ClauseHeader::Try(header) => find_keyword(
                StartPosition::clause_start(header),
                SimpleTokenKind::Try,
                source,
            ),
            ClauseHeader::ExceptHandler(header) => find_keyword(
                StartPosition::clause_start(header),
                SimpleTokenKind::Except,
                source,
            ),
            ClauseHeader::TryFinally(header) => {
                let last_statement = header
                    .orelse
                    .last()
                    .map(AnyNodeRef::from)
                    .or_else(|| header.handlers.last().map(AnyNodeRef::from))
                    .or_else(|| header.body.last().map(AnyNodeRef::from))
                    .unwrap();

                find_keyword(
                    StartPosition::LastStatement(last_statement.end()),
                    SimpleTokenKind::Finally,
                    source,
                )
            }
            ClauseHeader::Match(header) => find_keyword(
                StartPosition::clause_start(header),
                SimpleTokenKind::Match,
                source,
            ),
            ClauseHeader::MatchCase(header) => find_keyword(
                StartPosition::clause_start(header),
                SimpleTokenKind::Case,
                source,
            ),
            ClauseHeader::For(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::For
                };
                find_keyword(StartPosition::clause_start(header), keyword, source)
            }
            ClauseHeader::While(header) => find_keyword(
                StartPosition::clause_start(header),
                SimpleTokenKind::While,
                source,
            ),
            ClauseHeader::With(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::With
                };

                find_keyword(StartPosition::clause_start(header), keyword, source)
            }
            ClauseHeader::OrElse(header) => match header {
                ElseClause::Try(try_stmt) => {
                    let last_statement = try_stmt
                        .handlers
                        .last()
                        .map(AnyNodeRef::from)
                        .or_else(|| try_stmt.body.last().map(AnyNodeRef::from))
                        .unwrap();

                    find_keyword(
                        StartPosition::LastStatement(last_statement.end()),
                        SimpleTokenKind::Else,
                        source,
                    )
                }
                ElseClause::For(StmtFor { body, .. })
                | ElseClause::While(StmtWhile { body, .. }) => find_keyword(
                    StartPosition::LastStatement(body.last().unwrap().end()),
                    SimpleTokenKind::Else,
                    source,
                ),
            },
        }
    }
}

impl<'a> From<ClauseHeader<'a>> for AnyNodeRef<'a> {
    fn from(value: ClauseHeader<'a>) -> Self {
        match value {
            ClauseHeader::Class(stmt_class_def) => stmt_class_def.into(),
            ClauseHeader::Function(stmt_function_def) => stmt_function_def.into(),
            ClauseHeader::If(stmt_if) => stmt_if.into(),
            ClauseHeader::ElifElse(elif_else_clause) => elif_else_clause.into(),
            ClauseHeader::Try(stmt_try) => stmt_try.into(),
            ClauseHeader::ExceptHandler(except_handler_except_handler) => {
                except_handler_except_handler.into()
            }
            ClauseHeader::TryFinally(stmt_try) => stmt_try.into(),
            ClauseHeader::Match(stmt_match) => stmt_match.into(),
            ClauseHeader::MatchCase(match_case) => match_case.into(),
            ClauseHeader::For(stmt_for) => stmt_for.into(),
            ClauseHeader::While(stmt_while) => stmt_while.into(),
            ClauseHeader::With(stmt_with) => stmt_with.into(),
            ClauseHeader::OrElse(else_clause) => else_clause.into(),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ElseClause<'a> {
    Try(&'a StmtTry),
    For(&'a StmtFor),
    While(&'a StmtWhile),
}

impl<'a> From<ElseClause<'a>> for AnyNodeRef<'a> {
    fn from(value: ElseClause<'a>) -> Self {
        match value {
            ElseClause::Try(stmt_try) => stmt_try.into(),
            ElseClause::For(stmt_for) => stmt_for.into(),
            ElseClause::While(stmt_while) => stmt_while.into(),
        }
    }
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

struct FormatClauseBody<'a> {
    body: &'a Suite,
    kind: SuiteKind,
    trailing_comments: &'a [SourceComment],
}

fn clause_body<'a>(
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
            && let Some(ellipsis) = as_only_an_ellipsis(self.body, f.context().comments())
            && self.trailing_comments.is_empty()
        {
            write!(f, [space(), ellipsis.format(), hard_line_break()])
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

pub(crate) struct FormatClause<'a, 'ast> {
    header: ClauseHeader<'a>,
    /// How to format the clause header
    header_formatter: Argument<'a, PyFormatContext<'ast>>,
    /// Leading comments coming before the branch, together with the previous node, if any. Only relevant
    /// for alternate branches.
    leading_comments: Option<(&'a [SourceComment], Option<AnyNodeRef<'a>>)>,
    /// The trailing comments coming after the colon.
    trailing_colon_comment: &'a [SourceComment],
    body: &'a Suite,
    kind: SuiteKind,
}

impl<'a, 'ast> FormatClause<'a, 'ast> {
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

    fn clause_header(&self) -> FormatClauseHeader<'a, 'ast> {
        FormatClauseHeader {
            header: self.header,
            formatter: self.header_formatter,
            leading_comments: self.leading_comments,
            trailing_colon_comment: self.trailing_colon_comment,
        }
    }

    fn clause_body(&self) -> FormatClauseBody<'a> {
        clause_body(self.body, self.kind, self.trailing_colon_comment)
    }
}

/// Formats a clause, handling the case where the compound
/// statement lies on a single line with `# fmt: skip` and
/// should be suppressed.
pub(crate) fn clause<'a, 'ast, Content>(
    header: ClauseHeader<'a>,
    header_formatter: &'a Content,
    trailing_colon_comment: &'a [SourceComment],
    body: &'a Suite,
    kind: SuiteKind,
) -> FormatClause<'a, 'ast>
where
    Content: Format<PyFormatContext<'ast>>,
{
    FormatClause {
        header,
        header_formatter: Argument::new(header_formatter),
        leading_comments: None,
        trailing_colon_comment,
        body,
        kind,
    }
}

impl<'ast> Format<PyFormatContext<'ast>> for FormatClause<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        match should_suppress_clause(self, f)? {
            SuppressClauseHeader::Yes {
                last_child_in_clause,
            } => write_suppressed_clause(self, f, last_child_in_clause),
            SuppressClauseHeader::No => {
                write!(f, [self.clause_header(), self.clause_body()])
            }
        }
    }
}

/// Finds the range of `keyword` starting the search at `start_position`.
///
/// If the start position is at the end of the previous statement, the
/// search will skip the optional semi-colon at the end of that statement.
/// Other than this, we expect only trivia between the `start_position`
/// and the keyword.
fn find_keyword(
    start_position: StartPosition,
    keyword: SimpleTokenKind,
    source: &str,
) -> FormatResult<TextRange> {
    let next_token = match start_position {
        StartPosition::ClauseStart(text_size) => SimpleTokenizer::starts_at(text_size, source)
            .skip_trivia()
            .next(),
        StartPosition::LastStatement(text_size) => {
            let mut tokenizer = SimpleTokenizer::starts_at(text_size, source).skip_trivia();

            let mut token = tokenizer.next();

            // If the last statement ends with a semi-colon, skip it.
            if matches!(
                token,
                Some(SimpleToken {
                    kind: SimpleTokenKind::Semi,
                    ..
                })
            ) {
                token = tokenizer.next();
            }
            token
        }
    };

    match next_token {
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

/// Offset directly before clause header.
///
/// Can either be the beginning of the clause header
/// or the end of the last statement preceding the clause.
#[derive(Clone, Copy)]
enum StartPosition {
    /// The beginning of a clause header
    ClauseStart(TextSize),
    /// The end of the last statement in the suite preceding a clause.
    ///
    /// For example:
    /// ```python
    /// if cond:
    ///     a
    ///     b
    ///     c;
    /// # ...^here
    /// else:
    ///     d
    /// ```
    LastStatement(TextSize),
}

impl StartPosition {
    fn clause_start(ranged: impl Ranged) -> Self {
        Self::ClauseStart(ranged.start())
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
            debug_assert!(
                false,
                "Expected the colon marking the end of the case header but found {token:?} instead."
            );
            Err(FormatError::syntax_error(
                "Expected colon marking the end of the case header but found another token instead.",
            ))
        }
        None => {
            debug_assert!(
                false,
                "Expected the colon marking the end of the case header but found the end of the range."
            );
            Err(FormatError::syntax_error(
                "Expected the colon marking the end of the case header but found the end of the range.",
            ))
        }
    }
}

fn should_suppress_clause<'a>(
    clause: &FormatClause<'a, '_>,
    f: &mut Formatter<PyFormatContext<'_>>,
) -> FormatResult<SuppressClauseHeader<'a>> {
    let source = f.context().source();

    let Some(last_child_in_clause) = clause.header.last_child_in_clause() else {
        return Ok(SuppressClauseHeader::No);
    };

    // Early return if we don't have a skip comment
    // to avoid computing header range in the common case
    if !has_skip_comment(
        f.context().comments().trailing(last_child_in_clause),
        source,
    ) {
        return Ok(SuppressClauseHeader::No);
    }

    let clause_start = clause.header.range(source)?.end();

    let clause_range = TextRange::new(clause_start, last_child_in_clause.end());

    // Only applies to clauses on a single line
    if source.contains_line_break(clause_range) {
        return Ok(SuppressClauseHeader::No);
    }

    Ok(SuppressClauseHeader::Yes {
        last_child_in_clause,
    })
}

#[cold]
fn write_suppressed_clause(
    clause: &FormatClause,
    f: &mut Formatter<PyFormatContext<'_>>,
    last_child_in_clause: AnyNodeRef,
) -> FormatResult<()> {
    if let Some((leading_comments, last_node)) = clause.leading_comments {
        leading_alternate_branch_comments(leading_comments, last_node).fmt(f)?;
    }

    let header = clause.header;
    let clause_start = header.first_keyword_range(f.context().source())?.start();

    let comments = f.context().comments().clone();

    let clause_end = last_child_in_clause.end();

    // Write the outer comments and format the node as verbatim
    write!(
        f,
        [
            source_position(clause_start),
            verbatim_text(TextRange::new(clause_start, clause_end)),
            source_position(clause_end),
            trailing_comments(comments.trailing(last_child_in_clause)),
            hard_line_break()
        ]
    )?;

    // We mark comments in the header as formatted as in
    // the implementation of [`write_suppressed_clause_header`].
    //
    // Note that the header may be multi-line and contain
    // various comments since we only require that the range
    // starting at the _colon_ and ending at the `# fmt: skip`
    // fits on one line.
    header.visit(&mut |child| {
        for comment in comments.leading_trailing(child) {
            comment.mark_formatted();
        }
        comments.mark_verbatim_node_comments_formatted(child);
    });

    // Similarly we mark the comments in the body as formatted.
    // Note that the trailing comments for the last child in the
    // body have already been handled above.
    for stmt in clause.body {
        comments.mark_verbatim_node_comments_formatted(stmt.into());
    }

    Ok(())
}

enum SuppressClauseHeader<'a> {
    No,
    Yes {
        last_child_in_clause: AnyNodeRef<'a>,
    },
}
