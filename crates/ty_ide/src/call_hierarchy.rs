//! LSP **Call Hierarchy** support.
//!
//! Implements `textDocument/prepareCallHierarchy`, `callHierarchy/incomingCalls`,
//! and `callHierarchy/outgoingCalls`.
//!
//! The three entry points are deliberately not `#[salsa::tracked]`, matching the
//! `goto_definition` / `find_references` / `prepare_type_hierarchy` precedents.
//! AST access goes through the salsa-cached `parsed_module`, which preserves
//! incrementality without forcing the entry points themselves to be tracked.

pub(crate) mod incoming_calls;
pub(crate) mod outgoing_calls;

use crate::goto::{GotoTarget, find_goto_target};
use crate::{Db, SymbolKind};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::CoveringNode;
use ruff_python_ast::name::Name;
use ruff_python_ast::token::Tokens;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_module_resolver::ResolverFile;
use ty_python_core::ProgramFile;
use ty_python_core::definition::DefinitionKind;
use ty_python_semantic::{ImportAliasResolution, ResolvedDefinition, SemanticModel};

/// Resolve the symbol at `offset` to a list of [`CallHierarchyItem`]s.
///
/// Returns `None` when the cursor is not on a function, method, or class — only
/// callable definitions can anchor a call hierarchy. Returns one item per
/// resolved definition; the cursor on an overload implementation or a call site
/// of an overloaded function yields one item per overload candidate, while the
/// cursor on a specific `@overload def` yields just that one.
pub fn prepare_call_hierarchy(
    db: &dyn Db,
    file: ProgramFile<'_>,
    offset: TextSize,
) -> Option<Vec<CallHierarchyItem>> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &module, offset)?;
    let definitions = goto_target
        .definitions(&model, ImportAliasResolution::ResolveAliases)?
        .goto_declaration(&model, &goto_target)?;

    let mut items = Vec::new();
    for resolved in &definitions {
        let Some(def) = resolved.definition() else {
            continue;
        };

        let module_ref = parsed_module(db, def.python_file(db)).load(db);

        if let Some(item) = CallHierarchyItem::from_definition(db, resolved, &module_ref) {
            items.push(item);
        }
    }

    if items.is_empty() { None } else { Some(items) }
}

/// One node in a call hierarchy.
///
/// Mirrors `lsp_types::CallHierarchyItem` but in ty's domain types — the LSP-layer
/// conversion happens in `ty_server`.
#[derive(Debug, Clone)]
pub struct CallHierarchyItem {
    pub name: Name,
    pub kind: SymbolKind,
    /// The containing module, matching the detail shown for type hierarchy items.
    pub detail: Option<String>,
    /// The file containing the callable definition.
    pub file: File,
    /// Full range of the definition (or full file range for `Module`).
    pub full_range: TextRange,
    /// Selection range — the symbol name. Used as the stateless key when the
    /// LSP client re-sends this item to `incomingCalls` / `outgoingCalls`.
    pub selection_range: TextRange,
}

impl CallHierarchyItem {
    /// Build a [`CallHierarchyItem`] from a resolved definition, returning `None`
    /// for kinds that are not callable (variables, type aliases, parameters, ...).
    ///
    /// Takes an already-loaded `ParsedModuleRef` for `def.file(db)` so the name
    /// is read directly from it instead of going through `def.name(db)`, which
    /// would re-load the module internally.
    fn from_definition(
        db: &dyn Db,
        resolved: &ResolvedDefinition<'_>,
        module: &ruff_db::parsed::ParsedModuleRef,
    ) -> Option<CallHierarchyItem> {
        let def = resolved.definition()?;
        let def_file = def.file(db);
        let def_kind = def.kind(db);

        let name = def.name(db)?;

        let kind = match def_kind {
            DefinitionKind::Function(_) => {
                SymbolKind::function_kind(&name, def.scope(db).scope(db).kind().is_class())
            }

            DefinitionKind::Class(_) => SymbolKind::Class,

            _ => return None,
        };

        Some(CallHierarchyItem {
            name: Name::new(name),
            kind,
            detail: module_detail(db, def.program_file(db).resolver_file(db)),
            file: def_file,
            full_range: def.full_range(db, module).range(),
            selection_range: def.focus_range(db, module).range(),
        })
    }
}

fn module_detail(db: &dyn Db, file: ResolverFile<'_>) -> Option<String> {
    ty_module_resolver::file_to_module(db, file).map(|module| module.name(db).to_string())
}

#[cfg(test)]
pub(super) fn snapshot_item_label(item: &CallHierarchyItem) -> String {
    let label = format!("{}: `{}`", item.kind.to_string(), item.name);
    match &item.detail {
        Some(detail) => format!("{label} (`{detail}`)"),
        None => label,
    }
}

/// The relevant node + offset for resolving the callee of a call site. For
/// `foo(...)` this is the `ExprName` of `foo`; for `obj.foo(...)` it is the
/// `Identifier` of `foo` in the attribute access.
#[derive(Clone, Copy)]
enum CalleeLeaf<'a> {
    Name(&'a ast::ExprName),
    AttrIdentifier {
        attribute: &'a ast::ExprAttribute,
        identifier: &'a ast::Identifier,
    },
}

impl<'a> CalleeLeaf<'a> {
    fn from_expr(expr: &'a ast::Expr) -> Option<Self> {
        match expr {
            ast::Expr::Name(name) => Some(CalleeLeaf::Name(name)),
            ast::Expr::Attribute(attr) => Some(CalleeLeaf::AttrIdentifier {
                attribute: attr,
                identifier: &attr.attr,
            }),
            _ => None,
        }
    }

    /// Build a `CoveringNode` whose leaf is the callee identifier and run
    /// `GotoTarget::from_covering_node`. Returns the resolved goto target and the
    /// callee's range (the range LSP wants for `from_ranges`).
    fn resolve(
        self,
        model: &'a SemanticModel,
        tokens: &Tokens,
        ancestors: &[AnyNodeRef<'a>],
    ) -> Option<(GotoTarget<'a>, TextRange)> {
        // Construct the leaf stack the way `find_goto_target_impl` does: the leaf
        // node has to be the identifier/name, with `ExprAttribute` (for attribute
        // calls) sitting just above it so `from_covering_node`'s `Identifier` arm
        // walks up to the `ExprCall` grandparent.
        let mut stack: Vec<AnyNodeRef<'_>> = ancestors.to_vec();
        let call_site_range = match self {
            CalleeLeaf::Name(name) => {
                stack.push(AnyNodeRef::from(name));
                name.range
            }
            CalleeLeaf::AttrIdentifier {
                attribute,
                identifier,
            } => {
                stack.push(AnyNodeRef::from(attribute));
                stack.push(AnyNodeRef::from(identifier));
                identifier.range
            }
        };
        let covering = CoveringNode::from_ancestors(stack);
        let goto_target =
            GotoTarget::from_covering_node(model, &covering, call_site_range.start(), tokens)?;
        Some((goto_target, call_site_range))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};

    impl CursorTest {
        pub(super) fn prepare_calls(&self) -> Option<Vec<CallHierarchyItem>> {
            prepare_call_hierarchy(
                &self.db,
                self.program_file(self.cursor.file),
                self.cursor.offset,
            )
        }

        fn prepare_call_hierarchy(&self) -> String {
            let Some(items) = self.prepare_calls() else {
                return "No call hierarchy items found".to_string();
            };

            self.render_diagnostics(items.into_iter().map(CallHierarchyItemDiagnostic::new))
        }
    }

    struct CallHierarchyItemDiagnostic {
        item: CallHierarchyItem,
    }

    impl CallHierarchyItemDiagnostic {
        fn new(item: CallHierarchyItem) -> Self {
            Self { item }
        }
    }

    impl IntoDiagnostic for CallHierarchyItemDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("prepare-call-hierarchy")),
                Severity::Info,
                snapshot_item_label(&self.item),
            );
            diagnostic.annotate(Annotation::primary(
                Span::from(self.item.file).with_range(self.item.selection_range),
            ));
            diagnostic
        }
    }

    #[test]
    fn prepare_on_function_def() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Function: `foo` (`main`)
         --> main.py:2:5
          |
        2 | def foo():
          |     ^^^
          |
        ");
    }

    #[test]
    fn prepare_on_class_def() {
        let test = cursor_test(
            r#"
            class My<CURSOR>Class:
                pass
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Class: `MyClass` (`main`)
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
          |
        ");
    }

    #[test]
    fn prepare_on_method() {
        let test = cursor_test(
            r#"
            class C:
                def me<CURSOR>thod(self):
                    pass
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Method: `method` (`main`)
         --> main.py:3:9
          |
        3 |     def method(self):
          |         ^^^^^^
          |
        ");
    }

    #[test]
    fn prepare_on_call_site() {
        let test = cursor_test(
            r#"
            def foo():
                pass

            f<CURSOR>oo()
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Function: `foo` (`main`)
         --> main.py:2:5
          |
        2 | def foo():
          |     ^^^
          |
        ");
    }

    #[test]
    fn prepare_on_non_callable_returns_none() {
        let test = cursor_test(
            r#"
            x = 4<CURSOR>2
            "#,
        );
        assert!(test.prepare_calls().is_none());
    }

    #[test]
    fn prepare_on_overloaded_function() {
        // `prepare_call_hierarchy`'s doc promises overload groups surface as
        // multiple items. Cursor placed on the implementation def so the
        // resolution covers the whole group rather than a single `@overload`.
        let test = cursor_test(
            r#"
            from typing import overload

            @overload
            def foo(x: int) -> int: ...
            @overload
            def foo(x: str) -> str: ...
            def f<CURSOR>oo(x):
                return x
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Function: `foo` (`main`)
         --> main.py:5:5
          |
        5 | def foo(x: int) -> int: ...
          |     ^^^
          |

        info[prepare-call-hierarchy]: Function: `foo` (`main`)
         --> main.py:7:5
          |
        7 | def foo(x: str) -> str: ...
          |     ^^^
          |

        info[prepare-call-hierarchy]: Function: `foo` (`main`)
         --> main.py:8:5
          |
        8 | def foo(x):
          |     ^^^
          |
        ");
    }

    #[test]
    fn prepare_on_async_function() {
        // `CallHierarchyItemKind::Function`'s rustdoc states `async def` is
        // covered. Verify it directly.
        let test = cursor_test(
            r#"
            async def f<CURSOR>oo():
                pass
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Function: `foo` (`main`)
         --> main.py:2:11
          |
        2 | async def foo():
          |           ^^^
          |
        ");
    }

    #[test]
    fn prepare_on_staticmethod() {
        let test = cursor_test(
            r#"
            class C:
                @staticmethod
                def m<CURSOR>ethod():
                    pass
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Method: `method` (`main`)
         --> main.py:4:9
          |
        4 |     def method():
          |         ^^^^^^
          |
        ");
    }

    #[test]
    fn prepare_on_classmethod() {
        let test = cursor_test(
            r#"
            class C:
                @classmethod
                def m<CURSOR>ethod(cls):
                    pass
            "#,
        );
        insta::assert_snapshot!(test.prepare_call_hierarchy(), @"
        info[prepare-call-hierarchy]: Method: `method` (`main`)
         --> main.py:4:9
          |
        4 |     def method(cls):
          |         ^^^^^^
          |
        ");
    }
}
