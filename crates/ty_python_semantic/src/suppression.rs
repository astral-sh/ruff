mod add_ignore;
mod parser;
mod unused;

use smallvec::SmallVec;
use std::fmt;

use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, IntoDiagnosticMessage, Severity, Span,
};
use ruff_db::{files::File, parsed::parsed_module, source::source_text};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::diagnostic::DiagnosticGuard;
use crate::lint::{GetLintError, Level, LintMetadata, LintRegistry, LintStatus};
pub use crate::suppression::add_ignore::create_suppression_fix;
use crate::suppression::parser::{
    ParseError, ParseErrorKind, SuppressionComment, SuppressionParser,
};
use crate::suppression::unused::check_unused_suppressions;
use crate::types::TypeCheckDiagnostics;
use crate::{Db, declare_lint, lint::LintId};

declare_lint! {
    /// ## What it does
    /// Checks for `type: ignore` or `ty: ignore` directives that are no longer applicable.
    ///
    /// ## Why is this bad?
    /// A `type: ignore` directive that no longer matches any diagnostic violations is likely
    /// included by mistake, and should be removed to avoid confusion.
    ///
    /// ## Examples
    /// ```py
    /// a = 20 / 2  # ty: ignore[division-by-zero]
    /// ```
    ///
    /// Use instead:
    ///
    /// ```py
    /// a = 20 / 2
    /// ```
    pub(crate) static UNUSED_IGNORE_COMMENT = {
        summary: "detects unused `type: ignore` comments",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for `ty: ignore[code]` where `code` isn't a known lint rule.
    ///
    /// ## Why is this bad?
    /// A `ty: ignore[code]` directive with a `code` that doesn't match
    /// any known rule will not suppress any type errors, and is probably a mistake.
    ///
    /// ## Examples
    /// ```py
    /// a = 20 / 0  # ty: ignore[division-by-zer]
    /// ```
    ///
    /// Use instead:
    ///
    /// ```py
    /// a = 20 / 0  # ty: ignore[division-by-zero]
    /// ```
    pub(crate) static IGNORE_COMMENT_UNKNOWN_RULE = {
        summary: "detects `ty: ignore` comments that reference unknown rules",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for `type: ignore` and `ty: ignore` comments that are syntactically incorrect.
    ///
    /// ## Why is this bad?
    /// A syntactically incorrect ignore comment is probably a mistake and is useless.
    ///
    /// ## Examples
    /// ```py
    /// a = 20 / 0  # type: ignoree
    /// ```
    ///
    /// Use instead:
    ///
    /// ```py
    /// a = 20 / 0  # type: ignore
    /// ```
    pub(crate) static INVALID_IGNORE_COMMENT = {
        summary: "detects ignore comments that use invalid syntax",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn suppressions(db: &dyn Db, file: File) -> Suppressions {
    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, file);

    let respect_type_ignore = db.analysis_settings().respect_type_ignore_comments;

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
                            builder.add_comment(comment, TextRange::new(line_start, token.end()));
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
    check_unused_suppressions(&mut context);

    context.diagnostics.into_inner().into_diagnostics()
}

/// Checks for `ty: ignore` comments that reference unknown rules.
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
    ///
    /// For now, this is limited to `type: ignore` comments.
    file: SmallVec<[Suppression; 1]>,

    /// Suppressions that apply to a specific line (or lines).
    ///
    /// Comments with multiple codes create multiple [`Suppression`]s that all share the same [`Suppression::comment_range`].
    ///
    /// The suppressions are sorted by [`Suppression::range`] (which implies [`Suppression::comment_range`]).
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
    /// A suppression applies for the given range if it contains the range's
    /// start or end offset. This means the suppression is on the same line
    /// as the diagnostic's start or end.
    fn line_suppressions(&self, range: TextRange) -> impl Iterator<Item = &Suppression> + '_ {
        // First find the index of the suppression comment that ends right before the range
        // starts. This allows us to skip suppressions that are not relevant for the range.
        let end_offset = self
            .line
            .binary_search_by_key(&range.start(), |suppression| {
                suppression.suppressed_range.end()
            })
            .unwrap_or_else(|index| index);

        // From here, search the remaining suppression comments for one that
        // contains the range's start or end offset. Stop the search
        // as soon as the suppression's range and the range no longer overlap.
        self.line[end_offset..]
            .iter()
            // Stop searching if the suppression starts after the range we're looking for.
            .take_while(move |suppression| range.end() >= suppression.suppressed_range.start())
            .filter(move |suppression| {
                // Don't use intersect to avoid that suppressions on inner-expression
                // ignore errors for outer expressions
                suppression.suppressed_range.contains(range.start())
                    || suppression.suppressed_range.contains(range.end())
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
    /// Most of the time, this is the range of the comment's line.
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

    /// `type: ignore` comments at the top of the file before any non-trivia code apply to the entire file.
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
    fn add_comment(&mut self, comment: SuppressionComment, line_range: TextRange) {
        // `type: ignore` comments at the start of the file apply to the entire range.
        // > A # type: ignore comment on a line by itself at the top of a file, before any docstrings,
        // > imports, or other executable code, silences all errors in the file.
        // > Blank lines and other comments, such as shebang lines and coding cookies,
        // > may precede the # type: ignore comment.
        // > https://typing.python.org/en/latest/spec/directives.html#type-ignore-comments
        let is_file_suppression = comment.kind().is_type_ignore() && !self.seen_non_trivia_token;

        let suppressed_range = if is_file_suppression {
            TextRange::new(0.into(), self.source.text_len())
        } else {
            line_range
        };

        let mut push_type_ignore_suppression = |suppression: Suppression| {
            if is_file_suppression {
                self.file.push(suppression);
            } else {
                self.line.push(suppression);
            }
        };

        match comment.codes() {
            // `type: ignore`
            None => {
                push_type_ignore_suppression(Suppression {
                    target: SuppressionTarget::All,
                    kind: comment.kind(),
                    comment_range: comment.range(),
                    range: comment.range(),
                    suppressed_range,
                });
            }

            // `type: ignore[..]`
            // The suppression applies to all lints if it is a `type: ignore`
            // comment. `type: ignore` apply to all lints for better mypy compatibility.
            Some(_) if comment.kind().is_type_ignore() => {
                push_type_ignore_suppression(Suppression {
                    target: SuppressionTarget::All,
                    kind: comment.kind(),
                    comment_range: comment.range(),
                    range: comment.range(),
                    suppressed_range,
                });
            }

            // `ty: ignore[]`
            Some([]) => {
                self.line.push(Suppression {
                    target: SuppressionTarget::Empty,
                    kind: comment.kind(),
                    range: comment.range(),
                    comment_range: comment.range(),
                    suppressed_range,
                });
            }

            // `ty: ignore[a, b]`
            Some(codes) => {
                for &code_range in codes {
                    let code = &self.source[code_range];

                    match self.lint_registry.get(code) {
                        Ok(lint) => {
                            self.line.push(Suppression {
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
