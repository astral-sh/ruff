mod add_ignore;
mod parser;
mod unused;

use smallvec::SmallVec;
use std::fmt;

use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, IntoDiagnosticMessage, LintName, Severity, Span,
};
use ruff_db::{files::File, parsed::parsed_module, source::source_text};
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_python_trivia::indentation_at_offset;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::diagnostic::DiagnosticGuard;
use crate::lint::{GetLintError, Level, LintMetadata, LintRegistry, LintStatus};
pub use crate::suppression::add_ignore::{SuppressFix, suppress_all, suppress_single};
use crate::suppression::parser::{
    ParseError, ParseErrorKind, SuppressionComment, SuppressionParser,
};
use crate::suppression::unused::check_unused_suppressions;
use crate::types::TypeCheckDiagnostics;
use crate::{Db, declare_lint, lint::LintId};

declare_lint! {
    #[doc = include_str!("../resources/lint_docs/unused-ignore-comment.md")]
    pub static UNUSED_IGNORE_COMMENT = {
        summary: "detects unused `ty: ignore` comments",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../resources/lint_docs/unused-type-ignore-comment.md")]
    pub(crate) static UNUSED_TYPE_IGNORE_COMMENT = {
        summary: "detects unused `type: ignore` comments",
        status: LintStatus::stable("0.0.14"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../resources/lint_docs/ignore-comment-unknown-rule.md")]
    pub(crate) static IGNORE_COMMENT_UNKNOWN_RULE = {
        summary: "detects `ty: ignore` comments that reference unknown rules",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../resources/lint_docs/invalid-ignore-comment.md")]
    pub(crate) static INVALID_IGNORE_COMMENT = {
        summary: "detects ignore comments that use invalid syntax",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../resources/lint_docs/blanket-ignore-comment.md")]
    pub(crate) static BLANKET_IGNORE_COMMENT = {
        summary: "detects blanket `ty: ignore` comments",
        status: LintStatus::stable("0.0.57"),
        default_level: Level::Ignore,
    }
}

pub fn is_unused_ignore_comment_lint(name: LintName) -> bool {
    name == UNUSED_IGNORE_COMMENT.name() || name == UNUSED_TYPE_IGNORE_COMMENT.name()
}

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn suppressions(db: &dyn Db, file: File) -> Suppressions {
    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, file);

    let respect_type_ignore = db.analysis_settings(file).respect_type_ignore_comments;

    let mut builder = SuppressionsBuilder::new(&source, db.lint_registry());
    let mut line_start = TextSize::default();

    for token in parsed.tokens() {
        if !token.kind().is_trivia() {
            builder.set_seen_non_trivia_token();
        }

        match token.kind() {
            TokenKind::Comment => {
                let parser = SuppressionParser::new(&source, token.range());

                for comment in parser {
                    match comment {
                        Ok(comment) => {
                            if comment.kind().is_type_ignore() && !respect_type_ignore {
                                continue;
                            }
                            builder.add_comment(
                                comment,
                                TextRange::new(line_start, token.end()),
                                parsed.tokens(),
                            );
                        }
                        Err(error) => match error.kind {
                            ParseErrorKind::NotASuppression
                            | ParseErrorKind::CommentWithoutHash => {
                                // Skip non suppression comments and comments that miss a hash (shouldn't ever happen)
                            }
                            ParseErrorKind::NoWhitespaceAfterIgnore(kind)
                            | ParseErrorKind::CodesMissingComma(kind)
                            | ParseErrorKind::InvalidCode(kind)
                            | ParseErrorKind::CodesMissingClosingBracket(kind) => {
                                if kind.is_type_ignore() && !respect_type_ignore {
                                    continue;
                                }

                                builder.add_invalid_comment(kind, error);
                            }
                        },
                    }
                }
            }
            TokenKind::Newline | TokenKind::NonLogicalNewline => {
                line_start = token.end();
            }
            _ => {}
        }
    }

    builder.finish()
}

pub(crate) fn check_suppressions(
    db: &dyn Db,
    file: File,
    diagnostics: TypeCheckDiagnostics,
) -> Vec<Diagnostic> {
    let mut context = CheckSuppressionsContext::new(db, file, diagnostics);

    check_unknown_rule(&mut context);
    check_invalid_suppression(&mut context);
    check_blanket_suppressions(&mut context);
    check_unused_suppressions(&mut context);

    context.diagnostics.into_inner().into_diagnostics()
}

fn check_blanket_suppressions(context: &mut CheckSuppressionsContext) {
    if context.is_lint_disabled(&BLANKET_IGNORE_COMMENT) {
        return;
    }

    for suppression in context.suppressions.iter().filter(|suppression| {
        suppression.kind == SuppressionKind::Ty && suppression.target == SuppressionTarget::All
    }) {
        // A blanket suppression cannot suppress its own diagnostic, but a code-specific
        // suppression can.
        if let Some(code_suppression) = context
            .suppressions
            .lint_suppressions(suppression.range, LintId::of(&BLANKET_IGNORE_COMMENT))
            .find(|candidate| candidate.target.is_lint())
        {
            context
                .diagnostics
                .borrow_mut()
                .mark_used(code_suppression.id());
        } else if let Some(diag) =
            context.report_unchecked(&BLANKET_IGNORE_COMMENT, suppression.range)
        {
            diag.into_diagnostic("Use specific rule codes in `ty: ignore`");
        }
    }
}

/// Checks for `ty: ignore` and `type: ignore[ty:<code>]` comments that reference unknown rules.
fn check_unknown_rule(context: &mut CheckSuppressionsContext) {
    if context.is_lint_disabled(&IGNORE_COMMENT_UNKNOWN_RULE) {
        return;
    }

    for unknown in &context.suppressions.unknown {
        if let Some(diag) = context.report_lint(&IGNORE_COMMENT_UNKNOWN_RULE, unknown.range) {
            diag.into_diagnostic(&unknown.reason);
        }
    }
}

fn check_invalid_suppression(context: &mut CheckSuppressionsContext) {
    if context.is_lint_disabled(&INVALID_IGNORE_COMMENT) {
        return;
    }

    for invalid in &context.suppressions.invalid {
        if let Some(diag) = context.report_lint(&INVALID_IGNORE_COMMENT, invalid.error.range) {
            diag.into_diagnostic(format_args!(
                "Invalid `{kind}` comment: {reason}",
                kind = invalid.kind,
                reason = &invalid.error
            ));
        }
    }
}

struct CheckSuppressionsContext<'a> {
    db: &'a dyn Db,
    file: File,
    suppressions: &'a Suppressions,
    diagnostics: std::cell::RefCell<TypeCheckDiagnostics>,
}

impl<'a> CheckSuppressionsContext<'a> {
    fn new(db: &'a dyn Db, file: File, diagnostics: TypeCheckDiagnostics) -> Self {
        let suppressions = suppressions(db, file);
        Self {
            db,
            file,
            suppressions,
            diagnostics: diagnostics.into(),
        }
    }

    fn is_lint_disabled(&self, lint: &'static LintMetadata) -> bool {
        !self
            .db
            .rule_selection(self.file)
            .is_enabled(LintId::of(lint))
    }

    fn is_suppression_used(&self, id: FileSuppressionId) -> bool {
        self.diagnostics.borrow().is_used(id)
    }

    fn report_lint<'ctx>(
        &'ctx self,
        lint: &'static LintMetadata,
        range: TextRange,
    ) -> Option<SuppressionDiagnosticGuardBuilder<'ctx, 'a>> {
        if let Some(suppression) = self.suppressions.find_suppression(range, LintId::of(lint)) {
            self.diagnostics.borrow_mut().mark_used(suppression.id());
            return None;
        }

        self.report_unchecked(lint, range)
    }

    /// Reports a diagnostic without checking if the lint at the given range is suppressed or marking
    /// the suppression as used.
    fn report_unchecked<'ctx>(
        &'ctx self,
        lint: &'static LintMetadata,
        range: TextRange,
    ) -> Option<SuppressionDiagnosticGuardBuilder<'ctx, 'a>> {
        SuppressionDiagnosticGuardBuilder::new(self, lint, range)
    }
}

/// A builder for constructing a diagnostic guard.
///
/// This type exists to separate the phases of "check if a diagnostic should
/// be reported" and "build the actual diagnostic."
pub(crate) struct SuppressionDiagnosticGuardBuilder<'ctx, 'db> {
    ctx: &'ctx CheckSuppressionsContext<'db>,
    id: DiagnosticId,
    range: TextRange,
    severity: Severity,
}

impl<'ctx, 'db> SuppressionDiagnosticGuardBuilder<'ctx, 'db> {
    fn new(
        ctx: &'ctx CheckSuppressionsContext<'db>,
        lint: &'static LintMetadata,
        range: TextRange,
    ) -> Option<Self> {
        let severity = ctx.db.rule_selection(ctx.file).severity(LintId::of(lint))?;

        Some(Self {
            ctx,
            id: DiagnosticId::Lint(lint.name()),
            severity,
            range,
        })
    }

    /// Create a new guard.
    ///
    /// This initializes a new diagnostic using the given message along with
    /// the ID and severity used to create this builder.
    ///
    /// The diagnostic can be further mutated on the guard via its `DerefMut`
    /// impl to `Diagnostic`.
    pub(crate) fn into_diagnostic(
        self,
        message: impl IntoDiagnosticMessage,
    ) -> DiagnosticGuard<'ctx> {
        let mut diag = Diagnostic::new(self.id, self.severity, message);

        let primary_span = Span::from(self.ctx.file).with_range(self.range);
        diag.annotate(Annotation::primary(primary_span));
        DiagnosticGuard::new(self.ctx.file, &self.ctx.diagnostics, diag)
    }
}

/// The suppressions of a single file.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize)]
pub(crate) struct Suppressions {
    /// Suppressions that apply to the entire file.
    ///
    /// The suppressions are sorted by [`Suppression::comment_range`] and the [`Suppression::suppressed_range`]
    /// spans the entire file.
    file: SmallVec<[Suppression; 1]>,

    /// Suppressions that apply to a specific line (or lines).
    ///
    /// Comments with multiple codes create multiple [`Suppression`]s that all share the same [`Suppression::comment_range`].
    ///
    /// The suppressions are sorted by [`Suppression::range`] (which implies
    /// [`Suppression::comment_range`]) and [`Suppression::suppressed_range`] start.
    line: Vec<Suppression>,

    /// Suppressions with lint codes that are unknown.
    unknown: Vec<UnknownSuppression>,

    /// Suppressions that are syntactically invalid.
    invalid: Vec<InvalidSuppression>,
}

impl Suppressions {
    pub(crate) fn find_suppression(&self, range: TextRange, id: LintId) -> Option<&Suppression> {
        self.lint_suppressions(range, id).next()
    }

    /// Returns all suppressions for the given lint
    fn lint_suppressions(
        &self,
        range: TextRange,
        id: LintId,
    ) -> impl Iterator<Item = &Suppression> + '_ {
        self.file
            .iter()
            .chain(self.line_suppressions(range))
            .filter(move |suppression| suppression.matches(id))
    }

    /// Returns the line-level suppressions that apply for `range`.
    ///
    /// A suppression applies for the given range if it contains the range's start or end offset.
    /// End-of-line suppressions cover the diagnostic's start or end line, while own-line
    /// suppressions cover the following logical line.
    fn line_suppressions(&self, range: TextRange) -> impl Iterator<Item = &Suppression> + '_ {
        // Suppression ranges are ordered by their start, so suppressions after this index cannot
        // contain either boundary of the diagnostic range.
        let end = self
            .line
            .partition_point(|suppression| suppression.suppressed_range.start() <= range.end());

        // Search the potentially overlapping suppression comments for one that contains the
        // range's start or end offset.
        self.line[..end].iter().filter(move |suppression| {
            // Don't use intersect to avoid that suppressions on inner-expression
            // ignore errors for outer expressions
            suppression.suppressed_range.contains(range.start())
                || suppression.suppressed_range.contains_inclusive(range.end())
        })
    }

    fn iter(&self) -> SuppressionsIter<'_> {
        self.file.iter().chain(&self.line)
    }
}

pub(crate) type SuppressionsIter<'a> =
    std::iter::Chain<std::slice::Iter<'a, Suppression>, std::slice::Iter<'a, Suppression>>;

impl<'a> IntoIterator for &'a Suppressions {
    type Item = &'a Suppression;
    type IntoIter = SuppressionsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A `type: ignore` or `ty: ignore` suppression.
///
/// Suppression comments that suppress multiple codes
/// create multiple suppressions: one for every code.
/// They all share the same `comment_range`.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub(crate) struct Suppression {
    target: SuppressionTarget,
    kind: SuppressionKind,

    /// The range of the code in this suppression.
    ///
    /// This is the same as the `comment_range` for the
    /// targets [`SuppressionTarget::All`] and [`SuppressionTarget::Empty`].
    range: TextRange,

    /// The range of the suppression comment.
    ///
    /// This isn't the range of the entire comment if this is a nested comment:
    ///
    /// ```py
    /// a # ty: ignore # fmt: off
    ///   ^^^^^^^^^^^^^
    /// ```
    ///
    /// It doesn't include the range of the nested `# fmt: off` comment.
    comment_range: TextRange,

    /// The range for which this suppression applies.
    /// Most of the time, this is the range of the comment's line. An own-line `ty: ignore`
    /// suppression also covers the following logical line.
    /// However, there are few cases where the range gets expanded to
    /// cover multiple lines:
    /// * multiline strings: `expr + """multiline\nstring"""  # type: ignore`
    /// * line continuations: `expr \ + "test"  # type: ignore`
    suppressed_range: TextRange,
}

impl Suppression {
    fn matches(&self, tested_id: LintId) -> bool {
        match self.target {
            SuppressionTarget::All => true,
            SuppressionTarget::Lint(suppressed_id) => tested_id == suppressed_id,
            SuppressionTarget::Empty => false,
        }
    }

    pub(crate) fn id(&self) -> FileSuppressionId {
        FileSuppressionId(self.range)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
enum SuppressionKind {
    TypeIgnore,
    Ty,
}

impl SuppressionKind {
    const fn is_type_ignore(self) -> bool {
        matches!(self, SuppressionKind::TypeIgnore)
    }

    fn len_utf8(self) -> usize {
        match self {
            SuppressionKind::TypeIgnore => "type".len(),
            SuppressionKind::Ty => "ty".len(),
        }
    }
}

impl fmt::Display for SuppressionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SuppressionKind::TypeIgnore => f.write_str("type: ignore"),
            SuppressionKind::Ty => f.write_str("ty: ignore"),
        }
    }
}

/// Unique ID for a suppression in a file.
///
/// ## Implementation
/// The wrapped `TextRange` is the suppression's range.
/// This is unique enough because it is its exact
/// location in the source.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub(crate) struct FileSuppressionId(TextRange);

#[derive(Copy, Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
enum SuppressionTarget {
    /// Suppress all lints
    All,

    /// Suppress the lint with the given id
    Lint(LintId),

    /// Suppresses no lint, e.g. `ty: ignore[]`
    Empty,
}

impl SuppressionTarget {
    const fn is_lint(self) -> bool {
        matches!(self, SuppressionTarget::Lint(_))
    }
}

struct SuppressionsBuilder<'a> {
    lint_registry: &'a LintRegistry,
    source: &'a str,

    /// Ignore comments at the top of the file before any non-trivia code apply to the entire file.
    /// This boolean tracks if there has been any non trivia token.
    seen_non_trivia_token: bool,

    line: Vec<Suppression>,
    file: SmallVec<[Suppression; 1]>,
    unknown: Vec<UnknownSuppression>,
    invalid: Vec<InvalidSuppression>,
}

impl<'a> SuppressionsBuilder<'a> {
    fn new(source: &'a str, lint_registry: &'a LintRegistry) -> Self {
        Self {
            source,
            lint_registry,
            seen_non_trivia_token: false,
            line: Vec::new(),
            file: SmallVec::new_const(),
            unknown: Vec::new(),
            invalid: Vec::new(),
        }
    }

    fn set_seen_non_trivia_token(&mut self) {
        self.seen_non_trivia_token = true;
    }

    fn finish(mut self) -> Suppressions {
        self.line.shrink_to_fit();
        self.file.shrink_to_fit();
        self.unknown.shrink_to_fit();
        self.invalid.shrink_to_fit();

        Suppressions {
            file: self.file,
            line: self.line,
            unknown: self.unknown,
            invalid: self.invalid,
        }
    }

    #[expect(clippy::needless_pass_by_value)]
    fn add_comment(&mut self, comment: SuppressionComment, line_range: TextRange, tokens: &Tokens) {
        // ignore comments at the start of the file apply to the entire range.
        // > A # type: ignore comment on a line by itself at the top of a file, before any docstrings,
        // > imports, or other executable code, silences all errors in the file.
        // > Blank lines and other comments, such as shebang lines and coding cookies,
        // > may precede the # type: ignore comment.
        // > https://typing.python.org/en/latest/spec/directives.html#type-ignore-comments
        let is_file_suppression = !self.seen_non_trivia_token;

        let suppressed_range = if is_file_suppression {
            TextRange::new(0.into(), self.source.text_len())
        } else if !comment.kind().is_type_ignore()
            && indentation_at_offset(comment.range().start(), self.source).is_some()
        {
            let (before, after) = tokens.split_at(comment.range().start());
            own_line_suppression_range(comment.range(), before, after)
        } else {
            line_range
        };

        let mut push_ignore_suppression = |suppression: Suppression| {
            if is_file_suppression {
                self.file.push(suppression);
            } else {
                self.line.push(suppression);
            }
        };

        match comment.codes() {
            // `type: ignore`
            None => {
                push_ignore_suppression(Suppression {
                    target: SuppressionTarget::All,
                    kind: comment.kind(),
                    comment_range: comment.range(),
                    range: comment.range(),
                    suppressed_range,
                });
            }

            // `ty: ignore[]` or `type: ignore[]`
            Some([]) => {
                push_ignore_suppression(Suppression {
                    target: SuppressionTarget::Empty,
                    kind: comment.kind(),
                    range: comment.range(),
                    comment_range: comment.range(),
                    suppressed_range,
                });
            }

            // `ty: ignore[a, b]` or `type: ignore[a, b]`
            Some(codes) => {
                for &code_range in codes {
                    let code = &self.source[code_range];

                    // For `type:ignore`, ignore codes that don't start with `ty:`.
                    let code = if comment.kind().is_type_ignore() {
                        if let Some(prefix) = code.strip_prefix("ty:") {
                            prefix
                        } else {
                            continue;
                        }
                    } else {
                        code
                    };

                    match self.lint_registry.get(code) {
                        Ok(lint) => {
                            push_ignore_suppression(Suppression {
                                target: SuppressionTarget::Lint(lint),
                                kind: comment.kind(),
                                range: code_range,
                                comment_range: comment.range(),
                                suppressed_range,
                            });
                        }
                        Err(error) => self.unknown.push(UnknownSuppression {
                            range: code_range,
                            comment_range: comment.range(),
                            reason: error,
                        }),
                    }
                }
            }
        }
    }

    fn add_invalid_comment(&mut self, kind: SuppressionKind, error: ParseError) {
        self.invalid.push(InvalidSuppression { kind, error });
    }
}

/// Returns the range covered by an own-line suppression comment.
///
/// A suppression before a logical line covers the entire logical line. A suppression inside a
/// multiline logical line covers the next non-comment physical line instead. This matches Ruff's
/// own-line suppression behavior.
fn own_line_suppression_range(range: TextRange, before: &[Token], after: &[Token]) -> TextRange {
    let mut end = range.end();
    let is_inner_comment = before.iter().rev().find_map(|token| match token.kind() {
        TokenKind::Newline => Some(false),
        TokenKind::NonLogicalNewline | TokenKind::Comment => None,
        _ => Some(true),
    });

    let is_inner_comment = is_inner_comment.unwrap_or(false);
    let mut is_blank_or_comment_only = true;
    let mut seen_nonlogical_newline = false;

    for token in after {
        match token.kind() {
            TokenKind::Newline => {
                end = token.start();
                break;
            }
            TokenKind::Comment => {}
            TokenKind::NonLogicalNewline if is_inner_comment => {
                end = token.start();
                if seen_nonlogical_newline && !is_blank_or_comment_only {
                    break;
                }
                seen_nonlogical_newline = true;
                is_blank_or_comment_only = true;
            }
            _ => {
                is_blank_or_comment_only = false;
                end = token.end();
            }
        }
    }

    TextRange::new(range.start(), end)
}

/// Suppression for an unknown lint rule.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct UnknownSuppression {
    /// The range of the code.
    range: TextRange,

    /// The range of the suppression comment
    comment_range: TextRange,

    reason: GetLintError,
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct InvalidSuppression {
    kind: SuppressionKind,
    error: ParseError,
}
