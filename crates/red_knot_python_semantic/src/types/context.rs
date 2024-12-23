use std::fmt;

use drop_bomb::DebugDropBomb;
use ruff_db::{
    diagnostic::{DiagnosticId, Severity},
    files::File,
};
use ruff_python_ast::AnyNodeRef;
use ruff_text_size::{Ranged, TextRange};

use super::{binding_ty, KnownFunction, TypeCheckDiagnostic, TypeCheckDiagnostics};

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
    bomb: DebugDropBomb,
}

impl<'db> InferContext<'db> {
    pub(crate) fn new(db: &'db dyn Db, scope: ScopeId<'db>) -> Self {
        Self {
            db,
            scope,
            file: scope.file(db),
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

        if self.is_in_no_type_check(node.range()) {
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

    fn is_in_no_type_check(&self, range: TextRange) -> bool {
        // Accessing the semantic index here is fine because
        // the index belongs to the same file as for which we emit the diagnostic.
        let index = semantic_index(self.db, self.file);

        let scope_id = self.scope.file_scope_id(self.db);

        // Unfortunately, we can't just use the `scope_id` here because the default values, return type
        // and other parts of a function declaration are inferred in the outer scope, and not in the function's scope.
        // That's why we walk all child-scopes to see if there's any child scope that fully contains the diagnostic range
        // and if there's any, use that scope as the starting scope instead.
        // We could probably use a binary search here but it's probably not worth it, considering that most
        // scopes have only very few child scopes and binary search also isn't free.
        let enclosing_scope = index
            .child_scopes(scope_id)
            .find_map(|(child_scope_id, scope)| {
                if scope
                    .node()
                    .as_function()
                    .is_some_and(|function| function.range().contains_range(range))
                {
                    Some(child_scope_id)
                } else {
                    None
                }
            })
            .unwrap_or(scope_id);

        // Inspect all enclosing function scopes walking bottom up and infer the function's type.
        let mut function_scope_tys = index
            .ancestor_scopes(enclosing_scope)
            .filter_map(|(_, scope)| scope.node().as_function())
            .filter_map(|function| {
                binding_ty(self.db, index.definition(function)).into_function_literal()
            });

        // Iterate over all functions and test if any is decorated with `@no_type_check`.
        function_scope_tys.any(|function_ty| {
            function_ty
                .decorators(self.db)
                .iter()
                .filter_map(|decorator| decorator.into_function_literal())
                .any(|decorator_ty| decorator_ty.is_known(self.db, KnownFunction::NoTypeCheck))
        })
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
