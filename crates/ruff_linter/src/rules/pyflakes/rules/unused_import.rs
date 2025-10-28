use std::borrow::Cow;
use std::iter;

use anyhow::{Result, anyhow, bail};
use std::collections::BTreeMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::{QualifiedName, QualifiedNameBuilder};
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_semantic::{
    AnyImport, Binding, BindingFlags, BindingId, BindingKind, Exceptions, Imported, NodeId, Scope,
    ScopeId, SemanticModel, SubmoduleImport,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix;
use crate::preview::{
    is_dunder_init_fix_unused_import_enabled, is_refined_submodule_import_match_enabled,
};
use crate::registry::Rule;
use crate::rules::{isort, isort::ImportSection, isort::ImportType};
use crate::settings::LinterSettings;
use crate::{Applicability, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for unused imports.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. They also increase the cognitive load of reading the code.
///
/// If an import statement is used to check for the availability or existence
/// of a module, consider using `importlib.util.find_spec` instead.
///
/// If an import statement is used to re-export a symbol as part of a module's
/// public interface, consider using a "redundant" import alias, which
/// instructs Ruff (and other tools) to respect the re-export, and avoid
/// marking it as unused, as in:
///
/// ```python
/// from module import member as member
/// ```
///
/// Alternatively, you can use `__all__` to declare a symbol as part of the module's
/// interface, as in:
///
/// ```python
/// # __init__.py
/// import some_module
///
/// __all__ = ["some_module"]
/// ```
///
/// ## Preview
/// When [preview] is enabled (and certain simplifying assumptions
/// are met), we analyze all import statements for a given module
/// when determining whether an import is used, rather than simply
/// the last of these statements. This can result in both different and
/// more import statements being marked as unused.
///
/// For example, if a module consists of
///
/// ```python
/// import a
/// import a.b
/// ```
///
/// then both statements are marked as unused under [preview], whereas
/// only the second is marked as unused under stable behavior.
///
/// As another example, if a module consists of
///
/// ```python
/// import a.b
/// import a
///
/// a.b.foo()
/// ```
///
/// then a diagnostic will only be emitted for the first line under [preview],
/// whereas a diagnostic would only be emitted for the second line under
/// stable behavior.
///
/// Note that this behavior is somewhat subjective and is designed
/// to conform to the developer's intuition rather than Python's actual
/// execution. To wit, the statement `import a.b` automatically executes
/// `import a`, so in some sense `import a` is _always_ redundant
/// in the presence of `import a.b`.
///
///
/// ## Fix safety
///
/// Fixes to remove unused imports are safe, except in `__init__.py` files.
///
/// Applying fixes to `__init__.py` files is currently in preview. The fix offered depends on the
/// type of the unused import. Ruff will suggest a safe fix to export first-party imports with
/// either a redundant alias or, if already present in the file, an `__all__` entry. If multiple
/// `__all__` declarations are present, Ruff will not offer a fix. Ruff will suggest an unsafe fix
/// to remove third-party and standard library imports -- the fix is unsafe because the module's
/// interface changes.
///
/// See [this FAQ section](https://docs.astral.sh/ruff/faq/#how-does-ruff-determine-which-of-my-imports-are-first-party-third-party-etc)
/// for more details on how Ruff
/// determines whether an import is first or third-party.
///
/// ## Example
///
/// ```python
/// import numpy as np  # unused import
///
///
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// Use instead:
///
/// ```python
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// To check the availability of a module, use `importlib.util.find_spec`:
///
/// ```python
/// from importlib.util import find_spec
///
/// if find_spec("numpy") is not None:
///     print("numpy is installed")
/// else:
///     print("numpy is not installed")
/// ```
///
/// ## Options
/// - `lint.ignore-init-module-imports`
/// - `lint.pyflakes.allowed-unused-imports`
///
/// ## References
/// - [Python documentation: `import`](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation: `importlib.util.find_spec`](https://docs.python.org/3/library/importlib.html#importlib.util.find_spec)
/// - [Typing documentation: interface conventions](https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.18")]
pub(crate) struct UnusedImport {
    /// Qualified name of the import
    name: String,
    /// Unqualified name of the import
    module: String,
    /// Name of the import binding
    binding: String,
    context: UnusedImportContext,
    multiple: bool,
    ignore_init_module_imports: bool,
}

impl Violation for UnusedImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport { name, context, .. } = self;
        match context {
            UnusedImportContext::ExceptHandler => {
                format!(
                    "`{name}` imported but unused; consider using `importlib.util.find_spec` to test for availability"
                )
            }
            UnusedImportContext::DunderInitFirstParty { .. } => {
                format!(
                    "`{name}` imported but unused; consider removing, adding to `__all__`, or using a redundant alias"
                )
            }
            UnusedImportContext::Other => format!("`{name}` imported but unused"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedImport {
            name,
            module,
            binding,
            multiple,
            ignore_init_module_imports,
            context,
        } = self;
        if *ignore_init_module_imports {
            match context {
                UnusedImportContext::DunderInitFirstParty {
                    dunder_all_count: DunderAllCount::Zero,
                    submodule_import: false,
                } => return Some(format!("Use an explicit re-export: `{module} as {module}`")),
                UnusedImportContext::DunderInitFirstParty {
                    dunder_all_count: DunderAllCount::Zero,
                    submodule_import: true,
                } => {
                    return Some(format!(
                        "Use an explicit re-export: `import {parent} as {parent}; import {binding}`",
                        parent = binding
                            .split('.')
                            .next()
                            .expect("Expected all submodule imports to contain a '.'")
                    ));
                }
                UnusedImportContext::DunderInitFirstParty {
                    dunder_all_count: DunderAllCount::One,
                    submodule_import: false,
                } => return Some(format!("Add unused import `{binding}` to __all__")),
                UnusedImportContext::DunderInitFirstParty {
                    dunder_all_count: DunderAllCount::One,
                    submodule_import: true,
                } => {
                    return Some(format!(
                        "Add `{}` to __all__",
                        binding
                            .split('.')
                            .next()
                            .expect("Expected all submodule imports to contain a '.'")
                    ));
                }
                UnusedImportContext::DunderInitFirstParty {
                    dunder_all_count: DunderAllCount::Many,
                    submodule_import: _,
                }
                | UnusedImportContext::ExceptHandler
                | UnusedImportContext::Other => {}
            }
        }
        Some(if *multiple {
            "Remove unused import".to_string()
        } else {
            format!("Remove unused import: `{name}`")
        })
    }
}

/// Enumeration providing three possible answers to the question:
/// "How many `__all__` definitions are there in this file?"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DunderAllCount {
    Zero,
    One,
    Many,
}

impl From<usize> for DunderAllCount {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::Zero,
            1 => Self::One,
            _ => Self::Many,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, is_macro::Is)]
enum UnusedImportContext {
    /// The unused import occurs inside an except handler
    ExceptHandler,
    /// The unused import is a first-party import in an `__init__.py` file
    DunderInitFirstParty {
        dunder_all_count: DunderAllCount,
        submodule_import: bool,
    },
    /// The unused import is something else
    Other,
}

fn is_first_party(import: &AnyImport, checker: &Checker) -> bool {
    let source_name = import.source_name().join(".");
    let category = isort::categorize(
        &source_name,
        import.qualified_name().is_unresolved_import(),
        &checker.settings().src,
        checker.package(),
        checker.settings().isort.detect_same_package,
        &checker.settings().isort.known_modules,
        checker.target_version(),
        checker.settings().isort.no_sections,
        &checker.settings().isort.section_order,
        &checker.settings().isort.default_section,
    );
    matches! {
        category,
        ImportSection::Known(ImportType::FirstParty | ImportType::LocalFolder)
    }
}

/// Find the `Expr` for top-level `__all__` bindings.
fn find_dunder_all_exprs<'a>(semantic: &'a SemanticModel) -> Vec<&'a ast::Expr> {
    semantic
        .global_scope()
        .get_all("__all__")
        .filter_map(|binding_id| {
            let binding = semantic.binding(binding_id);
            let stmt = match binding.kind {
                BindingKind::Export(_) => binding.statement(semantic),
                _ => None,
            }?;
            match stmt {
                Stmt::Assign(ast::StmtAssign { value, .. }) => Some(&**value),
                Stmt::AnnAssign(ast::StmtAnnAssign { value, .. }) => value.as_deref(),
                Stmt::AugAssign(ast::StmtAugAssign { value, .. }) => Some(&**value),
                _ => None,
            }
        })
        .collect()
}

/// F401
/// For some unused binding in an import statement...
///
///  __init__.py ∧ 1stpty → safe,   if one __all__, add to __all__
///                         safe,   if no __all__, convert to redundant-alias
///                         n/a,    if multiple __all__, offer no fix
///  __init__.py ∧ stdlib → unsafe, remove
///  __init__.py ∧ 3rdpty → unsafe, remove
///
/// ¬__init__.py ∧ 1stpty → safe,   remove
/// ¬__init__.py ∧ stdlib → safe,   remove
/// ¬__init__.py ∧ 3rdpty → safe,   remove
///
pub(crate) fn unused_import(checker: &Checker, scope: &Scope) {
    // Collect all unused imports by statement.
    let mut unused: BTreeMap<(NodeId, Exceptions), Vec<ImportBinding>> = BTreeMap::default();
    let mut ignored: BTreeMap<(NodeId, Exceptions), Vec<ImportBinding>> = BTreeMap::default();

    for binding in unused_imports_in_scope(checker.semantic(), scope, checker.settings()) {
        let Some(import) = binding.as_any_import() else {
            continue;
        };

        let Some(node_id) = binding.source else {
            continue;
        };

        let name = binding.name(checker.source());

        // If an import is marked as required, avoid treating it as unused, regardless of whether
        // it was _actually_ used.
        if checker
            .settings()
            .isort
            .required_imports
            .iter()
            .any(|required_import| required_import.matches(name, &import))
        {
            continue;
        }

        // If an import was marked as allowed, avoid treating it as unused.
        if checker
            .settings()
            .pyflakes
            .allowed_unused_imports
            .iter()
            .any(|allowed_unused_import| {
                let allowed_unused_import = QualifiedName::user_defined(allowed_unused_import);
                import.qualified_name().starts_with(&allowed_unused_import)
            })
        {
            continue;
        }

        let import = ImportBinding {
            name,
            import,
            range: binding.range(),
            parent_range: binding.parent_range(checker.semantic()),
            scope: binding.scope,
        };

        if checker.rule_is_ignored(Rule::UnusedImport, import.start())
            || import.parent_range.is_some_and(|parent_range| {
                checker.rule_is_ignored(Rule::UnusedImport, parent_range.start())
            })
        {
            ignored
                .entry((node_id, binding.exceptions))
                .or_default()
                .push(import);
        } else {
            unused
                .entry((node_id, binding.exceptions))
                .or_default()
                .push(import);
        }
    }

    let in_init = checker.path().ends_with("__init__.py");
    let fix_init = !checker.settings().ignore_init_module_imports;
    let preview_mode = is_dunder_init_fix_unused_import_enabled(checker.settings());
    let dunder_all_exprs = find_dunder_all_exprs(checker.semantic());

    // Generate a diagnostic for every import, but share fixes across all imports within the same
    // statement (excluding those that are ignored).
    for ((import_statement, exceptions), bindings) in unused {
        let in_except_handler =
            exceptions.intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);
        let multiple = bindings.len() > 1;

        // pair each binding with context; divide them by how we want to fix them
        let (to_reexport, to_remove): (Vec<_>, Vec<_>) = bindings
            .into_iter()
            .map(|binding| {
                let context = if in_except_handler {
                    UnusedImportContext::ExceptHandler
                } else if in_init
                    && binding.scope.is_global()
                    && is_first_party(&binding.import, checker)
                {
                    UnusedImportContext::DunderInitFirstParty {
                        dunder_all_count: DunderAllCount::from(dunder_all_exprs.len()),
                        submodule_import: binding.import.is_submodule_import(),
                    }
                } else {
                    UnusedImportContext::Other
                };
                (binding, context)
            })
            .partition(|(_, context)| context.is_dunder_init_first_party() && preview_mode);

        // generate fixes that are shared across bindings in the statement
        let (fix_remove, fix_reexport) =
            if (!in_init || fix_init || preview_mode) && !in_except_handler {
                (
                    fix_by_removing_imports(
                        checker,
                        import_statement,
                        to_remove.iter().map(|(binding, _)| binding),
                        in_init,
                    )
                    .ok(),
                    fix_by_reexporting(
                        checker,
                        import_statement,
                        to_reexport.iter().map(|(b, _)| b),
                        &dunder_all_exprs,
                    )
                    .ok(),
                )
            } else {
                (None, None)
            };

        for ((binding, context), fix) in iter::Iterator::chain(
            iter::zip(to_remove, iter::repeat(fix_remove)),
            iter::zip(to_reexport, iter::repeat(fix_reexport)),
        ) {
            let mut diagnostic = checker.report_diagnostic(
                UnusedImport {
                    name: binding.import.qualified_name().to_string(),
                    module: binding.import.member_name().to_string(),
                    binding: binding.name.to_string(),
                    context,
                    multiple,
                    ignore_init_module_imports: !fix_init,
                },
                binding.range,
            );
            if let Some(range) = binding.parent_range {
                diagnostic.set_parent(range.start());
            }
            if !in_except_handler {
                if let Some(fix) = fix.as_ref() {
                    diagnostic.set_fix(fix.clone());
                }
            }

            diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
        }
    }

    // Separately, generate a diagnostic for every _ignored_ import, to ensure that the
    // suppression comments aren't marked as unused.
    for binding in ignored.into_values().flatten() {
        let mut diagnostic = checker.report_diagnostic(
            UnusedImport {
                name: binding.import.qualified_name().to_string(),
                module: binding.import.member_name().to_string(),
                binding: binding.name.to_string(),
                context: UnusedImportContext::Other,
                multiple: false,
                ignore_init_module_imports: !fix_init,
            },
            binding.range,
        );
        if let Some(range) = binding.parent_range {
            diagnostic.set_parent(range.start());
        }

        diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
    }
}

/// An unused import with its surrounding context.
#[derive(Debug)]
struct ImportBinding<'a> {
    /// Name of the binding, which for renamed imports will differ from the qualified name.
    name: &'a str,
    /// The qualified name of the import (e.g., `typing.List` for `from typing import List`).
    import: AnyImport<'a, 'a>,
    /// The trimmed range of the import (e.g., `List` in `from typing import List`).
    range: TextRange,
    /// The range of the import's parent statement.
    parent_range: Option<TextRange>,
    /// The [`ScopeId`] of the scope in which the [`ImportBinding`] was defined.
    scope: ScopeId,
}

impl ImportBinding<'_> {
    /// The symbol that is stored in the outer scope as a result of this import.
    ///
    /// For example:
    /// - `import foo` => `foo` symbol stored in outer scope
    /// - `import foo as bar` => `bar` symbol stored in outer scope
    /// - `from foo import bar` => `bar` symbol stored in outer scope
    /// - `from foo import bar as baz` => `baz` symbol stored in outer scope
    /// - `import foo.bar` => `foo` symbol stored in outer scope
    fn symbol_stored_in_outer_scope(&self) -> &str {
        match &self.import {
            AnyImport::FromImport(_) => self.name,
            AnyImport::Import(_) => self.name,
            AnyImport::SubmoduleImport(SubmoduleImport { qualified_name }) => {
                qualified_name.segments().first().unwrap_or_else(|| {
                    panic!(
                        "Expected an import binding to have a non-empty qualified name;
                        got {qualified_name}"
                    )
                })
            }
        }
    }
}

impl Ranged for ImportBinding<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Generate a [`Fix`] to remove unused imports from a statement.
fn fix_by_removing_imports<'a>(
    checker: &Checker,
    node_id: NodeId,
    imports: impl Iterator<Item = &'a ImportBinding<'a>>,
    in_init: bool,
) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);
    let parent = checker.semantic().parent_statement(node_id);

    let member_names: Vec<Cow<'_, str>> = imports
        .map(|ImportBinding { import, .. }| import)
        .map(Imported::member_name)
        .collect();
    if member_names.is_empty() {
        bail!("Expected import bindings");
    }

    let edit = fix::edits::remove_unused_imports(
        member_names.iter().map(AsRef::as_ref),
        statement,
        parent,
        checker.locator(),
        checker.stylist(),
        checker.indexer(),
    )?;

    // It's unsafe to remove things from `__init__.py` because it can break public interfaces.
    let applicability = if in_init {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Ok(
        Fix::applicable_edit(edit, applicability).isolate(Checker::isolation(
            checker.semantic().parent_statement_id(node_id),
        )),
    )
}

/// Generate a [`Fix`] to make bindings in a statement explicit, either by adding them to `__all__`
/// or by changing them from `import a` to `import a as a`.
fn fix_by_reexporting<'a>(
    checker: &Checker,
    node_id: NodeId,
    imports: impl IntoIterator<Item = &'a ImportBinding<'a>>,
    dunder_all_exprs: &[&ast::Expr],
) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);

    let imports = {
        let mut imports: Vec<&str> = imports
            .into_iter()
            .map(ImportBinding::symbol_stored_in_outer_scope)
            .collect();
        if imports.is_empty() {
            bail!("Expected import bindings");
        }
        imports.sort_unstable();
        imports
    };

    let edits = match dunder_all_exprs {
        [] => fix::edits::make_redundant_alias(imports.into_iter(), statement),
        [dunder_all] => {
            fix::edits::add_to_dunder_all(imports.into_iter(), dunder_all, checker.stylist())
        }
        _ => bail!("Cannot offer a fix when there are multiple __all__ definitions"),
    };

    // Only emit a fix if there are edits.
    let mut tail = edits.into_iter();
    let head = tail.next().ok_or(anyhow!("No edits to make"))?;

    let isolation = Checker::isolation(checker.semantic().parent_statement_id(node_id));
    Ok(Fix::safe_edits(head, tail).isolate(isolation))
}

/// Returns an iterator over bindings to import statements that appear unused.
///
/// The stable behavior is to return those bindings to imports
/// satisfying the following properties:
///
/// - they are not shadowed
/// - they are not `global`, not `nonlocal`, and not explicit exports (i.e. `import foo as foo`)
/// - they have no references, according to the semantic model
///
/// Under preview, there is a more refined analysis performed
/// in the case where all bindings shadowed by a given import
/// binding (including the binding itself) are of a simple form:
/// they are required to be un-aliased imports or submodule imports.
///
/// This alternative analysis is described in the documentation for
/// [`unused_imports_from_binding`].
fn unused_imports_in_scope<'a, 'b>(
    semantic: &'a SemanticModel<'b>,
    scope: &'a Scope,
    settings: &'a LinterSettings,
) -> impl Iterator<Item = &'a Binding<'b>> {
    scope
        .binding_ids()
        .map(|id| (id, semantic.binding(id)))
        .filter(|(_, bdg)| {
            matches!(
                bdg.kind,
                BindingKind::Import(_)
                    | BindingKind::FromImport(_)
                    | BindingKind::SubmoduleImport(_)
            )
        })
        .filter(|(_, bdg)| !bdg.is_global() && !bdg.is_nonlocal() && !bdg.is_explicit_export())
        .flat_map(|(id, bdg)| {
            if is_refined_submodule_import_match_enabled(settings)
                // No need to apply refined logic if there is only a single binding
                && scope.shadowed_bindings(id).nth(1).is_some()
                // Only apply the new logic in certain situations to avoid
                // complexity, false positives, and intersection with
                // `redefined-while-unused` (`F811`).
                && has_simple_shadowed_bindings(scope, id, semantic)
            {
                unused_imports_from_binding(semantic, id, scope)
            } else if bdg.is_used() {
                vec![]
            } else {
                vec![bdg]
            }
        })
}

/// Returns a `Vec` of bindings to unused import statements that
/// are shadowed by a given binding.
///
/// This is best explained by example. So suppose we have:
///
/// ```python
/// import a
/// import a.b
/// import a.b.c
///
/// __all__ = ["a"]
///
/// a.b.foo()
/// ```
///
/// As of 2025-09-25, Ruff's semantic model, upon visiting
/// the whole module, will have a single live binding for
/// the symbol `a` that points to the line `import a.b.c`,
/// and the remaining two import bindings are considered shadowed
/// by the last.
///
/// This function expects to receive the `id`
/// for the live binding and will begin by collecting
/// all bindings shadowed by the given one - i.e. all
/// the different import statements binding the symbol `a`.
/// We iterate over references to this
/// module and decide (somewhat subjectively) which
/// import statement the user "intends" to reference. To that end,
/// to each reference we attempt to build a [`QualifiedName`]
/// corresponding to an iterated attribute access (e.g. `a.b.foo`).
/// We then determine the closest matching import statement to that
/// qualified name, and mark it as used.
///
/// In the present example, the qualified name associated to the
/// reference from the dunder all export is `"a"` and the qualified
/// name associated to the reference in the last line is `"a.b.foo"`.
/// The closest matches are `import a` and `import a.b`, respectively,
/// leaving `import a.b.c` unused.
///
/// For a precise definition of "closest match" see [`best_match`]
/// and [`rank_matches`].
///
/// Note: if any reference comes from something other than
/// a `Name` or a dunder all expression, then we return just
/// the original binding, thus reverting the stable behavior.
fn unused_imports_from_binding<'a, 'b>(
    semantic: &'a SemanticModel<'b>,
    id: BindingId,
    scope: &'a Scope,
) -> Vec<&'a Binding<'b>> {
    let mut marked = MarkedBindings::from_binding_id(semantic, id, scope);

    let binding = semantic.binding(id);

    // ensure we only do this once
    let mut marked_dunder_all = false;

    for ref_id in binding.references() {
        let resolved_reference = semantic.reference(ref_id);
        if !marked_dunder_all && resolved_reference.in_dunder_all_definition() {
            let first = *binding
                                .as_any_import()
                                .expect("binding to be import binding since current function called after restricting to these in `unused_imports_in_scope`")
                                .qualified_name()
                                .segments().first().expect("import binding to have nonempty qualified name");
            mark_uses_of_qualified_name(&mut marked, &QualifiedName::user_defined(first));
            marked_dunder_all = true;
            continue;
        }
        let Some(expr_id) = resolved_reference.expression_id() else {
            // If there is some other kind of reference, abandon
            // the refined approach for the usual one
            return vec![binding];
        };
        let Some(prototype) = expand_to_qualified_name_attribute(semantic, expr_id) else {
            return vec![binding];
        };

        mark_uses_of_qualified_name(&mut marked, &prototype);
    }

    marked.into_unused()
}

#[derive(Debug)]
struct MarkedBindings<'a, 'b> {
    bindings: Vec<&'a Binding<'b>>,
    used: Vec<bool>,
}

impl<'a, 'b> MarkedBindings<'a, 'b> {
    fn from_binding_id(semantic: &'a SemanticModel<'b>, id: BindingId, scope: &'a Scope) -> Self {
        let bindings: Vec<_> = scope
            .shadowed_bindings(id)
            .map(|id| semantic.binding(id))
            .collect();

        Self {
            used: vec![false; bindings.len()],
            bindings,
        }
    }

    fn into_unused(self) -> Vec<&'a Binding<'b>> {
        self.bindings
            .into_iter()
            .zip(self.used)
            .filter_map(|(bdg, is_used)| (!is_used).then_some(bdg))
            .collect()
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = (&'a Binding<'b>, &mut bool)> {
        self.bindings.iter().copied().zip(self.used.iter_mut())
    }
}

/// Returns `Some` [`QualifiedName`] delineating the path for the
/// maximal [`ExprName`] or [`ExprAttribute`] containing the expression
/// associated to the given [`NodeId`], or `None` otherwise.
///
/// For example, if the `expr_id` points to `a` in `a.b.c.foo()`
/// then the qualified name would have segments [`a`, `b`, `c`, `foo`].
fn expand_to_qualified_name_attribute<'b>(
    semantic: &SemanticModel<'b>,
    expr_id: NodeId,
) -> Option<QualifiedName<'b>> {
    let mut builder = QualifiedNameBuilder::with_capacity(16);

    let mut expr_id = expr_id;

    let expr = semantic.expression(expr_id)?;

    let name = expr.as_name_expr()?;

    builder.push(&name.id);

    while let Some(node_id) = semantic.parent_expression_id(expr_id) {
        let Some(expr) = semantic.expression(node_id) else {
            break;
        };
        let Some(expr_attr) = expr.as_attribute_expr() else {
            break;
        };
        builder.push(expr_attr.attr.as_str());
        expr_id = node_id;
    }
    Some(builder.build())
}

fn mark_uses_of_qualified_name(marked: &mut MarkedBindings, prototype: &QualifiedName) {
    let Some(best) = best_match(&marked.bindings, prototype) else {
        return;
    };

    let Some(best_import) = best.as_any_import() else {
        return;
    };

    let best_name = best_import.qualified_name();

    // We loop through all bindings in case there are repeated instances
    // of the `best_name`. For example, if we have
    //
    // ```python
    // import a
    // import a
    //
    // a.foo()
    // ```
    //
    // then we want to mark both import statements as used. It
    // is the job of `redefined-while-unused` (`F811`) to catch
    // the repeated binding in this case.
    for (binding, is_used) in marked.iter_mut() {
        if *is_used {
            continue;
        }

        if binding
            .as_any_import()
            .is_some_and(|imp| imp.qualified_name() == best_name)
        {
            *is_used = true;
        }
    }
}

/// Returns a pair with first component the length of the largest
/// shared prefix between the qualified name of the import binding
/// and the `prototype` and second component the length of the
/// qualified name of the import binding (i.e. the number of path
/// segments). Moreover, we regard the second component as ordered
/// in reverse.
///
/// For example, if the binding corresponds to `import a.b.c`
/// and the prototype to `a.b.foo()`, then the function returns
/// `(2,std::cmp::Reverse(3))`.
fn rank_matches(binding: &Binding, prototype: &QualifiedName) -> (usize, std::cmp::Reverse<usize>) {
    let Some(import) = binding.as_any_import() else {
        unreachable!()
    };
    let qname = import.qualified_name();
    let left = qname
        .segments()
        .iter()
        .zip(prototype.segments())
        .take_while(|(x, y)| x == y)
        .count();
    (left, std::cmp::Reverse(qname.segments().len()))
}

/// Returns the import binding that shares the longest prefix
/// with the `prototype` and is of minimal length amongst these.
///
/// See also [`rank_matches`].
fn best_match<'a, 'b>(
    bindings: &Vec<&'a Binding<'b>>,
    prototype: &QualifiedName,
) -> Option<&'a Binding<'b>> {
    bindings
        .iter()
        .copied()
        .max_by_key(|binding| rank_matches(binding, prototype))
}

#[inline]
fn has_simple_shadowed_bindings(scope: &Scope, id: BindingId, semantic: &SemanticModel) -> bool {
    let Some(binding_node) = semantic.binding(id).source else {
        return false;
    };

    scope.shadowed_bindings(id).enumerate().all(|(i, shadow)| {
        let shadowed_binding = semantic.binding(shadow);
        // Bail if one of the shadowed bindings is
        // used before the last live binding. This is
        // to avoid situations like this:
        //
        // ```
        // import a
        // a.b
        // import a.b
        // ```
        if i > 0 && shadowed_binding.is_used() {
            return false;
        }
        // We want to allow a situation like this:
        //
        // ```python
        // import a.b
        // if TYPE_CHECKING:
        //     import a.b.c
        // ```
        // but bail in a situation like this:
        //
        // ```python
        // try:
        //     import a.b
        // except ImportError:
        //     import argparse
        //     import a
        //     a.b = argparse.Namespace()
        // ```
        //
        // So we require that all the shadowed bindings dominate the
        // last live binding for the import. That is: if the last live
        // binding is executed it should imply that all the shadowed
        // bindings were executed as well.
        if shadowed_binding
            .source
            .is_none_or(|node_id| !semantic.dominates(node_id, binding_node))
        {
            return false;
        }
        matches!(
            shadowed_binding.kind,
            BindingKind::Import(_) | BindingKind::SubmoduleImport(_)
        ) && !shadowed_binding.flags.contains(BindingFlags::ALIAS)
    })
}
