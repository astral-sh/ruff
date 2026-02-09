use std::fmt;

use drop_bomb::DebugDropBomb;
use ruff_db::diagnostic::{DiagnosticTag, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, IntoDiagnosticMessage, Severity, Span},
    files::File,
};
use ruff_text_size::{Ranged, TextRange};

use super::{Type, TypeCheckDiagnostics, binding_type};

use crate::diagnostic::DiagnosticGuard;
use crate::lint::LintSource;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::semantic_index;
use crate::types::function::FunctionDecorators;
use crate::{
    Db,
    lint::{LintId, LintMetadata},
    suppression::suppressions,
};

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
/// on the current inference result.
pub(crate) struct InferContext<'db, 'ast> {
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    file: File,
    module: &'ast ParsedModuleRef,
    diagnostics: std::cell::RefCell<TypeCheckDiagnostics>,
    no_type_check: InNoTypeCheck,
    multi_inference: bool,
    bomb: DebugDropBomb,
}

impl<'db, 'ast> InferContext<'db, 'ast> {
    pub(crate) fn new(db: &'db dyn Db, scope: ScopeId<'db>, module: &'ast ParsedModuleRef) -> Self {
        Self {
            db,
            scope,
            module,
            file: scope.file(db),
            multi_inference: false,
            diagnostics: std::cell::RefCell::new(TypeCheckDiagnostics::default()),
            no_type_check: InNoTypeCheck::default(),
            bomb: DebugDropBomb::new(
                "`InferContext` needs to be explicitly consumed by calling `::finish` to prevent accidental loss of diagnostics.",
            ),
        }
    }

    /// The file for which the types are inferred.
    pub(crate) fn file(&self) -> File {
        self.file
    }

    /// The module for which the types are inferred.
    pub(crate) fn module(&self) -> &'ast ParsedModuleRef {
        self.module
    }

    pub(crate) fn scope(&self) -> ScopeId<'db> {
        self.scope
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

    /// Create a secondary annotation attached to the range of the given value in
    /// the file currently being type checked.
    ///
    /// The annotation returned has no message attached to it.
    pub(crate) fn secondary<T: Ranged>(&self, ranged: T) -> Annotation {
        Annotation::secondary(self.span(ranged))
    }

    pub(crate) fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub(crate) fn extend(&mut self, other: &TypeCheckDiagnostics) {
        if !self.is_in_multi_inference() {
            self.diagnostics.get_mut().extend(other);
        }
    }

    pub(super) fn is_lint_enabled(&self, lint: &'static LintMetadata) -> bool {
        LintDiagnosticGuardBuilder::severity_and_source(self, LintId::of(lint)).is_some()
    }

    /// Optionally return a builder for a lint diagnostic guard.
    ///
    /// If the current context believes a diagnostic should be reported for
    /// the given lint, then a builder is returned that enables building a
    /// lint diagnostic guard. The guard can then be used, via its `DerefMut`
    /// implementation, to directly mutate a `Diagnostic`.
    ///
    /// The severity of the diagnostic returned is automatically determined
    /// by the given lint and configuration. The message given to
    /// `LintDiagnosticGuardBuilder::to_diagnostic` is used to construct the
    /// initial diagnostic and should be considered the "top-level message" of
    /// the diagnostic. (i.e., If nothing else about the diagnostic is seen,
    /// aside from its identifier, the message is probably the thing you'd pick
    /// to show.)
    ///
    /// The diagnostic constructed also includes a primary annotation with a
    /// `Span` derived from the range given attached to the `File` in this
    /// typing context. (That means the range given _must_ be valid for the
    /// `File` currently being type checked.) This primary annotation does
    /// not have a message attached to it, but callers can attach one via
    /// `LintDiagnosticGuard::set_primary_message`.
    ///
    /// After using the builder to make a guard, once the guard is dropped, the
    /// diagnostic is added to the context, unless there is something in the
    /// diagnostic that excludes it. (Currently, no such conditions exist.)
    ///
    /// If callers need to create a non-lint diagnostic, you'll want to use the
    /// lower level `InferContext::report_diagnostic` routine.
    pub(super) fn report_lint<'ctx, T: Ranged>(
        &'ctx self,
        lint: &'static LintMetadata,
        ranged: T,
    ) -> Option<LintDiagnosticGuardBuilder<'ctx, 'db>> {
        LintDiagnosticGuardBuilder::new(self, lint, ranged.range())
    }

    /// Optionally return a builder for a diagnostic guard.
    ///
    /// This only returns a builder if the current context allows a diagnostic
    /// with the given information to be added. In general, the requirements
    /// here are quite a bit less than for `InferContext::report_lint`, since
    /// this routine doesn't take rule selection into account (among other
    /// things).
    ///
    /// After using the builder to make a guard, once the guard is dropped, the
    /// diagnostic is added to the context, unless there is something in the
    /// diagnostic that excludes it. (Currently, no such conditions exist.)
    ///
    /// Callers should generally prefer adding a lint diagnostic via
    /// `InferContext::report_lint` whenever possible.
    pub(super) fn report_diagnostic<'ctx>(
        &'ctx self,
        id: DiagnosticId,
        severity: Severity,
    ) -> Option<DiagnosticGuardBuilder<'ctx, 'db>> {
        DiagnosticGuardBuilder::new(self, id, severity)
    }

    /// Returns `true` if the current expression is being inferred for a second
    /// (or subsequent) time, with a potentially different bidirectional type
    /// context.
    pub(super) fn is_in_multi_inference(&self) -> bool {
        self.multi_inference
    }

    /// Set the multi-inference state, returning the previous value.
    pub(super) fn set_multi_inference(&mut self, multi_inference: bool) -> bool {
        std::mem::replace(&mut self.multi_inference, multi_inference)
    }

    pub(super) fn set_in_no_type_check(&mut self, no_type_check: InNoTypeCheck) -> InNoTypeCheck {
        std::mem::replace(&mut self.no_type_check, no_type_check)
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
                    .filter_map(Type::as_function_literal);

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
        self.file.is_stub(self.db())
    }

    #[must_use]
    pub(crate) fn finish(mut self) -> TypeCheckDiagnostics {
        self.bomb.defuse();
        let mut diagnostics = self.diagnostics.into_inner();
        diagnostics.shrink_to_fit();
        diagnostics
    }
}

impl fmt::Debug for InferContext<'_, '_> {
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

/// An abstraction for mutating a diagnostic through the lense of a lint.
///
/// Callers can build this guard by starting with `InferContext::report_lint`.
///
/// There are two primary functions of this guard, which mutably derefs to
/// a `Diagnostic`:
///
/// * On `Drop`, the underlying diagnostic is added to the typing context.
/// * Some convenience methods for mutating the underlying `Diagnostic`
///   in lint context. For example, `LintDiagnosticGuard::set_primary_message`
///   will attach a message to the primary span on the diagnostic.
pub(super) struct LintDiagnosticGuard<'db, 'ctx> {
    /// The typing context.
    ctx: &'ctx InferContext<'db, 'ctx>,
    /// The diagnostic that we want to report.
    ///
    /// This is always `Some` until the `Drop` impl.
    diag: Option<Diagnostic>,

    source: LintSource,
}

impl LintDiagnosticGuard<'_, '_> {
    /// Set the message on the primary annotation for this diagnostic.
    ///
    /// If a message already exists on the primary annotation, then this
    /// overwrites the existing message.
    ///
    /// This message is associated with the primary annotation created
    /// for every `Diagnostic` that uses the `LintDiagnosticGuard` API.
    /// Specifically, the annotation is derived from the `TextRange` given to
    /// the `InferContext::report_lint` API.
    ///
    /// Callers can add additional primary or secondary annotations via the
    /// `DerefMut` trait implementation to a `Diagnostic`.
    pub(super) fn set_primary_message(&mut self, message: impl IntoDiagnosticMessage) {
        // N.B. It is normally bad juju to define `self` methods
        // on types that implement `Deref`. Instead, it's idiomatic
        // to do `fn foo(this: &mut LintDiagnosticGuard)`, which in
        // turn forces callers to use
        // `LintDiagnosticGuard(&mut guard, message)`. But this is
        // supremely annoying for what is expected to be a common
        // case.
        //
        // Moreover, most of the downside that comes from these sorts
        // of methods is a semver hazard. Because the deref target type
        // could also define a method by the same name, and that leads
        // to confusion. But we own all the code involved here and
        // there is no semver boundary. So... ¯\_(ツ)_/¯ ---AG

        // OK because we know the diagnostic was constructed with a single
        // primary annotation that will always come before any other annotation
        // in the diagnostic. (This relies on the `Diagnostic` API not exposing
        // any methods for removing annotations or re-ordering them, which is
        // true as of 2025-04-11.)
        let ann = self.primary_annotation_mut().unwrap();
        ann.set_message(message);
    }

    /// Adds a tag on the primary annotation for this diagnostic.
    ///
    /// This tag is associated with the primary annotation created
    /// for every `Diagnostic` that uses the `LintDiagnosticGuard` API.
    /// Specifically, the annotation is derived from the `TextRange` given to
    /// the `InferContext::report_lint` API.
    ///
    /// Callers can add additional primary or secondary annotations via the
    /// `DerefMut` trait implementation to a `Diagnostic`.
    pub(super) fn add_primary_tag(&mut self, tag: DiagnosticTag) {
        let ann = self.primary_annotation_mut().unwrap();
        ann.push_tag(tag);
    }
}

impl std::ops::Deref for LintDiagnosticGuard<'_, '_> {
    type Target = Diagnostic;

    fn deref(&self) -> &Diagnostic {
        // OK because `self.diag` is only `None` within `Drop`.
        self.diag.as_ref().unwrap()
    }
}

/// Return a mutable borrow of the diagnostic in this guard.
///
/// Callers may mutate the diagnostic to add new sub-diagnostics
/// or annotations.
///
/// The diagnostic is added to the typing context, if appropriate,
/// when this guard is dropped.
impl std::ops::DerefMut for LintDiagnosticGuard<'_, '_> {
    fn deref_mut(&mut self) -> &mut Diagnostic {
        // OK because `self.diag` is only `None` within `Drop`.
        self.diag.as_mut().unwrap()
    }
}

/// Finishes use of this guard.
///
/// This will add the lint as a diagnostic to the typing context if
/// appropriate. The diagnostic may be skipped, for example, if there is a
/// relevant suppression.
impl Drop for LintDiagnosticGuard<'_, '_> {
    fn drop(&mut self) {
        // OK because the only way `self.diag` is `None`
        // is via this impl, which can only run at most
        // once.
        let mut diag = self.diag.take().unwrap();

        diag.sub(SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            match self.source {
                LintSource::Default => format!("rule `{}` is enabled by default", diag.id()),
                LintSource::Cli => format!("rule `{}` was selected on the command line", diag.id()),
                LintSource::File => {
                    format!(
                        "rule `{}` was selected in the configuration file",
                        diag.id()
                    )
                }
                LintSource::Editor => {
                    format!("rule `{}` was selected in the editor settings", diag.id())
                }
            },
        ));

        self.ctx.diagnostics.borrow_mut().push(diag);
    }
}

/// A builder for constructing a lint diagnostic guard.
///
/// This type exists to separate the phases of "check if a diagnostic should
/// be reported" and "build the actual diagnostic." It's why, for example,
/// `InferContext::report_lint` only requires a `LintMetadata` (and a range),
/// but this builder further requires a message before one can mutate the
/// diagnostic. This is because the `LintMetadata` can be used to derive
/// the diagnostic ID and its severity (based on configuration). Combined
/// with a message you get the minimum amount of data required to build a
/// `Diagnostic`.
///
/// Additionally, the range is used to construct a primary annotation (without
/// a message) using the file current being type checked. The range given to
/// `InferContext::report_lint` must be from the file currently being type
/// checked.
///
/// If callers need to report a diagnostic with an identifier type other
/// than `DiagnosticId::Lint`, then they should use the more general
/// `InferContext::report_diagnostic` API. But note that this API will not take
/// rule selection or suppressions into account.
///
/// # When is the diagnostic added?
///
/// When a builder is not returned by `InferContext::report_lint`, then
/// it is known that the diagnostic should not be reported. This can happen
/// when the diagnostic is disabled or suppressed (among other reasons).
pub(super) struct LintDiagnosticGuardBuilder<'db, 'ctx> {
    ctx: &'ctx InferContext<'db, 'ctx>,
    id: LintId,
    severity: Severity,
    source: LintSource,
    primary_range: TextRange,
}

impl<'db, 'ctx> LintDiagnosticGuardBuilder<'db, 'ctx> {
    fn severity_and_source(
        ctx: &'ctx InferContext<'db, 'ctx>,
        lint: LintId,
    ) -> Option<(Severity, LintSource)> {
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

        if !ctx.db.should_check_file(ctx.file) {
            return None;
        }
        // Skip over diagnostics if the rule
        // is disabled.
        let (severity, source) = ctx.db.rule_selection(ctx.file).get(lint)?;
        // If we're not in type checking mode,
        // we can bail now.
        if ctx.is_in_no_type_check() {
            return None;
        }
        // If this lint is being reported as part of multi-inference of a given expression,
        // silence it to avoid duplicated diagnostics.
        if ctx.is_in_multi_inference() {
            return None;
        }

        Some((severity, source))
    }

    fn new(
        ctx: &'ctx InferContext<'db, 'ctx>,
        lint: &'static LintMetadata,
        range: TextRange,
    ) -> Option<LintDiagnosticGuardBuilder<'db, 'ctx>> {
        let lint_id = LintId::of(lint);

        let (severity, source) = Self::severity_and_source(ctx, lint_id)?;

        let suppressions = suppressions(ctx.db(), ctx.file());
        if let Some(suppression) = suppressions.find_suppression(range, lint_id) {
            ctx.diagnostics.borrow_mut().mark_used(suppression.id());
            return None;
        }

        Some(LintDiagnosticGuardBuilder {
            ctx,
            id: lint_id,
            severity,
            source,
            primary_range: range,
        })
    }

    /// Create a new lint diagnostic guard.
    ///
    /// This initializes a new diagnostic using the given message along with
    /// the ID and severity derived from the `LintMetadata` used to create
    /// this builder. The diagnostic also includes a primary annotation
    /// without a message. To add a message to this primary annotation, use
    /// `LintDiagnosticGuard::set_primary_message`.
    ///
    /// The diagnostic can be further mutated on the guard via its `DerefMut`
    /// impl to `Diagnostic`.
    pub(super) fn into_diagnostic(
        self,
        message: impl std::fmt::Display,
    ) -> LintDiagnosticGuard<'db, 'ctx> {
        let mut diag = Diagnostic::new(DiagnosticId::Lint(self.id.name()), self.severity, message);
        diag.set_documentation_url(Some(self.id.documentation_url()));
        // This is why `LintDiagnosticGuard::set_primary_message` exists.
        // We add the primary annotation here (because it's required), but
        // the optional message can be added later. We could accept it here
        // in this `build` method, but we already accept the main diagnostic
        // message. So the messages are likely to be quite confusable.
        let primary_span = Span::from(self.ctx.file()).with_range(self.primary_range);
        diag.annotate(Annotation::primary(primary_span));
        LintDiagnosticGuard {
            ctx: self.ctx,
            source: self.source,
            diag: Some(diag),
        }
    }
}

/// A builder for constructing a diagnostic guard.
///
/// This type exists to separate the phases of "check if a diagnostic should
/// be reported" and "build the actual diagnostic." It's why, for example,
/// `InferContext::report_diagnostic` only requires an ID and a severity, but
/// this builder further requires a message (with those three things being the
/// minimal amount of information with which to construct a diagnostic) before
/// one can mutate the diagnostic.
pub(super) struct DiagnosticGuardBuilder<'db, 'ctx> {
    ctx: &'ctx InferContext<'db, 'ctx>,
    id: DiagnosticId,
    severity: Severity,
}

impl<'db, 'ctx> DiagnosticGuardBuilder<'db, 'ctx> {
    fn new(
        ctx: &'ctx InferContext<'db, 'ctx>,
        id: DiagnosticId,
        severity: Severity,
    ) -> Option<DiagnosticGuardBuilder<'db, 'ctx>> {
        if !ctx.db.should_check_file(ctx.file) {
            return None;
        }
        // If this lint is being reported as part of multi-inference of a given expression,
        // silence it to avoid duplicated diagnostics.
        if ctx.is_in_multi_inference() {
            return None;
        }
        Some(DiagnosticGuardBuilder { ctx, id, severity })
    }

    /// Create a new guard.
    ///
    /// This initializes a new diagnostic using the given message along with
    /// the ID and severity used to create this builder.
    ///
    /// The diagnostic can be further mutated on the guard via its `DerefMut`
    /// impl to `Diagnostic`.
    pub(super) fn into_diagnostic(self, message: impl std::fmt::Display) -> DiagnosticGuard<'ctx> {
        let diag = Diagnostic::new(self.id, self.severity, message);

        DiagnosticGuard::new(self.ctx.file, &self.ctx.diagnostics, diag)
    }
}
