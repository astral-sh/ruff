use std::fmt;

use drop_bomb::DebugDropBomb;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span},
    files::File,
};
use ruff_text_size::Ranged;

use super::{binding_type, Type, TypeCheckDiagnostics};

use crate::semantic_index::symbol::ScopeId;
use crate::{
    lint::{LintId, LintMetadata},
    suppression::suppressions,
    Db,
};
use crate::{semantic_index::semantic_index, types::FunctionDecorators};

/// Context for inferring the types of a single file.
///
/// One context exists for at least for every inferred region but it's
/// possible that inferring a sub-region, like an unpack assignment, creates
/// a sub-context.
///
/// Tracks the reported diagnostics of the inferred region.
///
/// ## Consuming
/// It's important that the context is explicitly consumed before dropping by calling
/// [`InferContext::finish`] and the returned diagnostics must be stored
/// on the current [`TypeInference`](super::infer::TypeInference) result.
pub(crate) struct InferContext<'db> {
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    file: File,
    diagnostics: std::cell::RefCell<TypeCheckDiagnostics>,
    no_type_check: InNoTypeCheck,
    bomb: DebugDropBomb,
}

impl<'db> InferContext<'db> {
    pub(crate) fn new(db: &'db dyn Db, scope: ScopeId<'db>) -> Self {
        Self {
            db,
            scope,
            file: scope.file(db),
            diagnostics: std::cell::RefCell::new(TypeCheckDiagnostics::default()),
            no_type_check: InNoTypeCheck::default(),
            bomb: DebugDropBomb::new("`InferContext` needs to be explicitly consumed by calling `::finish` to prevent accidental loss of diagnostics."),
        }
    }

    /// The file for which the types are inferred.
    pub(crate) fn file(&self) -> File {
        self.file
    }

    /// Create a span with the range of the given expression
    /// in the file being currently type checked.
    ///
    /// If you're creating a diagnostic with snippets in files
    /// other than this one, you should create the span directly
    /// and not use this convenience API.
    pub(crate) fn span<T: Ranged>(&self, ranged: T) -> Span {
        Span::from(self.file()).with_range(ranged.range())
    }

    pub(crate) fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub(crate) fn extend(&mut self, other: &TypeCheckDiagnostics) {
        self.diagnostics.get_mut().extend(other);
    }

    /// Reports a lint located at `ranged`.
    pub(super) fn report_lint<T>(
        &self,
        lint: &'static LintMetadata,
        ranged: T,
        message: fmt::Arguments,
    ) where
        T: Ranged,
    {
        let Some(builder) = self.lint(lint) else {
            return;
        };
        let mut reporter = builder.build("");
        let diag = reporter.diagnostic();
        let span = Span::from(self.file).with_range(ranged.range());
        diag.annotate(Annotation::primary(span).message(message));
    }

    /// Optionally return a reporter builder for adding a lint diagnostic.
    ///
    /// If the current context believes a diagnostic should be reported for the
    /// given lint, then a reporter builder is returned that enables building
    /// a diagnostic. The severity of the diagnostic returned is automatically
    /// determined by the given lint and configuration. The message given is
    /// used to construct the initial diagnostic and should be considered the
    /// "primary message" of the diagnostic. (i.e., If nothing else about the
    /// diagnostic is seen, aside from its identifier, the message is probably
    /// the thing you'd pick to show.)
    ///
    /// After using the builder to make a reporter, once the reporter is
    /// dropped, the diagnostic is added to the context, unless there is
    /// something in the diagnostic that excludes it. For example, if a
    /// diagnostic's primary span is covered by a suppression, then the
    /// constructed diagnostic will be ignored.
    ///
    /// If callers need to create a non-lint diagnostic, you'll want to use
    /// the lower level `InferContext::report` routine.
    pub(super) fn lint<'ctx>(
        &'ctx self,
        lint: &'static LintMetadata,
    ) -> Option<LintReporterBuilder<'ctx, 'db>> {
        LintReporterBuilder::new(self, lint)
    }

    /// Optionally return a reporter builder for adding a diagnostic.
    ///
    /// This only returns a reporter builder if the current context
    /// allows a diagnostic with the given information to be added.
    /// In general, the requirements here are quite a bit less than
    /// for `InferContext::lint`, since this routine doesn't take rule
    /// selection into account.
    ///
    /// After using the builder to make a reporter, once the reporter is
    /// dropped, the diagnostic is added to the context, unless there is
    /// something in the diagnostic that excludes it.
    ///
    /// Callers should generally prefer adding a lint diagnostic via
    /// `InferContext::lint` whenever possible.
    pub(super) fn report<'ctx>(
        &'ctx self,
        id: DiagnosticId,
        severity: Severity,
    ) -> Option<DiagnosticReporterBuilder<'ctx, 'db>> {
        DiagnosticReporterBuilder::new(self, id, severity)
    }

    pub(super) fn set_in_no_type_check(&mut self, no_type_check: InNoTypeCheck) {
        self.no_type_check = no_type_check;
    }

    fn is_in_no_type_check(&self) -> bool {
        match self.no_type_check {
            InNoTypeCheck::Possibly => {
                // Accessing the semantic index here is fine because
                // the index belongs to the same file as for which we emit the diagnostic.
                let index = semantic_index(self.db, self.file);

                let scope_id = self.scope.file_scope_id(self.db);

                // Inspect all ancestor function scopes by walking bottom up and infer the function's type.
                let mut function_scope_tys = index
                    .ancestor_scopes(scope_id)
                    .filter_map(|(_, scope)| scope.node().as_function())
                    .map(|node| binding_type(self.db, index.expect_single_definition(node)))
                    .filter_map(Type::into_function_literal);

                // Iterate over all functions and test if any is decorated with `@no_type_check`.
                function_scope_tys.any(|function_ty| {
                    function_ty.has_known_decorator(self.db, FunctionDecorators::NO_TYPE_CHECK)
                })
            }
            InNoTypeCheck::Yes => true,
        }
    }

    /// Are we currently inferring types in a stub file?
    pub(crate) fn in_stub(&self) -> bool {
        self.file.is_stub(self.db().upcast())
    }

    #[must_use]
    pub(crate) fn finish(mut self) -> TypeCheckDiagnostics {
        self.bomb.defuse();
        let mut diagnostics = self.diagnostics.into_inner();
        diagnostics.shrink_to_fit();
        diagnostics
    }
}

impl fmt::Debug for InferContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("TyContext")
            .field("file", &self.file)
            .field("diagnostics", &self.diagnostics)
            .field("defused", &self.bomb)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum InNoTypeCheck {
    /// The inference might be in a `no_type_check` block but only if any
    /// ancestor function is decorated with `@no_type_check`.
    #[default]
    Possibly,

    /// The inference is known to be in an `@no_type_check` decorated function.
    Yes,
}

/// An abstraction for reporting lints as diagnostics.
///
/// Callers can build a reporter via `InferContext::lint`.
///
/// A reporter encapsulates the logic for determining if a diagnostic *should*
/// be reported according to the environment, configuration, and any relevant
/// suppressions. The advantage of this reporter is that callers may avoid
/// doing extra work to populate a diagnostic if it is known that it would be
/// otherwise ignored.
///
/// The diagnostic is added to the typing context, if appropriate, when this
/// reporter is dropped. That is, there are two different filtering points:
///
/// * Building the reporter may return `None` if the initial information given
///   is sufficient to determine that the diagnostic will not be shown to end
///   users.
/// * Dropping the reporter may ignore the diagnostic if information inside the
///   diagnostic itself (like its span positions) indicates that it should be
///   suppressed.
///
/// If callers need to report a diagnostic with an identifier type other
/// than `DiagnosticId::Lint`, then they should use the more general
/// `InferContext::report` API. But note that this API will not take rule
/// selection or suppressions into account.
pub(super) struct LintReporter<'db, 'ctx> {
    /// The typing context.
    ctx: &'ctx InferContext<'db>,
    /// The diagnostic that we want to report.
    ///
    /// This is always `Some` until the `Drop` impl.
    diag: Option<Diagnostic>,
    /// The lint ID. Stored here because it doesn't
    /// seem easily derivable from `DiagnosticId` and
    /// because we need it for looking up suppressions.
    lint_id: LintId,
}

impl LintReporter<'_, '_> {
    /// Return a mutable borrow of the diagnostic on this reporter.
    ///
    /// Callers may mutate the diagnostic to add new sub-diagnostics
    /// or annotations.
    ///
    /// The diagnostic is added to the typing context, if appropriate,
    /// when this reporter is dropped.
    pub(super) fn diagnostic(&mut self) -> &mut Diagnostic {
        // OK because `self.diag` is only `None` within `Drop`.
        self.diag.as_mut().unwrap()
    }
}

/// Finishes use of this reporter.
///
/// This will add the lint as a diagnostic to the typing context if
/// appropriate. The diagnostic may be skipped, for example, if there is a
/// relevant suppression.
impl Drop for LintReporter<'_, '_> {
    fn drop(&mut self) {
        // OK because the only way `self.diag` is `None`
        // is via this impl, which can only run at most
        // once.
        let diag = self.diag.take().unwrap();
        let Some(span) = diag.primary_span() else {
            self.ctx.diagnostics.borrow_mut().push(diag);
            return;
        };
        let Some(range) = span.range() else {
            self.ctx.diagnostics.borrow_mut().push(diag);
            return;
        };
        // Two things to note here.
        //
        // First is that we use `span.file()` here to find suppressions
        // and *not* self.reporter.ctx.file` (as this code used to do).
        // The reasoning for this is that the suppression ought to be
        // based on the primary span of the diagnostic itself, and not
        // the file that happens to be checked currently. It seems likely
        // that in most (all?) cases, these will be the same. But in a
        // hypothetical case where it isn't, this seems like the more
        // sensible option.
        //
        // Second is that we are only checking suppressions here when
        // a range is present. But it seems like we could check
        // suppressions even when a range isn't present, since they
        // could be file-level suppressions. However, it's not clear,
        // in practice, when this matters. In particular, it is generally
        // expected that most (all?) lint diagnostics will come with at
        // least one primary span.
        let suppressions = suppressions(self.ctx.db, span.file());
        if let Some(suppression) = suppressions.find_suppression(range, self.lint_id) {
            self.ctx
                .diagnostics
                .borrow_mut()
                .mark_used(suppression.id());
        } else {
            self.ctx.diagnostics.borrow_mut().push(diag);
        }
    }
}

/// A builder for constructing a lint diagnostic reporter.
///
/// This type exists to separate the phases of "check if a diagnostic should
/// be reported" and "build the actual diagnostic." It's why, for example,
/// `InferContext::lint` only requires a `LintMetadata`, but this builder
/// further requires a message before one can mutate the diagnostic. This is
/// because the `LintMetadata` can be used to derive the diagnostic ID and its
/// severity (based on configuration). Combined with a message you get the
/// minimum amount of data required to build a `Diagnostic`.
pub(super) struct LintReporterBuilder<'db, 'ctx> {
    ctx: &'ctx InferContext<'db>,
    id: DiagnosticId,
    severity: Severity,
    lint_id: LintId,
}

impl<'db, 'ctx> LintReporterBuilder<'db, 'ctx> {
    fn new(
        ctx: &'ctx InferContext<'db>,
        lint: &'static LintMetadata,
    ) -> Option<LintReporterBuilder<'db, 'ctx>> {
        if !ctx.db.is_file_open(ctx.file) {
            return None;
        }
        let lint_id = LintId::of(lint);
        // Skip over diagnostics if the rule
        // is disabled.
        let severity = ctx.db.rule_selection().severity(lint_id)?;
        // If we're not in type checking mode,
        // we can bail now.
        if ctx.is_in_no_type_check() {
            return None;
        }
        let id = DiagnosticId::Lint(lint.name());
        Some(LintReporterBuilder {
            ctx,
            id,
            severity,
            lint_id,
        })
    }

    /// Create a new lint reporter.
    ///
    /// This initializes a new diagnostic using the given message along with
    /// the ID and severity derived from the `LintMetadata` used to create this
    /// builder.
    ///
    /// The diagnostic can be further mutated via
    /// `LintReporter::diagnostic`.
    #[must_use]
    pub(super) fn build(self, message: impl std::fmt::Display) -> LintReporter<'db, 'ctx> {
        let diag = Some(Diagnostic::new(self.id, self.severity, message));
        LintReporter {
            ctx: self.ctx,
            diag,
            lint_id: self.lint_id,
        }
    }
}

/// An abstraction for reporting diagnostics.
///
/// Callers can build a reporter via `InferContext::report`.
///
/// A reporter encapsulates the logic for determining if a diagnostic *should*
/// be reported according to the environment, configuration, and any relevant
/// suppressions. The advantage of this reporter is that callers may avoid
/// doing extra work to populate a diagnostic if it is known that it would be
/// otherwise ignored.
///
/// The diagnostic is added to the typing context, if appropriate, when this
/// reporter is dropped.
///
/// Callers likely should use `LintReporter` via `InferContext::lint` instead.
/// This reporter is only intended for use with non-lint diagnostics.
pub(super) struct DiagnosticReporter<'db, 'ctx> {
    ctx: &'ctx InferContext<'db>,
    /// The diagnostic that we want to report.
    ///
    /// This is always `Some` until the `Drop` impl.
    diag: Option<Diagnostic>,
}

impl DiagnosticReporter<'_, '_> {
    /// Return a mutable borrow of the diagnostic on this reporter.
    ///
    /// Callers may mutate the diagnostic to add new sub-diagnostics
    /// or annotations.
    ///
    /// The diagnostic is added to the typing context, if appropriate,
    /// when this reporter is dropped.
    pub(super) fn diagnostic(&mut self) -> &mut Diagnostic {
        // OK because `self.diag` is only `None` within `Drop`.
        self.diag.as_mut().unwrap()
    }
}

/// Finishes use of this reporter.
///
/// This will add the diagnostic to the typing context if appropriate.
impl Drop for DiagnosticReporter<'_, '_> {
    fn drop(&mut self) {
        // The comment below was copied from the original
        // implementation of diagnostic reporting. The code
        // has been refactored, but this still kind of looked
        // relevant, so I've preserved the note. ---AG
        //
        // TODO: Don't emit the diagnostic if:
        // * The enclosing node contains any syntax errors
        // * The rule is disabled for this file. We probably want to introduce a new query that
        //   returns a rule selector for a given file that respects the package's settings,
        //   any global pragma comments in the file, and any per-file-ignores.

        // OK because the only way `self.diag` is `None`
        // is via this impl, which can only run at most
        // once.
        let diag = self.diag.take().unwrap();
        self.ctx.diagnostics.borrow_mut().push(diag);
    }
}

/// A builder for constructing a diagnostic reporter.
///
/// This type exists to separate the phases of "check if a diagnostic should
/// be reported" and "build the actual diagnostic." It's why, for example,
/// `InferContext::report` only requires an ID and a severity, but this builder
/// further requires a message (with those three things being the minimal
/// amount of information with which to construct a diagnostic) before one can
/// mutate the diagnostic.
pub(super) struct DiagnosticReporterBuilder<'db, 'ctx> {
    ctx: &'ctx InferContext<'db>,
    id: DiagnosticId,
    severity: Severity,
}

impl<'db, 'ctx> DiagnosticReporterBuilder<'db, 'ctx> {
    fn new(
        ctx: &'ctx InferContext<'db>,
        id: DiagnosticId,
        severity: Severity,
    ) -> Option<DiagnosticReporterBuilder<'db, 'ctx>> {
        if !ctx.db.is_file_open(ctx.file) {
            return None;
        }
        Some(DiagnosticReporterBuilder { ctx, id, severity })
    }

    /// Create a new reporter.
    ///
    /// This initializes a new diagnostic using the given message along with
    /// the ID and severity used to create this builder.
    ///
    /// The diagnostic can be further mutated via
    /// `DiagnosticReporter::diagnostic`.
    #[must_use]
    pub(super) fn build(self, message: impl std::fmt::Display) -> DiagnosticReporter<'db, 'ctx> {
        let diag = Some(Diagnostic::new(self.id, self.severity, message));
        DiagnosticReporter {
            ctx: self.ctx,
            diag,
        }
    }
}
