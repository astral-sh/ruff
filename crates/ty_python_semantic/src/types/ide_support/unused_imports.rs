use get_size2::GetSize;
use ruff_db::files::File;
use ruff_db::parsed::{parsed_module, parsed_string_annotation};
use ruff_db::source::{SourceText, source_text};
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{
    self as ast, AnyNodeRef,
    helpers::is_dunder,
    name::{Name, UnqualifiedName},
};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState, dotted_starts_with};
use ty_python_core::scope::ScopeKind;
use ty_python_core::semantic_index;

use super::visible_reachable_definitions_for_name;
use crate::Db;
use crate::dunder_all::dunder_all_names;
use crate::semantic_model::SemanticModel;
use crate::types::TypeContext;
use crate::types::infer::{infer_deferred_types, infer_definition_types, infer_scope_types};

#[derive(Debug, Clone, Eq, PartialEq, Hash, GetSize)]
pub struct UnusedImport {
    pub range: TextRange,
    pub name: Name,
}

/// Returns unused import aliases for IDE-facing unnecessary hints.
///
/// This is intentionally file-local. It reports imports that are unused in their defining file
/// unless the file explicitly reexports them with `as` aliases or `__all__`. This can report an
/// implicit package export as unused; make the export explicit to suppress the hint.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub fn unused_imports(db: &dyn Db, file: File) -> Box<[UnusedImport]> {
    let parsed = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);
    let mut string_annotation_definitions = None;
    let mut explicit_exports = None;
    let mut member_attribute_names: Option<FxHashSet<&str>> = None;
    let mut unused = Vec::new();

    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);
        let scope = index.scope(file_scope_id);
        let is_module_scope = scope.kind().is_module();

        if matches!(scope.kind(), ScopeKind::TypeParams | ScopeKind::TypeAlias) {
            continue;
        }

        let use_def_map = index.use_def_map(file_scope_id);

        for (definition_id, state, is_used) in use_def_map.all_definitions_with_usage() {
            let DefinitionState::Defined(definition) = state else {
                continue;
            };

            let kind = definition.kind(db);
            if !should_report_import(kind) || kind.is_future_import(&parsed) {
                continue;
            }

            if is_module_scope && kind.is_reexported() {
                continue;
            }

            let multipart_import_name = kind.unaliased_multipart_import_name(&parsed);
            let is_used = if multipart_import_name.is_some() {
                use_def_map.is_multipart_import_definition_used(definition_id)
            } else {
                is_used
            };

            if is_used {
                continue;
            }

            let Some((range, display_name)) = import_target(kind, &parsed) else {
                continue;
            };

            if is_intentionally_unused_name(&display_name) {
                continue;
            }

            // Class-body imports can be used as attributes (`self.os`), which records
            // a member place without marking the class-scope symbol used.
            // TODO: Match by the accessed object's type instead of by name alone.
            if scope.kind().is_class()
                && member_attribute_names
                    .get_or_insert_with(|| {
                        index
                            .scope_ids()
                            .flat_map(|scope_id| {
                                index
                                    .place_table(scope_id.file_scope_id(db))
                                    .members()
                                    .filter_map(|member| member.first_attribute_name())
                            })
                            .collect()
                    })
                    .contains(display_name.as_str())
            {
                continue;
            }

            // Multipart imports additionally require a dotted path in some string
            // annotation to go through the imported submodule.
            let string_annotation_uses = string_annotation_definitions
                .get_or_insert_with(|| string_annotation_used_definitions(db, file));
            let used_in_string_annotation =
                string_annotation_uses.definitions.contains(&definition)
                    && multipart_import_name.is_none_or(|imported_name| {
                        string_annotation_uses
                            .dotted_names
                            .iter()
                            .any(|dotted| dotted_starts_with(dotted.split('.'), imported_name))
                    });

            if used_in_string_annotation {
                continue;
            }

            let is_explicit_export = multipart_import_name.is_none()
                && is_module_scope
                && explicit_exports
                    .get_or_insert_with(|| dunder_all_names(db, file))
                    .as_ref()
                    .is_some_and(|exports| exports.contains(&display_name));

            if is_explicit_export {
                continue;
            }

            unused.push(UnusedImport {
                range,
                name: display_name,
            });
        }
    }

    unused.sort_unstable_by_key(|import| (import.range.start(), import.range.end()));
    unused.dedup_by_key(|import| import.range);
    unused.into_boxed_slice()
}

/// Definitions and dotted attribute paths referenced from string annotations.
struct StringAnnotationUses<'db> {
    definitions: FxHashSet<Definition<'db>>,
    dotted_names: FxHashSet<Box<str>>,
}

/// Resolves the strings that inference classified as type expressions.
///
/// Inference retains the node keys of every string it parsed as an annotation, so
/// classification is inherited from the checker. Only name resolution and dotted
/// paths are re-derived here.
fn string_annotation_used_definitions(db: &dyn Db, file: File) -> StringAnnotationUses<'_> {
    let index = semantic_index(db, file);

    let mut annotation_keys: FxHashSet<ExpressionNodeKey> = FxHashSet::default();
    for scope_id in index.scope_ids() {
        annotation_keys
            .extend(infer_scope_types(db, scope_id, TypeContext::default()).string_annotations());

        let file_scope_id = scope_id.file_scope_id(db);
        let use_def = index.use_def_map(file_scope_id);
        for (_, state, _) in use_def.all_definitions_with_usage() {
            let DefinitionState::Defined(definition) = state else {
                continue;
            };
            let inference = infer_definition_types(db, definition);
            annotation_keys.extend(inference.string_annotations());
            for deferred in inference.deferred_definitions() {
                annotation_keys.extend(infer_deferred_types(db, *deferred).string_annotations());
            }
        }
    }

    let mut uses = StringAnnotationUses {
        definitions: FxHashSet::default(),
        dotted_names: FxHashSet::default(),
    };

    if annotation_keys.is_empty() {
        return uses;
    }

    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, file);
    let model = SemanticModel::new(db, file);

    let mut collector = StringLiteralCollector {
        annotation_keys: &annotation_keys,
        strings: Vec::new(),
    };
    for stmt in parsed.suite() {
        collector.visit_stmt(stmt);
    }

    for string in collector.strings {
        let mut visitor = StringAnnotationResolver {
            model: &model,
            source: &source,
            annotation_keys: &annotation_keys,
            scope_node: string.into(),
            uses: &mut uses,
        };
        visitor.visit_string(string);
    }

    uses
}

/// Collects the file-level string literals that inference classified as annotations.
struct StringLiteralCollector<'a, 'ast> {
    annotation_keys: &'a FxHashSet<ExpressionNodeKey>,
    strings: Vec<&'ast ast::ExprStringLiteral>,
}

impl<'ast> SourceOrderVisitor<'ast> for StringLiteralCollector<'_, 'ast> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        if let ast::Expr::StringLiteral(string) = expr {
            if self.annotation_keys.contains(&expr.into()) {
                self.strings.push(string);
            }
        } else {
            source_order::walk_expr(self, expr);
        }
    }
}

struct StringAnnotationResolver<'a, 'db> {
    model: &'a SemanticModel<'db>,
    source: &'a SourceText,
    annotation_keys: &'a FxHashSet<ExpressionNodeKey>,
    /// The file-level string literal, used to resolve names in its enclosing scope.
    scope_node: AnyNodeRef<'a>,
    uses: &'a mut StringAnnotationUses<'db>,
}

impl StringAnnotationResolver<'_, '_> {
    fn visit_string(&mut self, string: &ast::ExprStringLiteral) {
        let Some(string_literal) = string.as_single_part_string() else {
            return;
        };
        let Ok(parsed) = parsed_string_annotation(self.source.as_str(), string_literal) else {
            return;
        };
        self.visit_expr(parsed.expr());
    }
}

impl<'ast> SourceOrderVisitor<'ast> for StringAnnotationResolver<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        match expr {
            ast::Expr::Name(name) => {
                self.uses
                    .definitions
                    .extend(visible_reachable_definitions_for_name(
                        self.model,
                        name.id.as_str(),
                        self.scope_node,
                    ));
            }
            // Sub-ASTs keep file-relative node keys, so inference's classification
            // applies at any nesting depth.
            ast::Expr::StringLiteral(string) => {
                if self.annotation_keys.contains(&expr.into()) {
                    self.visit_string(string);
                }
            }
            ast::Expr::Attribute(_) => {
                // Retain the dotted path for multipart matching, the walk must continue
                // so the root name records its definitions.
                if let Some(dotted) = UnqualifiedName::from_expr(expr) {
                    self.uses
                        .dotted_names
                        .insert(dotted.to_string().into_boxed_str());
                }
                source_order::walk_expr(self, expr);
            }
            _ => source_order::walk_expr(self, expr),
        }
    }
}

/// Returns `true` for concrete import aliases that can produce unused-import hints.
///
/// Star imports have no precise target.
fn should_report_import(kind: &DefinitionKind<'_>) -> bool {
    matches!(
        kind,
        DefinitionKind::Import(_) | DefinitionKind::ImportFrom(_)
    )
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

#[cfg(test)]
mod tests {
    use super::unused_imports;
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;
    use ruff_python_trivia::textwrap::dedent;

    struct UnusedImportTest<'a> {
        path: &'a str,
    }

    impl<'a> UnusedImportTest<'a> {
        fn new() -> Self {
            Self {
                path: "/src/main.py",
            }
        }

        fn with_path(mut self, path: &'a str) -> Self {
            self.path = path;
            self
        }

        fn entries(&self, source: &str) -> anyhow::Result<Vec<(String, String)>> {
            let source = dedent(source);
            let db = TestDbBuilder::new().with_file(self.path, &source).build()?;
            let file = system_path_to_file(&db, self.path)?;
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

        fn names(&self, source: &str) -> anyhow::Result<Vec<String>> {
            let mut names = self
                .entries(source)?
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>();
            names.sort();
            Ok(names)
        }
    }

    #[test]
    fn reports_basic_unused_imports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new().entries(
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
        let names = UnusedImportTest::new().names(
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
        let names = UnusedImportTest::new().names(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new().entries(
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
    fn reports_import_shadowed_before_use() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import os

            os = "not os"
            print(os)
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn reports_import_shadowed_by_function_local_binding() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import os

            def f():
                os = 1
                print(os)
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn skips_multipart_import_used_from_function_defined_before_import() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            def f():
                return os.path.join("a", "b")

            import os.path

            f()
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_multipart_import_used_from_nested_nested_scope() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            def outer():
                def inner():
                    return os.path.join("a", "b")

                return inner

            import os.path
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn late_multipart_sibling_imports_are_credited_together() -> anyhow::Result<()> {
        // Accepted false negative: a dotted use recorded before the import can't
        // defer its path, so a later import list is credited without submodule
        // matching.
        let names = UnusedImportTest::new().names(
            r#"
            def f():
                return json.decoder.JSONDecoder

            import json.decoder, json.encoder

            f()
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_multipart_import_used_before_function_local_binding() -> anyhow::Result<()> {
        // `os` is local for the whole body of `f`, so the dotted use is unbound at
        // runtime and the module import is never used.
        let names = UnusedImportTest::new().names(
            r#"
            import os.path

            def f():
                print(os.path)
                os = 1
            "#,
        )?;

        assert_eq!(names, vec!["os.path"]);
        Ok(())
    }

    #[test]
    fn reports_unused_multipart_sibling_import_used_from_function_scope() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            import json.decoder, json.encoder

            def f():
                return json.decoder.JSONDecoder
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("json.encoder".to_string(), "json.encoder".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_partially_used_multipart_import_lists() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
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
        let names = UnusedImportTest::new().names(
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
        let names = UnusedImportTest::new().names(
            r#"
            import xml.etree.ElementTree

            print(xml.etree.ElementTree.Element)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_import_used_only_before_import() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            print(os)

            import os
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn reports_multipart_import_used_only_before_import() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            print(os.path)

            import os.path
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_shadowed_before_use() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            import os.path

            os = None
            print(os.path)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_module_scope_multipart_import_used_from_function_scope() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
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
    fn skips_module_scope_multipart_import_used_after_function_local_shadowing()
    -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import os.path

            def f():
                os = None

            print(os.path)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_multipart_import_shadowed_by_function_parameter() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            import os.path

            def f(os):
                print(os.path)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_shadowed_by_lambda_parameter() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            import os.path

            f = lambda os: os.path
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_multipart_import_used_after_comprehension_target_shadowing() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import os.path

            _ = [os for os in range(3)]
            _ = {os for os in range(3)}
            _ = {os: None for os in range(3)}
            _ = (os for os in range(3))
            print(os.path)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_multipart_import_used_only_from_comprehension_shadowing() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            import os.path

            _ = [os.path for os in values]
            _ = {os.path for os in values}
            _ = {os.path: None for os in values}
            _ = (os.path for os in values)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("os.path".to_string(), "os.path".to_string())]
        );
        Ok(())
    }

    #[test]
    fn skips_function_scope_multipart_import_used_from_nested_scope() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new().entries(
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
    fn reports_multipart_import_when_only_same_leaf_different_path_is_used() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
            r#"
            import pkg.mod

            print(pkg.other.mod)
            "#,
        )?;

        assert_eq!(
            entries,
            vec![("pkg.mod".to_string(), "pkg.mod".to_string())]
        );
        Ok(())
    }

    #[test]
    fn reports_multipart_import_when_only_assigned() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
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
        let names = UnusedImportTest::new().names(
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
        let entries = UnusedImportTest::new().entries(
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
        let entries = UnusedImportTest::new()
            .with_path("/src/pkg/module.py")
            .entries(
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
        let names = UnusedImportTest::new().names(
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
    fn reports_function_scope_reexport_shaped_unused_imports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            def f():
                import sys as sys
                from pathlib import Path as Path
            "#,
        )?;

        assert_eq!(names, vec!["Path", "sys"]);
        Ok(())
    }

    #[test]
    fn reports_class_scope_unused_imports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            class C:
                import os
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn skips_class_scope_import_used_via_instance_attribute() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            class C:
                import os

                def m(self):
                    return self.os.getcwd()
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_class_scope_import_used_via_class_attribute() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            class C:
                import os

            C.os.getcwd()
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn class_scope_import_attribute_suppression_is_name_based() -> anyhow::Result<()> {
        // Accepted false negative: any attribute access with a matching name
        // suppresses the hint, even on an unrelated object.
        let names = UnusedImportTest::new().names(
            r#"
            class C:
                import os

            def f(x):
                return x.os
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_class_scope_unused_from_imports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            class C:
                from os import path
            "#,
        )?;

        assert_eq!(names, vec!["path"]);
        Ok(())
    }

    #[test]
    fn reports_class_scope_multipart_import_used_only_from_method_scope() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().entries(
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
        let names = UnusedImportTest::new().names(
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
        let names = UnusedImportTest::new().names(
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
    fn dunder_all_suppresses_renamed_import_exports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            __all__ = ["exported_by_all"]
            from fractions import Fraction as exported_by_all
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_star_imports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from os import *
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn dunder_all_only_applies_to_module_scope_imports() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
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
        let names = UnusedImportTest::new().names(
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
        let names = UnusedImportTest::new().names(
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
        let names = UnusedImportTest::new().names(
            r#"
            from __future__ import annotations
            import os
            "#,
        )?;

        assert_eq!(names, vec!["os"]);
        Ok(())
    }

    #[test]
    fn reports_aliased_plain_dunder_future_import() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import __future__ as future
            "#,
        )?;

        assert_eq!(names, vec!["future"]);
        Ok(())
    }

    #[test]
    fn reports_stub_file_unused_imports() -> anyhow::Result<()> {
        let entries = UnusedImportTest::new().with_path("/src/main.pyi").entries(
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
        let names = UnusedImportTest::new().names(
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

    #[test]
    fn skips_imports_used_only_in_stringified_annotations() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import List

            x: """List['Path']""" = []
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_imports_used_in_function_scope_stringified_annotations() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path

            def f():
                value: "Path"
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_multipart_imports_used_only_in_stringified_annotations() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import xml.etree.ElementTree

            def f(tree: "xml.etree.ElementTree.Element"): ...
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_multipart_imports_sharing_only_the_root_with_stringified_annotations()
    -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import xml.etree.ElementTree

            def f(x: "xml.dom.minidom.Document"): ...
            "#,
        )?;

        assert_eq!(names, vec!["xml.etree.ElementTree"]);
        Ok(())
    }

    #[test]
    fn skips_imports_used_in_stringified_cast_types() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from collections import OrderedDict
            from typing import cast

            def f(x):
                return cast("OrderedDict", x)
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_imports_used_in_stringified_assert_type_types() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from collections import OrderedDict
            from typing_extensions import assert_type

            def f(x):
                assert_type(x, "OrderedDict")
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_imports_used_in_stringified_type_alias_values() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from os import PathLike
            from typing import TypeAlias

            P: TypeAlias = "PathLike[str]"
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_imports_passed_as_strings_to_unknown_callables() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from collections import OrderedDict

            def cast(typ, val):
                return val

            def f(x):
                return cast("OrderedDict", x)
            "#,
        )?;

        assert_eq!(names, vec!["OrderedDict"]);
        Ok(())
    }

    #[test]
    fn reports_imports_used_only_as_literal_string_values() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Literal

            x: """Literal["Path"]""" = "Path"
            "#,
        )?;

        assert_eq!(names, vec!["Path"]);
        Ok(())
    }

    #[test]
    fn reports_imports_used_only_as_unquoted_literal_string_values() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Literal

            x: Literal["Path"] = "Path"
            "#,
        )?;

        assert_eq!(names, vec!["Path"]);
        Ok(())
    }

    #[test]
    fn reports_imports_used_only_as_aliased_literal_string_values() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Literal as L

            x: L["Path"] = "Path"
            "#,
        )?;

        assert_eq!(names, vec!["Path"]);
        Ok(())
    }

    #[test]
    fn reports_imports_used_only_as_unquoted_annotated_string_metadata() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Annotated

            value: Annotated[int, "Path"] = 1
            "#,
        )?;

        assert_eq!(names, vec!["Path"]);
        Ok(())
    }

    #[test]
    fn skips_import_used_as_unquoted_annotated_string_first_argument() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Annotated

            value: Annotated["Path", "metadata"]
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_imports_used_only_as_annotated_string_metadata() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Annotated

            value: "Annotated[int, 'Path']"
            "#,
        )?;

        assert_eq!(names, vec!["Path"]);
        Ok(())
    }

    #[test]
    fn skips_import_used_as_annotated_string_first_argument() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from pathlib import Path
            from typing import Annotated

            value: "Annotated['Path', 'metadata']"
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_imports_used_in_type_aliases() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from typing import Literal
            import typing

            type Style = Literal["italic", "bold", "underline"]
            type Other = typing.Literal["italic", "bold", "underline"]
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_import_used_in_lazy_type_alias_expression() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            import re
            from typing import Annotated

            type X = Annotated[int, lambda: re.compile("x")]
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_class_scope_import_used_in_type_alias() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            class C:
                from typing import Literal
                type Style = Literal["italic"]
            "#,
        )?;

        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_import_shadowed_in_class_type_alias() -> anyhow::Result<()> {
        let names = UnusedImportTest::new().names(
            r#"
            from typing import Literal

            class C:
                Literal = str
                type Style = Literal["italic", "bold", "underline"]
            "#,
        )?;

        assert_eq!(names, vec!["Literal"]);
        Ok(())
    }
}
