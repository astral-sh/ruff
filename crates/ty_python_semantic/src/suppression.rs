mod add_ignore;
mod parser;
mod unused;

use smallvec::{SmallVec, smallvec};
use std::fmt;
use std::hash::{Hash, Hasher};

use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, IntoDiagnosticMessage, LintName, Severity, Span,
};
use ruff_db::{PythonFile, files::File, parsed::parsed_module, source::source_text};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_trivia::indentation_at_offset;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use rustc_hash::FxHasher;

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
pub(crate) fn suppressions(db: &dyn Db, file: PythonFile<'_>) -> Suppressions {
    let source_file = file.file(db);
    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, source_file);

    let respect_type_ignore = db
        .analysis_settings(source_file)
        .respect_type_ignore_comments;

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
    file: PythonFile<'_>,
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
        // A blanket suppression cannot suppress its own diagnostic, but a lint-specific
        // suppression can.
        if let Some(lint_suppression) = select_preferred_suppression(
            context
                .suppressions
                .lint_suppressions(suppression.range, LintId::of(&BLANKET_IGNORE_COMMENT))
                .filter(|candidate| candidate.target.is_lint()),
            suppression.range,
        ) {
            context
                .diagnostics
                .borrow_mut()
                .mark_used(lint_suppression.id());
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
                reason = invalid.error
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
    fn new(db: &'a dyn Db, file: PythonFile<'a>, diagnostics: TypeCheckDiagnostics) -> Self {
        let suppressions = suppressions(db, file);
        Self {
            db,
            file: file.file(db),
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

    /// Suppressions that apply inline rather than to the entire file.
    ///
    /// Comments with multiple codes create multiple [`Suppression`]s that all share the same [`Suppression::comment_range`].
    ///
    /// The suppressions are indexed by [`Suppression::suppressed_range`] and retain source order.
    /// Their range ends aren't necessarily sorted because own-line suppressions can be nested:
    ///
    /// ```py
    /// # ty: ignore
    /// value = (
    ///     # ty: ignore
    ///     missing
    /// )
    /// ```
    ///
    /// The outer suppression starts before the inner suppression but ends after it.
    inline: IntervalIndex,

    /// Suppressions with lint codes that are unknown.
    unknown: Vec<UnknownSuppression>,

    /// Suppressions that are syntactically invalid.
    invalid: Vec<InvalidSuppression>,
}

impl Suppressions {
    /// Returns the suppression that takes precedence for the diagnostic `range` and lint `id`.
    ///
    /// Nested suppression ranges prefer the innermost candidate. If a diagnostic spans multiple
    /// physical lines and separate suppressions cover its opening and closing lines, the
    /// opening-line suppression retains precedence.
    pub(crate) fn find_suppression(&self, range: TextRange, id: LintId) -> Option<&Suppression> {
        select_preferred_suppression(self.lint_suppressions(range, id), range)
    }

    /// Returns applicable suppressions for `id`, with inline suppressions in reverse source order.
    fn lint_suppressions(
        &self,
        range: TextRange,
        id: LintId,
    ) -> impl Iterator<Item = &Suppression> + '_ {
        self.file
            .iter()
            .chain(self.inline.intersecting_rev(
                range,
                SuppressionTarget::All.target_mask() | SuppressionTarget::Lint(id).target_mask(),
            ))
            .filter(move |suppression| suppression.matches(id) && suppression.applies_to(range))
    }

    /// Returns applicable comments whose targets allow `--add-ignore` to append a code, in
    /// reverse source order.
    ///
    /// The interval index filters out blanket suppressions. Comments with multiple codes can
    /// appear more than once, and callers must still exclude comments with trailing reasons.
    fn editable_inline_suppressions_rev(
        &self,
        range: TextRange,
    ) -> impl Iterator<Item = &Suppression> + '_ {
        self.inline
            .intersecting_rev(range, IntervalIndex::EDITABLE_MASK)
            .filter(move |suppression| {
                matches!(
                    suppression.target,
                    SuppressionTarget::Lint(_) | SuppressionTarget::Empty
                ) && suppression.applies_to(range)
            })
    }

    fn iter(&self) -> impl Iterator<Item = &Suppression> {
        self.file.iter().chain(self.inline.iter())
    }
}

/// Selects between applicable suppressions yielded in reverse source order.
///
/// Candidates covering the same endpoint are nested, so the first (innermost) candidate wins. For
/// a diagnostic spanning multiple physical lines, however, a later candidate may cover only its
/// closing line while a separate earlier candidate covers its opening line. The closing-line
/// candidate wins only when its suppression range is nested within the opening-line candidate's
/// range; otherwise the opening-line candidate retains precedence.
fn select_preferred_suppression<'a>(
    mut candidates: impl Iterator<Item = &'a Suppression>,
    diagnostic_range: TextRange,
) -> Option<&'a Suppression> {
    let end_candidate = candidates.next()?;
    let diagnostic_start = diagnostic_range.start();

    if end_candidate.suppressed_range.contains(diagnostic_start) {
        return Some(end_candidate);
    }

    let start_candidate =
        candidates.find(|candidate| candidate.suppressed_range.contains(diagnostic_start));

    match start_candidate {
        Some(start_candidate)
            if start_candidate
                .suppressed_range
                .contains_range(end_candidate.suppressed_range) =>
        {
            Some(end_candidate)
        }
        Some(start_candidate) => Some(start_candidate),
        None => Some(end_candidate),
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
    /// Returns whether this suppression covers either endpoint of `range`.
    ///
    /// Requiring endpoint containment, rather than any intersection, prevents a suppression on an
    /// inner expression from suppressing a diagnostic for an enclosing expression.
    fn applies_to(&self, range: TextRange) -> bool {
        self.suppressed_range.contains(range.start())
            || self.suppressed_range.contains_inclusive(range.end())
    }

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

    /// Returns the conservative bit used to skip subtrees without this target.
    ///
    /// Lints are hashed into 62 buckets, with one bucket reserved for blanket suppressions and
    /// one for editable suppressions. The interval index only traverses subtrees whose buckets
    /// overlap the queried lint.
    fn target_mask(self) -> u64 {
        match self {
            // Keep blanket suppressions separate so they are always considered.
            SuppressionTarget::All => 1,
            SuppressionTarget::Empty => 0,
            SuppressionTarget::Lint(id) => {
                let mut hasher = FxHasher::default();
                id.hash(&mut hasher);
                // The top bit is reserved for editable suppressions.
                1 << (1 + hasher.finish() % 62)
            }
        }
    }
}

struct SuppressionsBuilder<'a> {
    lint_registry: &'a LintRegistry,
    source: &'a str,

    /// Ignore comments at the top of the file before any non-trivia code apply to the entire file.
    /// This boolean tracks if there has been any non trivia token.
    seen_non_trivia_token: bool,

    inline: Vec<Suppression>,
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
            inline: Vec::new(),
            file: SmallVec::new_const(),
            unknown: Vec::new(),
            invalid: Vec::new(),
        }
    }

    fn set_seen_non_trivia_token(&mut self) {
        self.seen_non_trivia_token = true;
    }

    fn finish(mut self) -> Suppressions {
        self.file.shrink_to_fit();
        self.unknown.shrink_to_fit();
        self.invalid.shrink_to_fit();

        Suppressions {
            file: self.file,
            inline: IntervalIndex::from_sorted(self.inline),
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
        let comment_token_start = tokens.token_range(comment.range().start()).start();

        let suppressed_range = if is_file_suppression {
            TextRange::new(0.into(), self.source.text_len())
        } else if !comment.kind().is_type_ignore()
            && indentation_at_offset(comment_token_start, self.source).is_some()
        {
            own_line_suppression_range(comment.range(), tokens)
        } else {
            line_range
        };

        let mut push_ignore_suppression = |suppression: Suppression| {
            if is_file_suppression {
                self.file.push(suppression);
            } else {
                self.inline.push(suppression);
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
fn own_line_suppression_range(range: TextRange, tokens: &Tokens) -> TextRange {
    let comment_token_start = tokens.token_range(range.start()).start();
    let (before, after) = tokens.split_at(comment_token_start);
    let mut end = range.end();

    // A suppression after a logical newline precedes a new logical line:
    //
    // # ty: ignore
    // value = (
    //     missing
    // )
    //
    // A suppression after a non-logical newline is inside an unfinished logical line:
    //
    // values = [
    //     # ty: ignore
    //     missing,
    // ]
    //
    // Walk backwards through comments and non-logical newlines to distinguish the two cases.
    let is_inner_comment = before.iter().rev().find_map(|token| match token.kind() {
        TokenKind::Newline => Some(false),
        TokenKind::NonLogicalNewline | TokenKind::Comment => None,
        _ => Some(true),
    });

    let is_inner_comment = is_inner_comment.unwrap_or(false);
    // For an inner suppression, the first non-logical newline ends the suppression's own
    // physical line. Subsequent blank or comment-only lines are skipped before the range ends at
    // the first physical line containing code.
    let mut is_blank_or_comment_only = true;
    let mut past_suppression_line = false;

    for token in after {
        match token.kind() {
            TokenKind::Newline => {
                // A suppression preceding a logical line includes that complete logical line.
                end = token.start();
                break;
            }
            TokenKind::Comment => {}
            TokenKind::NonLogicalNewline if is_inner_comment => {
                end = token.start();
                if past_suppression_line && !is_blank_or_comment_only {
                    break;
                }
                past_suppression_line = true;
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

/// A start-sorted interval index.
///
/// The entries form an implicit balanced binary tree. Each entry stores the maximum interval end
/// and target mask for its subtree, allowing queries to skip unrelated subtrees. Intervals may
/// overlap or nest, and queries traverse them in reverse input order.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize)]
struct IntervalIndex {
    entries: Box<[IntervalEntry]>,
}

/// An indexed value and the bounds of its implicit subtree.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize)]
struct IntervalEntry {
    suppression: Suppression,
    subtree_max_end: TextSize,
    subtree_target_mask: u64,
}

impl IntervalIndex {
    /// Marks lint-specific and empty suppressions that `--add-ignore` can consider extending.
    const EDITABLE_MASK: u64 = 1 << 63;

    /// Builds an index from values sorted by interval start, retaining their input order.
    fn from_sorted(suppressions: Vec<Suppression>) -> Self {
        debug_assert!(
            suppressions.is_sorted_by_key(|suppression| suppression.suppressed_range.start())
        );

        let mut entries = suppressions
            .into_iter()
            .map(|suppression| IntervalEntry {
                subtree_max_end: suppression.suppressed_range.end(),
                subtree_target_mask: suppression.target.target_mask()
                    | if matches!(
                        suppression.target,
                        SuppressionTarget::Lint(_) | SuppressionTarget::Empty
                    ) {
                        Self::EDITABLE_MASK
                    } else {
                        0
                    },
                suppression,
            })
            .collect::<Box<[_]>>();

        Self::set_subtree_max_ends(&mut entries);

        Self { entries }
    }

    /// Populates and returns the maximum end and target mask for `entries`.
    fn set_subtree_max_ends(entries: &mut [IntervalEntry]) -> (TextSize, u64) {
        let mid = entries.len() / 2;
        let (left, root_and_right) = entries.split_at_mut(mid);
        let Some((root, right)) = root_and_right.split_first_mut() else {
            return (TextSize::default(), 0);
        };

        let (left_max_end, left_mask) = Self::set_subtree_max_ends(left);
        let (right_max_end, right_mask) = Self::set_subtree_max_ends(right);
        root.subtree_max_end = root
            .suppression
            .suppressed_range
            .end()
            .max(left_max_end)
            .max(right_max_end);
        root.subtree_target_mask |= left_mask | right_mask;
        (root.subtree_max_end, root.subtree_target_mask)
    }

    /// Returns the indexed values that intersect `query`, in reverse input order.
    ///
    /// Interval endpoints are treated as inclusive so that an empty diagnostic range at an
    /// interval boundary remains a candidate. Callers can apply stricter containment rules to the
    /// returned values.
    fn intersecting_rev(
        &self,
        query: TextRange,
        wanted: u64,
    ) -> impl Iterator<Item = &Suppression> {
        let mut pending: SmallVec<[&[IntervalEntry]; 16]> = smallvec![self.entries.as_ref()];

        std::iter::from_fn(move || {
            while let Some(entries) = pending.pop() {
                match entries {
                    [entry] => {
                        let suppressed_range = entry.suppression.suppressed_range;
                        if entry.subtree_target_mask & wanted != 0
                            && suppressed_range.start() <= query.end()
                            && suppressed_range.end() >= query.start()
                        {
                            return Some(&entry.suppression);
                        }
                    }
                    entries => {
                        let mid = entries.len() / 2;
                        let (left, root_and_right) = entries.split_at(mid);
                        let Some((root, right)) = root_and_right.split_first() else {
                            continue;
                        };

                        if root.subtree_max_end < query.start()
                            || root.subtree_target_mask & wanted == 0
                        {
                            continue;
                        }

                        if root.suppression.suppressed_range.start() > query.end() {
                            pending.push(left);
                            continue;
                        }

                        // The stack is last-in, first-out, so push in source order to visit the
                        // right subtree first.
                        pending.push(left);
                        pending.push(std::slice::from_ref(root));
                        pending.push(right);
                    }
                }
            }

            None
        })
    }

    fn iter(&self) -> impl Iterator<Item = &Suppression> {
        self.entries.iter().map(|entry| &entry.suppression)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::{PythonFile, files::system_path_to_file};
    use ruff_text_size::{TextLen as _, TextRange};

    use super::suppressions;
    use crate::Db as _;
    use crate::db::tests::TestDbBuilder;

    #[test]
    fn nested_suppressions_for_other_lints_do_not_match() {
        let source = r#"seen_code = True
# ty: ignore[unresolved-reference]
# ty: ignore[division-by-zero]
# ty: ignore[division-by-zero]
# ty: ignore[division-by-zero]
value = missing
"#;
        let db = TestDbBuilder::new()
            .with_file("test.py", source)
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "test.py").unwrap();
        let unresolved_reference = db.lint_registry().get("unresolved-reference").unwrap();
        let missing_start = source.find("missing").unwrap().try_into().unwrap();
        let missing_range = TextRange::at(missing_start, "missing".text_len());

        let suppressions = suppressions(&db, PythonFile::new(&db, file, db.python_version()));
        assert_eq!(suppressions.inline.len(), 4);
        assert_eq!(
            suppressions
                .lint_suppressions(missing_range, unresolved_reference)
                .count(),
            1
        );
    }

    #[test]
    fn editable_index_skips_nested_blanket_suppressions() {
        let source = r#"seen_code = True
# ty: ignore
# ty: ignore
# ty: ignore
# ty: ignore[]
value = missing
"#;
        let db = TestDbBuilder::new()
            .with_file("test.py", source)
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "test.py").unwrap();
        let missing_start = source.find("missing").unwrap().try_into().unwrap();
        let missing_range = TextRange::at(missing_start, "missing".text_len());

        let suppressions = suppressions(&db, PythonFile::new(&db, file, db.python_version()));
        assert_eq!(suppressions.inline.len(), 4);
        assert_eq!(
            suppressions
                .editable_inline_suppressions_rev(missing_range)
                .count(),
            1
        );
    }
}
