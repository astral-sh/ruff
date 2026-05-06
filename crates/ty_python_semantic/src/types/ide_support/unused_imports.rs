use get_size2::GetSize;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{self as ast, helpers::is_dunder, name::Name};
use ruff_text_size::TextRange;
use ty_python_core::definition::{DefinitionKind, DefinitionState};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::{NodeWithScopeKind, ScopeKind};
use ty_python_core::semantic_index;

use crate::Db;
use crate::dunder_all::dunder_all_names;

#[derive(Debug, Clone, Eq, PartialEq, Hash, GetSize)]
pub struct UnusedImport {
    pub range: TextRange,
    pub name: Name,
}

/// Returns `true` for concrete import aliases that can produce unused-import hints.
///
/// Star imports have no precise target, and explicit reexports are intentional public API.
fn should_report_import(kind: &DefinitionKind<'_>) -> bool {
    matches!(
        kind,
        DefinitionKind::Import(_) | DefinitionKind::ImportFrom(_)
    ) && !kind.is_reexported()
}

fn is_future_import(kind: &DefinitionKind<'_>, parsed: &ruff_db::parsed::ParsedModuleRef) -> bool {
    match kind {
        DefinitionKind::Import(import) => import.alias(parsed).name.id.as_str() == "__future__",
        DefinitionKind::ImportFrom(import_from) => {
            import_from.import(parsed).module.as_deref() == Some("__future__")
        }
        _ => false,
    }
}

fn is_intentionally_unused_name(name: &Name) -> bool {
    name == "_" || is_dunder(name.as_str())
}

fn import_target(
    kind: &DefinitionKind<'_>,
    parsed: &ruff_db::parsed::ParsedModuleRef,
) -> Option<(TextRange, Name)> {
    let alias = match kind {
        DefinitionKind::Import(import) => import.alias(parsed),
        DefinitionKind::ImportFrom(import_from) => import_from.alias(parsed),
        _ => return None,
    };

    let target = alias.asname.as_ref().unwrap_or(&alias.name);
    Some((target.range, target.id.clone()))
}

fn unaliased_multipart_import_name<'a>(
    kind: &'a DefinitionKind<'_>,
    parsed: &'a ruff_db::parsed::ParsedModuleRef,
) -> Option<&'a str> {
    let DefinitionKind::Import(import) = kind else {
        return None;
    };

    let alias = import.alias(parsed);
    let name = alias.name.id.as_str();

    (alias.asname.is_none() && name.contains('.')).then_some(name)
}

fn dotted_name(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Name(name) => Some(name.id.to_string()),
        ast::Expr::Attribute(attribute) => {
            let mut name = dotted_name(&attribute.value)?;
            name.push('.');
            name.push_str(attribute.attr.id.as_str());
            Some(name)
        }
        _ => None,
    }
}

struct MultipartImportUseVisitor<'a> {
    imported_name: &'a str,
    used: bool,
}

impl<'a> SourceOrderVisitor<'a> for MultipartImportUseVisitor<'_> {
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if self.used {
            return;
        }

        if let ast::Expr::Attribute(attribute) = expr
            && matches!(attribute.ctx, ast::ExprContext::Load)
            && let Some(name) = dotted_name(expr)
            && multipart_name_matches(&name, self.imported_name)
        {
            self.used = true;
            return;
        }

        source_order::walk_expr(self, expr);
    }
}

fn multipart_name_matches(name: &str, imported_name: &str) -> bool {
    let imported_member_prefix = format!("{imported_name}.");
    name == imported_name || name.starts_with(&imported_member_prefix)
}

fn visit_class_body_stmt_for_multipart_usage(
    visitor: &mut MultipartImportUseVisitor<'_>,
    stmt: &ast::Stmt,
) {
    if !matches!(stmt, ast::Stmt::ClassDef(_) | ast::Stmt::FunctionDef(_)) {
        visitor.visit_stmt(stmt);
    }
}

fn multipart_import_is_used_in_scope(
    parsed: &ruff_db::parsed::ParsedModuleRef,
    scope_node: &NodeWithScopeKind,
    imported_name: &str,
) -> bool {
    let mut visitor = MultipartImportUseVisitor {
        imported_name,
        used: false,
    };

    match scope_node {
        NodeWithScopeKind::Module => {
            for stmt in parsed.suite() {
                visitor.visit_stmt(stmt);
                if visitor.used {
                    break;
                }
            }
        }
        NodeWithScopeKind::Class(class) => {
            for stmt in &class.node(parsed).body {
                visit_class_body_stmt_for_multipart_usage(&mut visitor, stmt);
                if visitor.used {
                    break;
                }
            }
        }
        NodeWithScopeKind::Function(function) => {
            for stmt in &function.node(parsed).body {
                visitor.visit_stmt(stmt);
                if visitor.used {
                    break;
                }
            }
        }
        NodeWithScopeKind::Lambda(lambda) => visitor.visit_expr(&lambda.node(parsed).body),
        _ => {}
    }

    visitor.used
}

/// Returns unused import aliases for IDE-facing unnecessary hints.
///
/// This is intentionally file-local. It reports imports that are unused in their defining file
/// unless the file explicitly reexports them with `as` aliases or `__all__`. This can report an
/// implicit package export as unused; make the export explicit to suppress the hint.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub fn unused_imports(db: &dyn Db, file: File) -> Vec<UnusedImport> {
    let parsed = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);
    let explicit_exports = dunder_all_names(db, file);
    let mut unused = Vec::new();

    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);
        let scope = index.scope(file_scope_id);
        let is_module_scope = matches!(scope.kind(), ScopeKind::Module);

        if matches!(scope.kind(), ScopeKind::TypeParams | ScopeKind::TypeAlias) {
            continue;
        }

        let place_table = index.place_table(file_scope_id);
        let use_def_map = index.use_def_map(file_scope_id);

        for (_, state, is_used) in use_def_map.all_definitions_with_usage() {
            let DefinitionState::Defined(definition) = state else {
                continue;
            };

            let kind = definition.kind(db);
            if !should_report_import(kind) || is_future_import(kind, &parsed) {
                continue;
            }

            let multipart_import_name = unaliased_multipart_import_name(kind, &parsed);
            if multipart_import_name
                .is_some_and(|name| multipart_import_is_used_in_scope(&parsed, scope.node(), name))
            {
                continue;
            }

            if is_used && multipart_import_name.is_none() {
                continue;
            }

            let ScopedPlaceId::Symbol(symbol_id) = definition.place(db) else {
                continue;
            };

            let symbol = place_table.symbol(symbol_id);
            let name = symbol.name();

            if is_intentionally_unused_name(name)
                || (multipart_import_name.is_none()
                    && is_module_scope
                    && explicit_exports
                        .as_ref()
                        .is_some_and(|exports| exports.contains(name)))
            {
                continue;
            }

            let Some((range, name)) = import_target(kind, &parsed) else {
                continue;
            };

            unused.push(UnusedImport { range, name });
        }
    }

    unused.sort_unstable_by_key(|import| (import.range.start(), import.range.end()));
    unused.dedup_by_key(|import| import.range);
    unused
}

#[cfg(test)]
mod tests {
    use super::unused_imports;
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;
    use ruff_python_trivia::textwrap::dedent;

    fn collect_unused_entries_in_file(
        path: &str,
        source: &str,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let source = dedent(source);
        let db = TestDbBuilder::new().with_file(path, &source).build()?;
        let file = system_path_to_file(&db, path)?;
        let mut entries = unused_imports(&db, file)
            .iter()
            .map(|import| {
                (
                    import.name.to_string(),
                    source[usize::from(import.range.start())..usize::from(import.range.end())]
                        .to_string(),
                )
            })
            .collect::<Vec<_>>();
        entries.sort();
        Ok(entries)
    }

    fn collect_unused_entries(source: &str) -> anyhow::Result<Vec<(String, String)>> {
        collect_unused_entries_in_file("/src/main.py", source)
    }

    fn collect_unused_names_in_file(path: &str, source: &str) -> anyhow::Result<Vec<String>> {
        let db = TestDbBuilder::new()
            .with_file(path, &dedent(source))
            .build()?;
        let file = system_path_to_file(&db, path)?;
        let mut names = unused_imports(&db, file)
            .iter()
            .map(|import| import.name.to_string())
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn collect_unused_names(source: &str) -> anyhow::Result<Vec<String>> {
        collect_unused_names_in_file("/src/main.py", source)
    }

    #[test]
    fn reports_basic_unused_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os
            import sys

            print(sys.version)
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn reports_import_forms_and_alias_ranges() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path
            import json.decoder as decoder
            from os import path
            from sys import version as sys_version
            "#,
        )?;

        assert_eq!(
            entries,
            vec![
                ("decoder".to_string(), "decoder".to_string()),
                ("os.path".to_string(), "os.path".to_string()),
                ("path".to_string(), "path".to_string()),
                ("sys_version".to_string(), "sys_version".to_string()),
            ]
        );
        Ok(())
    }

    #[test]
    fn reports_alias_ranges_in_multi_import_statements() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os as operating_system, sys as system
            from os import path as os_path, sep as separator

            print(system.version)
            print(separator)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![
                (
                    "operating_system".to_string(),
                    "operating_system".to_string()
                ),
                ("os_path".to_string(), "os_path".to_string()),
            ]
        );
        Ok(())
    }

    #[test]
    fn reports_alias_ranges_in_parenthesized_from_imports() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            from os import (
                path as os_path,
                sep,
            )

            print(sep)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os_path".to_string(), "os_path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_used_import_forms() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os.path
            import json.decoder as decoder
            from os import path
            from sys import version as sys_version

            print(os.path.join("a", "b"))
            print(decoder.JSONDecoder)
            print(path.join("a", "b"))
            print(sys_version)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_used_aliased_multipart_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os.path as path

            print(path.join("a", "b"))
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_unused_aliased_multipart_imports() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path as path

            print(os.path)
            "#,
        )?;

        assert_eq!(entries, vec![("path".to_string(), "path".to_string())]);
        Ok(())
    }

    #[test]
    fn reports_partially_used_import_lists() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os, sys
            from os import path, sep

            print(sys.version)
            print(sep)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![
                ("os".to_string(), "os".to_string()),
                ("path".to_string(), "path".to_string()),
            ]
        );
        Ok(())
    }

    #[test]
    fn reports_partially_used_multipart_import_lists() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path, os.pathsep

            print(os.path.join("a", "b"))
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.pathsep".to_string(), "os.pathsep".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_multipart_import_used_as_exact_dotted_name() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os.path

            print(os.path)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_multipart_import_used_as_dotted_prefix() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import xml.etree.ElementTree

            print(xml.etree.ElementTree.Element)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_module_scope_multipart_import_used_from_function_scope() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os.path

            def f():
                print(os.path.join("a", "b"))
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_function_scope_multipart_import_used_from_nested_scope() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            def f():
                import os.path

                def g():
                    return os.path.join("a", "b")

                return g()
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_function_scope_multipart_import_used_only_from_sibling_scope() -> anyhow::Result<()>
    {
        let entries = collect_unused_entries(
            r#"
            def f():
                import os.path

            def g():
                print(os.path.join("a", "b"))
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_when_only_parent_package_is_used() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import xml.etree.ElementTree

            print(xml.etree)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![(
                "xml.etree.ElementTree".to_string(),
                "xml.etree.ElementTree".to_string()
            )]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_when_only_root_is_used() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path

            print(os)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_when_only_similar_dotted_name_is_used() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path

            print(os.pathsep)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_when_only_assigned() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path

            os.path = None
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_multipart_import_when_member_is_assigned() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os.path

            os.path.join = None
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn dunder_all_does_not_suppress_multipart_imports() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            import os.path

            __all__ = ["os"]
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_relative_imports_and_alias_ranges() -> anyhow::Result<()> {
        let entries = collect_unused_entries_in_file(
            "/src/pkg/module.py",
            r#"
            from . import sibling
            from .subpackage import helper as local_helper
            "#,
        )?;

        assert_eq!(
            entries,
            vec![
                ("local_helper".to_string(), "local_helper".to_string()),
                ("sibling".to_string(), "sibling".to_string()),
            ]
        );
        Ok(())
    }

    #[test]
    fn reports_function_scope_unused_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            def f():
                import os
                import sys
                print(sys.version)
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn reports_class_scope_unused_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            class C:
                import os
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn reports_class_scope_multipart_import_used_only_from_method_scope() -> anyhow::Result<()> {
        let entries = collect_unused_entries(
            r#"
            class C:
                import os.path

                def method(self):
                    print(os.path.join("a", "b"))
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_reexports_and_dunder_all() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os as os
            from json import decoder as decoder
            import sys

            __all__ = ["sys"]
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn dunder_all_only_suppresses_listed_module_scope_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import json
            import sys

            __all__ = ["sys"]
            "#,
        )?;

        assert_eq!(names, vec!["json"]);
        Ok(())
    }

    #[test]
    fn skips_star_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            from os import *
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn dunder_all_only_applies_to_module_scope_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            __all__ = ["sys"]

            def f():
                import sys
            "#,
        )?;

        assert_eq!(names, vec!["sys"]);
        Ok(())
    }

    #[test]
    fn reports_private_import_aliases() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os as _os
            from sys import version as _version
            "#,
        )?;

        assert_eq!(names, vec!["_os", "_version"]);
        Ok(())
    }

    #[test]
    fn skips_intentionally_unused_import_aliases() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            import os as _
            import sys as __sys__
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_future_imports() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            from __future__ import annotations
            import __future__
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_stub_file_unused_imports() -> anyhow::Result<()> {
        let entries = collect_unused_entries_in_file(
            "/src/main.pyi",
            r#"
            import os
            import sys as sys
            from os import PathLike

            def f(path: PathLike[str]) -> None: ...
            "#,
        )?;

        assert_eq!(entries, vec![("os".to_string(), "os".to_string())]);
        Ok(())
    }

    #[test]
    fn skips_imports_used_only_in_annotations() -> anyhow::Result<()> {
        let names = collect_unused_names(
            r#"
            from __future__ import annotations
            from os import PathLike
            from typing import TypeAlias

            Path: TypeAlias = PathLike[str]
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }
}
