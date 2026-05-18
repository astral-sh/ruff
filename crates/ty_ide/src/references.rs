//! This module implements the core functionality of the "references",
//! "document highlight" and "rename" language server features. It locates
//! all references to a named symbol. Unlike a simple text search for the
//! symbol's name, this is a "semantic search" where the text and the semantic
//! meaning must match.
//!
//! Some symbols (such as parameters and local variables) are visible only
//! within their scope. All other symbols, such as those defined at the global
//! scope or within classes, are visible outside the module. Finding
//! all references to these externally-visible symbols therefore requires
//! an expensive search of all source files in the workspace.

use crate::goto::{Definitions, GotoTarget};
use crate::{Db, ReferenceKind, ReferenceTarget};
use ruff_db::files::File;
use ruff_python_ast::find_node::{CoveringNode, covering_node};
use ruff_python_ast::token::Tokens;
use ruff_python_ast::{
    self as ast, AnyNodeRef,
    visitor::source_order::{SourceOrderVisitor, TraversalSignal},
};
use ruff_text_size::Ranged;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::scope::ScopeKind;
use ty_python_semantic::{ImportAliasResolution, ResolvedDefinition, SemanticModel};

/// Mode for references search behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferencesMode {
    /// Find all references including the declaration
    References,
    /// Find all references but skip the declaration
    ReferencesSkipDeclaration,
    /// Find references for rename operations, limited to current file only
    Rename,
    /// Find references for multi-file rename operations (searches across all files)
    RenameMultiFile,
    /// Find references for document highlights (limits search to current file)
    DocumentHighlights,
}

impl ReferencesMode {
    pub(super) fn to_import_alias_resolution(self) -> ImportAliasResolution {
        match self {
            // Resolve import aliases for find references:
            // ```py
            // from warnings import deprecated as my_deprecated
            //
            // @my_deprecated
            // def foo
            // ```
            //
            // When finding references on `my_deprecated`, we want to find all usages of `deprecated` across the entire
            // project.
            Self::References | Self::ReferencesSkipDeclaration => {
                ImportAliasResolution::ResolveAliases
            }
            // For rename, don't resolve import aliases.
            //
            // ```py
            // from warnings import deprecated as my_deprecated
            //
            // @my_deprecated
            // def foo
            // ```
            // When renaming `my_deprecated`, only rename the alias, but not the original definition in `warnings`.
            Self::Rename | Self::RenameMultiFile | Self::DocumentHighlights => {
                ImportAliasResolution::PreserveAliases
            }
        }
    }
}

/// Find all references to a symbol at the given position.
/// Search for references across all files in the project.
pub(crate) fn references(
    db: &dyn Db,
    file: File,
    goto_target: &GotoTarget,
    mode: ReferencesMode,
) -> Option<Vec<ReferenceTarget>> {
    let model = SemanticModel::new(db, file);
    let target_definitions = goto_target.definitions(&model, mode.to_import_alias_resolution())?;
    let is_externally_visible_symbol =
        has_any_external_visible_definitions(db, &target_definitions);
    let target_definitions = target_definitions.goto_declaration(&model, goto_target)?;

    // Extract the target text from the goto target for fast comparison
    let target_text = goto_target.to_string()?;

    // Find all of the references to the symbol within this file
    let mut references = references_for_file(db, file, &target_definitions, &target_text, mode);

    // Check if we should search across files based on the mode
    let search_across_files = matches!(
        mode,
        ReferencesMode::References
            | ReferencesMode::ReferencesSkipDeclaration
            | ReferencesMode::RenameMultiFile
    );

    // Parameters are local by scope, but they can have cross-file references via keyword
    // argument labels (e.g. `f(param=...)`). Handle this case with a narrow scan that only
    // considers keyword arguments.
    let is_parameter = parameter_owner_is_externally_visible(db, &target_definitions);

    if search_across_files && (is_parameter || is_externally_visible_symbol) {
        let result = std::sync::Mutex::new(Vec::new());
        let files = db.project().files(db);

        {
            let db = Db::dyn_clone(db);
            let target_definitions = &target_definitions;
            let files = &files;
            let result = &result;
            let needle = target_text.as_ref();

            rayon::scope(move |s| {
                for other_file in files {
                    // Skip the current file as we already processed it
                    if other_file == file {
                        continue;
                    }

                    let db = Db::dyn_clone(&*db);

                    s.spawn(move |_| {
                        let db = &*db;

                        // First do a simple text search to see if there is a potential match in the file
                        let source = ruff_db::source::source_text(db, other_file);
                        if !contains_identifier(&source, needle) {
                            return;
                        }

                        // If the target text is found, do the more expensive semantic analysis
                        let references = if is_externally_visible_symbol {
                            references_for_file(db, other_file, target_definitions, needle, mode)
                        } else {
                            references_for_keyword_arguments_in_file(
                                db,
                                other_file,
                                target_definitions,
                                needle,
                                mode,
                            )
                        };

                        result.lock().unwrap().extend(references);
                    });
                }
            });
        }
        references.extend(result.into_inner().unwrap());
    }

    if references.is_empty() {
        None
    } else {
        Some(references)
    }
}

fn references_for_keyword_arguments_in_file(
    db: &dyn Db,
    file: File,
    target_definitions: &Definitions<'_>,
    target_text: &str,
    mode: ReferencesMode,
) -> Vec<ReferenceTarget> {
    // This path is used for cross-file parameter keyword-label references.
    // DocumentHighlights is same-file-only and should never route through here.
    debug_assert!(
        !matches!(mode, ReferencesMode::DocumentHighlights),
        "keyword-label cross-file scan should not run in DocumentHighlights mode"
    );

    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);
    let mut references = Vec::new();

    let mut finder = KeywordArgumentReferencesFinder(LocalReferencesFinder {
        model: &model,
        tokens: module.tokens(),
        target_definitions,
        references: &mut references,
        mode,
        target_text,
        ancestors: Vec::new(),
    });

    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);

    references
}

/// Cheap text prefilter for identifier references before AST/semantic validation.
///
/// Heuristically matches an ASCII approximation of `\b{name}\b`.
fn contains_identifier(source: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let bytes = source.as_bytes();
    let needle = name.as_bytes();

    memchr::memmem::find_iter(bytes, needle).any(move |pos| {
        let after = pos + needle.len();

        // Skip this entry if it is within an identifier. E.g. skip
        // this entry when searching for `x` and this is a match
        // within `exclude = 10`
        let boundary_before = pos == 0 || !is_ascii_identifier_continue(bytes[pos - 1]);
        let boundary_after = bytes
            .get(after)
            .is_none_or(|byte| !is_ascii_identifier_continue(*byte));

        boundary_before && boundary_after
    })
}

fn is_ascii_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Find all references to a local symbol within the current file.
/// The behavior depends on the provided mode.
fn references_for_file(
    db: &dyn Db,
    file: File,
    target_definitions: &Definitions<'_>,
    target_text: &str,
    mode: ReferencesMode,
) -> Vec<ReferenceTarget> {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);
    let mut references = Vec::new();

    let mut finder = LocalReferencesFinder {
        model: &model,
        target_definitions,
        references: &mut references,
        mode,
        tokens: module.tokens(),
        target_text,
        ancestors: Vec::new(),
    };

    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);

    references
}

/// Determines whether the resolved definitions can have references outside their file.
fn has_any_external_visible_definitions(db: &dyn Db, definitions: &Definitions<'_>) -> bool {
    definitions.iter().any(|definition| match definition {
        ResolvedDefinition::Definition(definition) => match definition.scope(db).scope(db).kind() {
            ScopeKind::Module | ScopeKind::Class => true,
            ScopeKind::TypeParams
            | ScopeKind::Function
            | ScopeKind::Lambda
            | ScopeKind::Comprehension
            | ScopeKind::TypeAlias => false,
        },
        ResolvedDefinition::Module(_) | ResolvedDefinition::FileWithRange(_) => true,
    })
}

/// Determine whether a parameter's owning callable is externally visible.
///
/// Parameters are local by scope, but their keyword-argument labels can appear across files
/// when the owning callable is visible outside of the current module.
fn parameter_owner_is_externally_visible(
    db: &dyn Db,
    target_definitions: &Definitions<'_>,
) -> bool {
    target_definitions
        .iter()
        .any(|target| parameter_owner_is_externally_visible_for_target(db, target))
}

fn parameter_owner_is_externally_visible_for_target(
    db: &dyn Db,
    definition: &ResolvedDefinition,
) -> bool {
    let target = definition.focus_range(db);
    let file = target.file();
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);

    let covering = covering_node(module.syntax().into(), target.range());
    let Ok(parameter_covering) =
        covering.find_last(|node| matches!(node, AnyNodeRef::Parameter(_)))
    else {
        return false;
    };

    let mut owner: Option<AnyNodeRef<'_>> = None;
    let mut seen_owner = false;
    let mut class_ancestor_found = false;

    // Heuristic: treat parameters as externally visible only when they belong to a top-level
    // function or a method on a top-level class. Nested functions/classes are excluded to avoid
    // broad, low-signal workspace scans.
    for ancestor in parameter_covering.ancestors() {
        if !seen_owner {
            if matches!(
                ancestor,
                AnyNodeRef::StmtFunctionDef(_) | AnyNodeRef::ExprLambda(_)
            ) {
                owner = Some(ancestor);
                seen_owner = true;
            }
            continue;
        }

        match ancestor {
            AnyNodeRef::StmtFunctionDef(_) | AnyNodeRef::ExprLambda(_) => {
                // Nested functions or lambdas are not externally visible.
                return false;
            }
            AnyNodeRef::StmtClassDef(_) => {
                if class_ancestor_found {
                    // Nested classes are treated as not externally visible for now.
                    return false;
                }
                class_ancestor_found = true;
            }
            _ => {}
        }
    }

    matches!(owner, Some(AnyNodeRef::StmtFunctionDef(_)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OccurrenceKind {
    /// An identifier that references a symbol.
    Reference,
    /// An identifier that declares a new symbol.
    Declaration,
    /// An identifier that binds a new value to a symbol.
    Binding,
}

impl OccurrenceKind {
    fn to_reference_kind(self) -> ReferenceKind {
        match self {
            Self::Reference => ReferenceKind::Read,
            Self::Declaration => ReferenceKind::Other,
            Self::Binding => ReferenceKind::Write,
        }
    }
}

impl From<ast::ExprContext> for OccurrenceKind {
    fn from(value: ast::ExprContext) -> Self {
        match value {
            ast::ExprContext::Load | ast::ExprContext::Invalid => Self::Reference,
            ast::ExprContext::Store | ast::ExprContext::Del => OccurrenceKind::Binding,
        }
    }
}

/// AST visitor to find all references to a specific symbol by comparing semantic definitions
struct LocalReferencesFinder<'a> {
    model: &'a SemanticModel<'a>,
    tokens: &'a Tokens,
    target_definitions: &'a Definitions<'a>,
    references: &'a mut Vec<ReferenceTarget>,
    mode: ReferencesMode,
    target_text: &'a str,
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> SourceOrderVisitor<'a> for LocalReferencesFinder<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            AnyNodeRef::ExprName(name_expr) => {
                // If the name doesn't match our target text, this isn't a match
                if name_expr.id.as_str() != self.target_text {
                    return TraversalSignal::Traverse;
                }

                let kind = OccurrenceKind::from(name_expr.ctx);
                let covering_node = CoveringNode::from_ancestors(self.ancestors.clone());
                self.check_covering_node(&covering_node, kind);
            }
            AnyNodeRef::ExprAttribute(attr_expr) => {
                let kind = OccurrenceKind::from(attr_expr.ctx);
                self.check_identifier(&attr_expr.attr, kind);
            }
            AnyNodeRef::StmtFunctionDef(func) => {
                self.check_declaration_identifier(&func.name);
            }
            AnyNodeRef::StmtClassDef(class) => {
                self.check_declaration_identifier(&class.name);
            }
            AnyNodeRef::Parameter(parameter) => {
                self.check_declaration_identifier(&parameter.name);
            }
            AnyNodeRef::Keyword(keyword) => {
                if let Some(arg) = &keyword.arg {
                    self.check_reference_identifier(arg);
                }
            }
            AnyNodeRef::StmtGlobal(global_stmt) => {
                for name in &global_stmt.names {
                    self.check_declaration_identifier(name);
                }
            }
            AnyNodeRef::StmtNonlocal(nonlocal_stmt) => {
                for name in &nonlocal_stmt.names {
                    self.check_declaration_identifier(name);
                }
            }
            AnyNodeRef::ExceptHandlerExceptHandler(handler) => {
                if let Some(name) = &handler.name {
                    self.check_binding_identifier(name);
                }
            }
            AnyNodeRef::PatternMatchAs(pattern_as) => {
                if let Some(name) = &pattern_as.name {
                    self.check_binding_identifier(name);
                }
            }
            AnyNodeRef::PatternMatchStar(pattern_star) => {
                if let Some(name) = &pattern_star.name {
                    self.check_binding_identifier(name);
                }
            }
            AnyNodeRef::PatternMatchMapping(pattern_mapping) => {
                if let Some(rest_name) = &pattern_mapping.rest {
                    self.check_binding_identifier(rest_name);
                }
            }
            AnyNodeRef::TypeParamParamSpec(param_spec) => {
                self.check_declaration_identifier(&param_spec.name);
            }
            AnyNodeRef::TypeParamTypeVarTuple(param_tuple) => {
                self.check_declaration_identifier(&param_tuple.name);
            }
            AnyNodeRef::TypeParamTypeVar(param_var) => {
                self.check_declaration_identifier(&param_var.name);
            }
            AnyNodeRef::ExprStringLiteral(string_expr) => {
                // Highlight the sub-AST of a string annotation
                if let Some((sub_ast, sub_model)) = self.model.enter_string_annotation(string_expr)
                {
                    let mut sub_finder = LocalReferencesFinder {
                        model: &sub_model,
                        target_definitions: self.target_definitions,
                        references: self.references,
                        mode: self.mode,
                        tokens: sub_ast.tokens(),
                        target_text: self.target_text,
                        ancestors: Vec::new(),
                    };
                    sub_finder.visit_expr(sub_ast.expr());
                }
            }
            AnyNodeRef::Alias(alias) => {
                // Handle import alias declarations
                if let Some(asname) = &alias.asname {
                    self.check_declaration_identifier(asname);
                }
                // Only check the original name if it matches our target text
                // This is for cases where we're renaming the imported symbol name itself
                if alias.name.id == self.target_text {
                    self.check_declaration_identifier(&alias.name);
                }
            }
            _ => {}
        }

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        debug_assert_eq!(self.ancestors.last(), Some(&node));
        self.ancestors.pop();
    }
}

/// AST visitor that searches only keyword-argument labels for semantic matches against a target.
struct KeywordArgumentReferencesFinder<'a>(LocalReferencesFinder<'a>);

impl<'a> SourceOrderVisitor<'a> for KeywordArgumentReferencesFinder<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.0.ancestors.push(node);

        if let AnyNodeRef::Keyword(keyword) = node {
            if let Some(arg) = &keyword.arg {
                self.0.check_reference_identifier(arg);
            }
        }

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        debug_assert_eq!(self.0.ancestors.last(), Some(&node));
        self.0.ancestors.pop();
    }
}

impl<'a> LocalReferencesFinder<'a> {
    /// Checks an identifier of a binding (e.g. `x = 10`)
    fn check_binding_identifier(&mut self, identifier: &ast::Identifier) {
        self.check_identifier(identifier, OccurrenceKind::Binding);
    }

    /// Checks an identifier that references a variable (a use).
    fn check_reference_identifier(&mut self, identifier: &ast::Identifier) {
        self.check_identifier(identifier, OccurrenceKind::Reference);
    }

    /// Checks an identifier that's part of a declaration, e.g. the name of the class.
    fn check_declaration_identifier(&mut self, identifier: &ast::Identifier) {
        self.check_identifier(identifier, OccurrenceKind::Declaration);
    }

    fn check_identifier(&mut self, identifier: &ast::Identifier, kind: OccurrenceKind) {
        // Quick text-based check first
        if identifier.id != self.target_text {
            return;
        }

        let mut ancestors_with_identifier = self.ancestors.clone();
        ancestors_with_identifier.push(AnyNodeRef::from(identifier));
        let covering_node = CoveringNode::from_ancestors(ancestors_with_identifier);
        self.check_covering_node(&covering_node, kind);
    }

    /// Returns the covering node's resolved definitions.
    fn definitions_for_covering_node(
        &self,
        covering_node: &CoveringNode<'_>,
    ) -> Option<Definitions<'a>> {
        // Use the start of the covering node as the offset. Any offset within
        // the node is fine here. Offsets matter only for import statements
        // where the identifier might be a multi-part module name.
        let offset = covering_node.node().start();
        let goto_target =
            GotoTarget::from_covering_node(self.model, covering_node, offset, self.tokens)?;

        let definitions = goto_target
            .definitions(self.model, self.mode.to_import_alias_resolution())?
            .goto_declaration(self.model, &goto_target)?;

        Some(definitions)
    }

    fn check_covering_node(&mut self, covering_node: &CoveringNode<'_>, kind: OccurrenceKind) {
        let Some(current_definitions) = self.definitions_for_covering_node(covering_node) else {
            return;
        };

        // Check if any of the current definitions match our target definitions
        if !self.target_definitions.intersects(&current_definitions) {
            return;
        }

        if matches!(self.mode, ReferencesMode::ReferencesSkipDeclaration) {
            let is_declaration = match kind {
                OccurrenceKind::Declaration => true,
                OccurrenceKind::Reference => false,
                OccurrenceKind::Binding => self.is_declaration(covering_node),
            };

            if is_declaration {
                return;
            }
        }

        let target = ReferenceTarget::new(
            self.model.file(),
            covering_node.node().range(),
            kind.to_reference_kind(),
        );
        self.references.push(target);
    }

    fn is_declaration(&self, covering_node: &CoveringNode<'_>) -> bool {
        let db = self.model.db();

        let Some(local_definition) = self.model.first_local_definition(covering_node) else {
            return false;
        };

        let file = local_definition.file(db);
        let module = ruff_db::parsed::parsed_module(db, file).load(db);
        let kind = local_definition.kind(db);
        let category = kind.category(file.is_stub(db), &module);

        if category.is_declaration() {
            return true;
        }

        if self.binding_has_reachable_explicit_declaration(local_definition) {
            return false;
        }

        self.binding_is_first_assignment_on_some_path(local_definition)
    }

    fn binding_has_reachable_explicit_declaration(&self, binding: Definition<'a>) -> bool {
        let db = self.model.db();
        let use_def = ty_python_core::use_def_map(db, binding.scope(db));
        use_def
            .declarations_at_binding(binding)
            .any(|declaration| declaration.declaration.definition().is_some())
    }

    fn binding_is_first_assignment_on_some_path(&self, binding: Definition<'a>) -> bool {
        let db = self.model.db();
        let use_def = ty_python_core::use_def_map(db, binding.scope(db));
        use_def
            .bindings_at_definition(binding)
            .any(|prior_binding| {
                matches!(
                    prior_binding.binding,
                    DefinitionState::Deleted | DefinitionState::Undefined
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goto::find_goto_target;
    use crate::tests::{CursorTest, cursor_test};

    fn cursor_target_is_externally_visible(test: &CursorTest) -> bool {
        let model = SemanticModel::new(&test.db, test.cursor.file);
        let goto_target =
            find_goto_target(&model, &test.cursor.parsed, test.cursor.offset).unwrap();
        let definitions = goto_target
            .definitions(
                &model,
                ReferencesMode::References.to_import_alias_resolution(),
            )
            .unwrap();

        has_any_external_visible_definitions(&test.db, &definitions)
    }

    #[test]
    fn externally_visible_definitions_can_have_cross_file_references() {
        for (case, source) in [
            ("module-global", "x<CURSOR> = 1"),
            (
                "class",
                "
class C:
    x<CURSOR> = 1
",
            ),
        ] {
            let test = cursor_test(source);
            assert!(cursor_target_is_externally_visible(&test), "{case}");
        }
    }

    #[test]
    fn non_public_scope_definitions_stay_in_file() {
        for (case, source) in [
            (
                "function",
                "
def f():
    x<CURSOR> = 1
    return x
",
            ),
            ("lambda", "f = lambda x<CURSOR>: x"),
            ("comprehension", "xs = [x for x<CURSOR> in range(3)]"),
            ("type parameters", "type Alias[T<CURSOR>] = list[T]"),
        ] {
            let test = cursor_test(source);
            assert!(!cursor_target_is_externally_visible(&test), "{case}");
        }
    }

    #[test]
    fn source_candidate_prefilters_use_identifier_boundaries() {
        for (source, name) in [("x = 1", "x"), ("obj.x", "x"), ("x()", "x")] {
            assert!(contains_identifier(source, name));
        }
    }
}
