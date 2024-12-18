use std::fmt;

use drop_bomb::DebugDropBomb;
use ruff_db::{
    diagnostic::{DiagnosticId, Severity},
    files::File,
};
use ruff_python_ast::AnyNodeRef;
use ruff_text_size::Ranged;

use crate::{
    lint::{LintId, LintMetadata},
    Db,
};

use super::{TypeCheckDiagnostic, TypeCheckDiagnostics};

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
    file: File,
    diagnostics: std::cell::RefCell<TypeCheckDiagnostics>,
    bomb: DebugDropBomb,
}

impl<'db> InferContext<'db> {
    pub(crate) fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            diagnostics: std::cell::RefCell::new(TypeCheckDiagnostics::default()),
            bomb: DebugDropBomb::new("`InferContext` needs to be explicitly consumed by calling `::finish` to prevent accidental loss of diagnostics."),
        }
    }

    /// The file for which the types are inferred.
    pub(crate) fn file(&self) -> File {
        self.file
    }

    pub(crate) fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub(crate) fn extend<T>(&mut self, other: &T)
    where
        T: WithDiagnostics,
    {
        self.diagnostics
            .get_mut()
            .extend(other.diagnostics().iter().cloned());
    }

    /// Reports a lint located at `node`.
    pub(super) fn report_lint(
        &self,
        lint: &'static LintMetadata,
        node: AnyNodeRef,
        message: std::fmt::Arguments,
    ) {
        // Skip over diagnostics if the rule is disabled.
        let Some(severity) = self.db.rule_selection().severity(LintId::of(lint)) else {
            return;
        };

        self.report_diagnostic(node, DiagnosticId::Lint(lint.name()), severity, message);
    }

    /// Adds a new diagnostic.
    ///
    /// The diagnostic does not get added if the rule isn't enabled for this file.
    pub(super) fn report_diagnostic(
        &self,
        node: AnyNodeRef,
        id: DiagnosticId,
        severity: Severity,
        message: std::fmt::Arguments,
    ) {
        if !self.db.is_file_open(self.file) {
            return;
        }

        // TODO: Don't emit the diagnostic if:
        // * The enclosing node contains any syntax errors
        // * The rule is disabled for this file. We probably want to introduce a new query that
        //   returns a rule selector for a given file that respects the package's settings,
        //   any global pragma comments in the file, and any per-file-ignores.
        // * Check for suppression comments, bump a counter if the diagnostic is suppressed.

        self.diagnostics.borrow_mut().push(TypeCheckDiagnostic {
            file: self.file,
            id,
            message: message.to_string(),
            range: node.range(),
            severity,
        });
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

pub(crate) trait WithDiagnostics {
    fn diagnostics(&self) -> &TypeCheckDiagnostics;
}
