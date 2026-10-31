use rustc_hash::FxHashMap;
use unicode_normalization::UnicodeNormalization;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{Expr, StmtImportFrom};
use ruff_python_semantic::{BindingId, BindingKind, NodeId, ScopeId, SemanticModel};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::Ranged;

use crate::codes::Category;
use crate::fix::edits::delete_stmt;
use crate::{Edit, Fix, FixAvailability, Violation, checkers::ast::Checker};

/// ## What it does
/// Checks for member imports that should instead be accessed by importing the
/// module.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for imports to make your code
/// more readable and idiomatic.
///
/// For example, it's common to import `pandas` as `pd`, and then access
/// members like `Series` via `pd.Series`, rather than importing `Series`
/// directly.
///
/// ## Example
/// ```python
/// from pandas import Series
///
/// Series()
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// pd.Series()
/// ```
///
/// ## Fix safety
/// This fix is always marked as unsafe because it changes the names bound by an
/// import, which can affect a module's public API and code that inspects its
/// namespace. Accessing attributes of a module can also behave differently from
/// importing its members, for example when a member is a submodule.
///
/// A fix is not available for relative imports, wildcard imports, lazy imports,
/// explicit re-exports, or imports whose names cannot be rewritten without
/// ambiguity. Fixes preserve comments, leaving the import unchanged when a
/// rewrite would remove a comment.
///
/// ## Options
/// - `lint.flake8-import-conventions.banned-from`
/// - `lint.flake8-import-conventions.aliases`
/// - `lint.flake8-import-conventions.extend-aliases`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.263", category = Category::Pedantic)]
pub(crate) struct BannedImportFrom {
    name: String,
}

impl Violation for BannedImportFrom {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedImportFrom { name } = self;
        format!("Members of `{name}` should not be imported explicitly")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Import `{}` instead", self.name))
    }
}

/// ICN003
pub(crate) fn banned_import_from(
    checker: &Checker,
    stmt: &StmtImportFrom,
    introduced: &mut FxHashMap<(ScopeId, String), NodeId>,
) {
    let name = ruff_python_ast::helpers::format_import_from(stmt.level, stmt.module.as_deref())
        .into_owned();
    let mut diagnostic = checker.report_diagnostic(BannedImportFrom { name }, stmt.range());
    if let Some(fix) = fix_import(checker, stmt, introduced) {
        diagnostic.set_fix(fix);
    }
}

/// A qualifier must resolve to the same import everywhere it replaces a name.
/// Check all bindings, including later assignments and declarations in closures.
fn available(
    semantic: &SemanticModel,
    name: &str,
    scope: ScopeId,
    existing: Option<BindingId>,
) -> bool {
    semantic.scopes.ancestors(scope).all(|scope| {
        !scope.uses_star_imports() && scope.get_all(name).all(|binding| Some(binding) == existing)
    })
}

fn fix_import(
    checker: &Checker,
    stmt: &StmtImportFrom,
    introduced: &mut FxHashMap<(ScopeId, String), NodeId>,
) -> Option<Fix> {
    let module = stmt.module.as_ref()?.as_str();
    if stmt.level != 0
        || stmt.is_lazy
        || module == "__future__"
        || checker.comment_ranges().intersects(stmt.range())
    {
        return None;
    }
    let semantic = checker.semantic();
    let scope = semantic.current_scope();
    let statement_id = semantic.current_statement_id()?;
    let configured_alias = checker
        .settings()
        .flake8_import_conventions
        .aliases
        .get(module);
    if configured_alias
        .is_some_and(|alias| !is_identifier(alias) || !alias.nfkc().eq(alias.chars()))
    {
        return None;
    }

    // Reuse an earlier module import in the same suite, never one that might
    // only execute in a different branch or after the member is used.
    let existing = scope
        .all_bindings()
        .filter_map(|(name, id)| {
            let binding = semantic.binding(id);
            let qualified = match &binding.kind {
                BindingKind::Import(import) => &import.qualified_name,
                BindingKind::SubmoduleImport(import) => &import.qualified_name,
                _ => return None,
            };
            let source = binding.source?;
            if !qualified.segments().iter().copied().eq(module.split('.'))
                || binding.is_lazy()
                || binding.is_deleted()
                || semantic.rebinding_scopes(id).is_some()
                || binding.start() >= stmt.start()
                || binding.context.is_typing() != semantic.execution_context().is_typing()
                || semantic.parent_statement_id(source)
                    != semantic.parent_statement_id(statement_id)
                || !semantic.same_branch(source, statement_id)
                || configured_alias.is_some_and(|alias| alias != name)
                || !available(semantic, name, semantic.scope_id, Some(id))
            {
                return None;
            }
            let qualifier = if matches!(binding.kind, BindingKind::SubmoduleImport(_)) {
                module.to_string()
            } else {
                name.to_string()
            };
            Some((id, name, qualifier))
        })
        .min_by_key(|(id, _, _)| semantic.binding(*id).start());

    let (existing_id, bound_name, qualifier) = if let Some((id, name, qualifier)) = existing {
        (Some(id), name, qualifier)
    } else if let Some(alias) = configured_alias {
        (None, alias.as_str(), alias.clone())
    } else {
        (None, module.split('.').next()?, module.to_string())
    };
    if !available(semantic, bound_name, semantic.scope_id, existing_id) {
        return None;
    }

    let mut edits = Vec::new();
    for alias in &stmt.names {
        let name = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
        let mut bindings = scope.get_all(name);
        let binding_id = bindings.next()?;
        let binding = semantic.binding(binding_id);
        if bindings.next().is_some()
            || semantic.rebinding_scopes(binding_id).is_some()
            || semantic.delayed_annotations(binding_id).is_some()
            || !matches!(binding.kind, BindingKind::FromImport(_))
            || binding.range() != alias.identifier()
            || binding.source != Some(statement_id)
            || binding.is_explicit_export()
            || binding.is_lazy()
            || binding.is_deleted()
            || binding.is_global()
            || binding.is_nonlocal()
        {
            return None;
        }
        for reference in binding.references() {
            let reference = semantic.reference(reference);
            if !matches!(
                semantic.expression(reference.expression_id()?),
                Some(Expr::Name(_))
            ) {
                return None;
            }
            if !reference.is_load()
                || reference.in_string_type_definition()
                || reference.in_dunder_all_definition()
                || checker.locator().slice(reference.range()) != name
                || !available(semantic, bound_name, reference.scope_id(), existing_id)
            {
                return None;
            }
            edits.push(Edit::range_replacement(
                format!("{qualifier}.{}", alias.name),
                reference.range(),
            ));
        }
    }

    let edit = if let Some(id) = existing_id {
        let existing_stmt = semantic.binding(id).statement(semantic)?;
        // Keep another fix (such as F401) from removing the import we reuse.
        edits.push(Edit::range_replacement(
            checker.locator().slice(existing_stmt.range()).to_string(),
            existing_stmt.range(),
        ));
        let deletion = delete_stmt(
            semantic.current_statement(),
            semantic.current_statement_parent(),
            checker.locator(),
            checker.indexer(),
        );
        if checker.comment_ranges().intersects(deletion.range()) {
            return None;
        }
        deletion
    } else {
        let import = if let Some(alias) = configured_alias {
            format!("import {module} as {alias}")
        } else {
            format!("import {module}")
        };
        Edit::range_replacement(import, stmt.range())
    };
    let fix = Fix::unsafe_edits(edit, edits);
    if existing_id.is_none() {
        // Apply at most one new import for this name per pass. Later passes
        // reuse it. Each diagnostic still receives a fix, so suppressing one
        // import's warning does not prevent fixing the others.
        let group = introduced
            .entry((semantic.scope_id, bound_name.to_string()))
            .or_insert(statement_id);
        Some(fix.isolate(Checker::isolation(Some(*group))))
    } else {
        Some(fix)
    }
}
