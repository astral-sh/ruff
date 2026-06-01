use get_size2::GetSize;
use ruff_db::files::File;
use ruff_db::parsed::{parsed_module, parsed_string_annotation};
use ruff_db::source::{SourceText, source_text};
use ruff_python_ast::ModExpression;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{self as ast, AnyNodeRef, helpers::is_dunder, name::Name};
use ruff_python_parser::Parsed;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState};
use ty_python_core::scope::{NodeWithScopeKind, ScopeKind};
use ty_python_core::semantic_index;

use super::visible_reachable_definitions_for_name;
use crate::dunder_all::dunder_all_names;
use crate::{Db, SemanticModel};

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
pub fn unused_imports(db: &dyn Db, file: File) -> Vec<UnusedImport> {
    let parsed = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);
    let mut string_annotation_definitions = None;
    let mut explicit_exports = None;
    let mut unused = Vec::new();

    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);
        let scope = index.scope(file_scope_id);
        let is_module_scope = matches!(scope.kind(), ScopeKind::Module);

        if matches!(scope.kind(), ScopeKind::TypeParams | ScopeKind::TypeAlias) {
            continue;
        }

        let use_def_map = index.use_def_map(file_scope_id);

        for (_, state, is_used) in use_def_map.all_definitions_with_usage() {
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

            let Some((range, display_name)) = import_target(kind, &parsed) else {
                continue;
            };

            let multipart_import_name = unaliased_multipart_import_name(kind, &parsed);
            let multipart_import_is_used =
                multipart_import_name.is_some_and(|name| match scope.node() {
                    NodeWithScopeKind::Module => {
                        multipart_import_is_used_in_body(parsed.suite(), name, range)
                    }
                    NodeWithScopeKind::Class(class) => multipart_import_is_used_in_class_body(
                        &class.node(&parsed).body,
                        name,
                        range,
                    ),
                    NodeWithScopeKind::Function(function) => {
                        multipart_import_is_used_in_body(&function.node(&parsed).body, name, range)
                    }
                    NodeWithScopeKind::Lambda(lambda) => {
                        let mut visitor = MultipartImportUseVisitor::new(name, range);
                        visitor.visit_expr(&lambda.node(&parsed).body);
                        visitor.used
                    }
                    _ => false,
                });
            if multipart_import_is_used || (multipart_import_name.is_none() && is_used) {
                continue;
            }

            if multipart_import_name.is_none()
                && string_annotation_definitions
                    .get_or_insert_with(|| string_annotation_used_definitions(db, file))
                    .contains(&definition)
            {
                continue;
            }

            if is_intentionally_unused_name(&display_name) {
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
    unused
}

fn string_annotation_used_definitions(db: &dyn Db, file: File) -> FxHashSet<Definition<'_>> {
    let parsed = parsed_module(db, file).load(db);
    let source = source_text(db, file);
    let model = SemanticModel::new(db, file);
    let mut definitions = FxHashSet::default();
    let mut visitor = StringAnnotationDefinitionVisitor {
        model: &model,
        source: &source,
        definitions: &mut definitions,
        in_annotation: false,
        scope_node: None,
    };

    for stmt in parsed.suite() {
        visitor.visit_stmt(stmt);
    }

    definitions
}

fn parse_string_annotation(
    source: &SourceText,
    string: &ast::ExprStringLiteral,
) -> Option<Parsed<ModExpression>> {
    let string_literal = string.as_single_part_string()?;

    if string_literal.flags.prefix().is_raw()
        || &source[string_literal.content_range()] != string_literal.as_str()
    {
        return None;
    }

    parsed_string_annotation(source.as_str(), string_literal).ok()
}

struct StringAnnotationDefinitionVisitor<'model, 'db> {
    model: &'model SemanticModel<'db>,
    source: &'model SourceText,
    definitions: &'model mut FxHashSet<Definition<'db>>,
    in_annotation: bool,
    scope_node: Option<AnyNodeRef<'model>>,
}

impl<'model> StringAnnotationDefinitionVisitor<'model, '_> {
    fn enter_annotation(&mut self, expr: &'model ast::Expr, visit: impl FnOnce(&mut Self)) {
        let previous = std::mem::replace(&mut self.in_annotation, true);
        let previous_scope_node = self.scope_node.replace(expr.into());
        visit(self);
        self.scope_node = previous_scope_node;
        self.in_annotation = previous;
    }

    fn visit_string_annotation(&mut self, string: &'model ast::ExprStringLiteral) {
        let Some(parsed) = parse_string_annotation(self.source, string) else {
            return;
        };

        let Some(scope_node) = self.scope_node else {
            return;
        };
        let mut visitor = ParsedStringAnnotationDefinitionVisitor {
            model: self.model,
            source: self.source,
            definitions: self.definitions,
            scope_node,
            parse_nested_string_annotations: true,
        };
        visitor.visit_expr(parsed.expr());
    }
}

impl<'model> SourceOrderVisitor<'model> for StringAnnotationDefinitionVisitor<'model, '_> {
    fn visit_annotation(&mut self, expr: &'model ast::Expr) {
        self.enter_annotation(expr, |visitor| {
            source_order::walk_annotation(visitor, expr);
        });
    }

    fn visit_expr(&mut self, expr: &'model ast::Expr) {
        match expr {
            ast::Expr::StringLiteral(string) if self.in_annotation => {
                self.visit_string_annotation(string);
            }
            _ => source_order::walk_expr(self, expr),
        }
    }
}

struct ParsedStringAnnotationDefinitionVisitor<'model, 'db> {
    model: &'model SemanticModel<'db>,
    source: &'model SourceText,
    definitions: &'model mut FxHashSet<Definition<'db>>,
    scope_node: AnyNodeRef<'model>,
    parse_nested_string_annotations: bool,
}

impl ParsedStringAnnotationDefinitionVisitor<'_, '_> {
    fn collect_name(&mut self, name: &ast::ExprName) {
        self.definitions
            .extend(visible_reachable_definitions_for_name(
                self.model,
                name.id.as_str(),
                self.scope_node,
            ));
    }

    fn visit_string_annotation(&mut self, string: &ast::ExprStringLiteral) {
        if !self.parse_nested_string_annotations {
            return;
        }

        let Some(parsed) = parse_string_annotation(self.source, string) else {
            return;
        };

        self.visit_expr(parsed.expr());
    }

    fn visit_subscript(&mut self, subscript: &ast::ExprSubscript) {
        self.visit_expr(&subscript.value);
        let subscript_name = annotation_subscript_name(&subscript.value);

        if subscript_name == Some("Literal") {
            // String arguments to `Literal` are values, not forward annotations.
            self.with_parse_nested_string_annotations(false, |visitor| {
                visitor.visit_expr(&subscript.slice);
            });
            return;
        }

        if subscript_name == Some("Annotated") {
            if let ast::Expr::Tuple(tuple) = subscript.slice.as_ref()
                && let Some((first, rest)) = tuple.elts.split_first()
            {
                self.visit_expr(first);
                self.with_parse_nested_string_annotations(false, |visitor| {
                    for elt in rest {
                        visitor.visit_expr(elt);
                    }
                });
                return;
            }
        }

        self.visit_expr(&subscript.slice);
    }

    fn with_parse_nested_string_annotations(&mut self, parse: bool, visit: impl FnOnce(&mut Self)) {
        let previous = std::mem::replace(&mut self.parse_nested_string_annotations, parse);
        visit(self);
        self.parse_nested_string_annotations = previous;
    }
}

impl<'ast> SourceOrderVisitor<'ast> for ParsedStringAnnotationDefinitionVisitor<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        match expr {
            ast::Expr::Name(name) => self.collect_name(name),
            ast::Expr::StringLiteral(string) => self.visit_string_annotation(string),
            ast::Expr::Subscript(subscript) => self.visit_subscript(subscript),
            _ => source_order::walk_expr(self, expr),
        }
    }
}

fn annotation_subscript_name(expr: &ast::Expr) -> Option<&str> {
    match expr {
        ast::Expr::Name(name) => Some(name.id.as_str()),
        ast::Expr::Attribute(attribute) => Some(attribute.attr.id.as_str()),
        _ => None,
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

fn expr_uses_dotted_import(expr: &ast::Expr, imported_name: &str) -> bool {
    let mut segments = imported_name.split('.');
    expr_matches_dotted_import_prefix(expr, &mut segments) && segments.next().is_none()
}

fn expr_matches_dotted_import_prefix(
    expr: &ast::Expr,
    segments: &mut std::str::Split<'_, char>,
) -> bool {
    match expr {
        ast::Expr::Name(name) => segments.next().is_some_and(|segment| name.id == segment),
        ast::Expr::Attribute(attribute) => {
            if !expr_matches_dotted_import_prefix(&attribute.value, segments) {
                return false;
            }

            // Once all imported segments are consumed, any further attribute access
            // is a use of the import.
            segments
                .next()
                .is_none_or(|segment| attribute.attr.id == segment)
        }
        _ => false,
    }
}

struct MultipartImportUseVisitor<'a> {
    imported_name: &'a str,
    imported_root: &'a str,
    import_range: TextRange,
    used: bool,
    shadowed_scopes: Vec<bool>,
}

impl<'import> MultipartImportUseVisitor<'import> {
    fn new(imported_name: &'import str, import_range: TextRange) -> Self {
        Self {
            imported_name,
            imported_root: imported_name.split('.').next().unwrap_or(imported_name),
            import_range,
            used: false,
            shadowed_scopes: vec![false],
        }
    }

    fn current_scope_is_shadowed(&self) -> bool {
        self.shadowed_scopes.last().copied().unwrap_or_default()
    }

    fn shadow_current_scope(&mut self) {
        if let Some(shadowed) = self.shadowed_scopes.last_mut() {
            *shadowed = true;
        }
    }

    fn with_scope(&mut self, initially_shadowed: bool, visit: impl FnOnce(&mut Self)) {
        self.shadowed_scopes.push(initially_shadowed);
        visit(self);
        self.shadowed_scopes.pop();
    }

    fn visit_comprehension_scope<'ast>(
        &mut self,
        generators: &'ast [ast::Comprehension],
        visit_body: impl FnOnce(&mut Self),
    ) where
        Self: SourceOrderVisitor<'ast>,
    {
        let Some((first, rest)) = generators.split_first() else {
            self.with_scope(self.current_scope_is_shadowed(), visit_body);
            return;
        };

        // The first iterator is evaluated in the surrounding scope. The targets,
        // filters, later iterators, and body live in the comprehension scope.
        self.visit_expr(&first.iter);
        self.with_scope(self.current_scope_is_shadowed(), |visitor| {
            visitor.visit_expr(&first.target);
            for if_expr in &first.ifs {
                visitor.visit_expr(if_expr);
            }

            for generator in rest {
                visitor.visit_expr(&generator.iter);
                visitor.visit_expr(&generator.target);
                for if_expr in &generator.ifs {
                    visitor.visit_expr(if_expr);
                }
            }

            visit_body(visitor);
        });
    }
}

impl<'a> SourceOrderVisitor<'a> for MultipartImportUseVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        if self.used {
            return;
        }

        match stmt {
            ast::Stmt::FunctionDef(function) => {
                for decorator in &function.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = function.type_params.as_deref() {
                    self.visit_type_params(type_params);
                }
                self.visit_parameters(&function.parameters);
                if let Some(returns) = function.returns.as_deref() {
                    self.visit_annotation(returns);
                }

                let shadows_outer_scope = function.range.end() > self.import_range.end()
                    && function.name.as_str() == self.imported_root;
                if shadows_outer_scope {
                    self.shadow_current_scope();
                }

                let body_is_shadowed = self.current_scope_is_shadowed()
                    || parameters_bind_name(&function.parameters, self.imported_root);
                self.with_scope(body_is_shadowed, |visitor| {
                    for stmt in &function.body {
                        visitor.visit_stmt(stmt);
                    }
                });
            }
            ast::Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = class.type_params.as_deref() {
                    self.visit_type_params(type_params);
                }
                if let Some(arguments) = class.arguments.as_deref() {
                    self.visit_arguments(arguments);
                }

                let shadows_outer_scope = class.range.end() > self.import_range.end()
                    && class.name.as_str() == self.imported_root;
                if shadows_outer_scope {
                    self.shadow_current_scope();
                }

                self.with_scope(self.current_scope_is_shadowed(), |visitor| {
                    for stmt in &class.body {
                        visitor.visit_stmt(stmt);
                    }
                });
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if self.used {
            return;
        }

        if let ast::Expr::Lambda(lambda) = expr {
            if let Some(parameters) = lambda.parameters.as_deref() {
                self.visit_parameters(parameters);
            }

            let body_is_shadowed = self.current_scope_is_shadowed()
                || lambda
                    .parameters
                    .as_deref()
                    .is_some_and(|parameters| parameters_bind_name(parameters, self.imported_root));
            self.with_scope(body_is_shadowed, |visitor| {
                visitor.visit_expr(&lambda.body);
            });
            return;
        }

        if expr.range().end() <= self.import_range.end() {
            return;
        }

        match expr {
            ast::Expr::ListComp(list_comp) => {
                self.visit_comprehension_scope(&list_comp.generators, |visitor| {
                    visitor.visit_expr(&list_comp.elt);
                });
                return;
            }
            ast::Expr::SetComp(set_comp) => {
                self.visit_comprehension_scope(&set_comp.generators, |visitor| {
                    visitor.visit_expr(&set_comp.elt);
                });
                return;
            }
            ast::Expr::DictComp(dict_comp) => {
                self.visit_comprehension_scope(&dict_comp.generators, |visitor| {
                    if let Some(key) = dict_comp.key.as_deref() {
                        visitor.visit_expr(key);
                    }
                    visitor.visit_expr(&dict_comp.value);
                });
                return;
            }
            ast::Expr::Generator(generator) => {
                self.visit_comprehension_scope(&generator.generators, |visitor| {
                    visitor.visit_expr(&generator.elt);
                });
                return;
            }
            _ => {}
        }

        if let ast::Expr::Name(name) = expr
            && matches!(name.ctx, ast::ExprContext::Store)
            && name.id == self.imported_root
        {
            self.shadow_current_scope();
            return;
        }

        if let ast::Expr::Attribute(attribute) = expr
            && matches!(attribute.ctx, ast::ExprContext::Load)
            && !self.current_scope_is_shadowed()
            && expr_uses_dotted_import(expr, self.imported_name)
        {
            self.used = true;
            return;
        }

        source_order::walk_expr(self, expr);
    }
}

fn multipart_import_is_used_in_body(
    body: &[ast::Stmt],
    imported_name: &str,
    import_range: TextRange,
) -> bool {
    let mut visitor = MultipartImportUseVisitor::new(imported_name, import_range);

    for stmt in body {
        visitor.visit_stmt(stmt);

        if visitor.used {
            break;
        }
    }

    visitor.used
}

fn multipart_import_is_used_in_class_body(
    body: &[ast::Stmt],
    imported_name: &str,
    import_range: TextRange,
) -> bool {
    let mut visitor = MultipartImportUseVisitor::new(imported_name, import_range);

    for stmt in body {
        if matches!(stmt, ast::Stmt::ClassDef(_) | ast::Stmt::FunctionDef(_)) {
            continue;
        }

        visitor.visit_stmt(stmt);

        if visitor.used {
            break;
        }
    }

    visitor.used
}

fn parameters_bind_name(parameters: &ast::Parameters, name: &str) -> bool {
    parameters
        .iter()
        .any(|parameter| parameter.name().as_str() == name)
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
