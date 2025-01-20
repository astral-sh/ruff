use std::fmt;

use drop_bomb::DebugDropBomb;
use ruff_db::{
    diagnostic::{DiagnosticId, Severity},
    files::File,
};
use ruff_python_ast::AnyNodeRef;
use ruff_text_size::Ranged;

use super::{binding_type, KnownFunction, TypeCheckDiagnostic, TypeCheckDiagnostics};

use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::ScopeId;
use crate::{
    lint::{LintId, LintMetadata},
    suppression::suppressions,
    Db,
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

    pub(crate) fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub(crate) fn extend<T>(&mut self, other: &T)
    where
        T: WithDiagnostics,
    {
        self.diagnostics.get_mut().extend(other.diagnostics());
    }

    /// Reports a lint located at `node`.
    pub(super) fn report_lint(
        &self,
        lint: &'static LintMetadata,
        node: AnyNodeRef,
        message: fmt::Arguments,
    ) {
        if !self.db.is_file_open(self.file) {
            return;
        }

        // Skip over diagnostics if the rule is disabled.
        let Some(severity) = self.db.rule_selection().severity(LintId::of(lint)) else {
            return;
        };

        if self.is_in_no_type_check() {
            return;
        }

        let suppressions = suppressions(self.db, self.file);

        if let Some(suppression) = suppressions.find_suppression(node.range(), LintId::of(lint)) {
            self.diagnostics.borrow_mut().mark_used(suppression.id());
            return;
        }

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
        message: fmt::Arguments,
    ) {
        if !self.db.is_file_open(self.file) {
            return;
        }

        // TODO: Don't emit the diagnostic if:
        // * The enclosing node contains any syntax errors
        // * The rule is disabled for this file. We probably want to introduce a new query that
        //   returns a rule selector for a given file that respects the package's settings,
        //   any global pragma comments in the file, and any per-file-ignores.

        self.diagnostics.borrow_mut().push(TypeCheckDiagnostic {
            file: self.file,
            id,
            message: message.to_string(),
            range: node.range(),
            severity,
        });
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
                    .filter_map(|function| {
                        binding_type(self.db, index.definition(function)).into_function_literal()
                    });

                // Iterate over all functions and test if any is decorated with `@no_type_check`.
                function_scope_tys.any(|function_ty| {
                    function_ty
                        .decorators(self.db)
                        .iter()
                        .filter_map(|decorator| decorator.into_function_literal())
                        .any(|decorator_ty| {
                            decorator_ty.is_known(self.db, KnownFunction::NoTypeCheck)
                        })
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

pub(crate) trait WithDiagnostics {
    fn diagnostics(&self) -> &TypeCheckDiagnostics;
}
