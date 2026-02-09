//! This module implements the core functionality of the "references",
//! "document highlight" and "rename" language server features. It locates
//! all references to a named symbol. Unlike a simple text search for the
//! symbol's name, this is a "semantic search" where the text and the semantic
//! meaning must match.
//!
//! Some symbols (such as parameters and local variables) are visible only
//! within their scope. All other symbols, such as those defined at the global
//! scope or within classes, are visible outside of the module. Finding
//! all references to these externally-visible symbols therefore requires
//! an expensive search of all source files in the workspace.

use crate::goto::GotoTarget;
use crate::{Db, NavigationTarget, NavigationTargets, ReferenceKind, ReferenceTarget};
use ruff_db::files::File;
use ruff_python_ast::find_node::{CoveringNode, covering_node};
use ruff_python_ast::token::Tokens;
use ruff_python_ast::{
    self as ast, AnyNodeRef,
    visitor::source_order::{SourceOrderVisitor, TraversalSignal},
};
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::{ImportAliasResolution, SemanticModel};

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
    let target_definitions = goto_target
        .get_definition_targets(&model, mode.to_import_alias_resolution())?
        .declaration_targets(&model, goto_target)?;

    // Extract the target text from the goto target for fast comparison
    let target_text = goto_target.to_string()?;

    // Find all of the references to the symbol within this file
    let mut references = Vec::new();
    references_for_file(
        db,
        file,
        &target_definitions,
        &target_text,
        mode,
        &mut references,
    );

    // Check if we should search across files based on the mode
    let search_across_files = matches!(
        mode,
        ReferencesMode::References
            | ReferencesMode::ReferencesSkipDeclaration
            | ReferencesMode::RenameMultiFile
    );

    if search_across_files {
        // For symbols that are potentially visible outside of the current module, perform a full
        // semantic search across files.
        if is_symbol_externally_visible(goto_target) {
            // Look for references in all other files within the workspace
            for other_file in &db.project().files(db) {
                // Skip the current file as we already processed it
                if other_file == file {
                    continue;
                }

                // First do a simple text search to see if there is a potential match in the file
                let source = ruff_db::source::source_text(db, other_file);
                if !source.as_str().contains(target_text.as_ref()) {
                    continue;
                }

                // If the target text is found, do the more expensive semantic analysis
                references_for_file(
                    db,
                    other_file,
                    &target_definitions,
                    &target_text,
                    mode,
                    &mut references,
                );
            }
        }

        // Parameters are local by scope, but they can have cross-file references via keyword
        // argument labels (e.g. `f(param=...)`). Handle this case with a narrow scan that only
        // considers keyword arguments.
        if matches!(goto_target, GotoTarget::Parameter(_))
            && parameter_owner_is_externally_visible(db, &target_definitions)
        {
            references_for_parameter_keyword_arguments_across_files(
                db,
                file,
                &target_definitions,
                &target_text,
                mode,
                &mut references,
            );
        }
    }

    if references.is_empty() {
        None
    } else {
        Some(references)
    }
}

/// Search other files for keyword-argument labels that bind to the given parameter.
///
/// This is intentionally narrower than a full cross-file references search to avoid turning
/// common parameter names into a costly workspace-wide scan.
fn references_for_parameter_keyword_arguments_across_files(
    db: &dyn Db,
    file: File,
    target_definitions: &NavigationTargets,
    target_text: &str,
    mode: ReferencesMode,
    references: &mut Vec<ReferenceTarget>,
) {
    for other_file in &db.project().files(db) {
        if other_file == file {
            continue;
        }

        let source = ruff_db::source::source_text(db, other_file);
        if !source_contains_keyword_argument_candidate(source.as_str(), target_text) {
            continue;
        }

        references_for_keyword_arguments_in_file(
            db,
            other_file,
            target_definitions,
            target_text,
            mode,
            references,
        );
    }
}

fn references_for_keyword_arguments_in_file(
    db: &dyn Db,
    file: File,
    target_definitions: &NavigationTargets,
    target_text: &str,
    mode: ReferencesMode,
    references: &mut Vec<ReferenceTarget>,
) {
    // This path is used for cross-file parameter keyword-label references.
    // DocumentHighlights is same-file-only and should never route through here.
    debug_assert!(
        !matches!(mode, ReferencesMode::DocumentHighlights),
        "keyword-label cross-file scan should not run in DocumentHighlights mode"
    );

    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);

    let mut finder = KeywordArgumentReferencesFinder(LocalReferencesFinder {
        model: &model,
        tokens: module.tokens(),
        target_definitions,
        references,
        mode,
        target_text,
        ancestors: Vec::new(),
    });

    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);
}

/// Cheap text prefilter for keyword-argument labels before AST/semantic validation.
///
/// Heuristically matches an ASCII approximation of `\b{name}\b\s*=\s*(?!=)`.
/// This is intentionally permissive and may include non-call contexts (e.g. assignments),
/// but it helps skip files that cannot possibly contain a matching `name=` label.
fn source_contains_keyword_argument_candidate(source: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let bytes = source.as_bytes();
    let needle = name.as_bytes();
    let mut start = 0usize;

    while let Some(rel_pos) = source[start..].find(name) {
        let pos = start + rel_pos;

        // Word boundary check before.
        if let Some(prev) = pos.checked_sub(1).and_then(|i| bytes.get(i))
            && (prev.is_ascii_alphanumeric() || *prev == b'_')
        {
            start = pos + needle.len();
            continue;
        }

        let after = pos + needle.len();

        // Skip whitespace and check for '=' (but not '==').
        let mut i = after;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if bytes.get(i) == Some(&b'=') && bytes.get(i + 1) != Some(&b'=') {
            return true;
        }

        start = after;
    }

    false
}

/// Return true if the declaration-target sets intersect.
///
/// A symbol can resolve to multiple declaration targets (for example, overload groups or an
/// import binding plus its underlying definition). Intersection semantics avoid missing valid
/// references/renames when target ordering differs.
fn navigation_targets_intersect(
    target_definitions: &NavigationTargets,
    current_targets: &NavigationTargets,
) -> bool {
    target_definitions.iter().any(|target_definition| {
        current_targets.iter().any(|current_target| {
            current_target.file == target_definition.file
                && current_target.focus_range == target_definition.focus_range
        })
    })
}

/// Find all references to a local symbol within the current file.
/// The behavior depends on the provided mode.
fn references_for_file(
    db: &dyn Db,
    file: File,
    target_definitions: &NavigationTargets,
    target_text: &str,
    mode: ReferencesMode,
    references: &mut Vec<ReferenceTarget>,
) {
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);

    let mut finder = LocalReferencesFinder {
        model: &model,
        target_definitions,
        references,
        mode,
        tokens: module.tokens(),
        target_text,
        ancestors: Vec::new(),
    };

    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);
}

/// Determines whether a symbol is potentially visible outside of the current module.
fn is_symbol_externally_visible(goto_target: &GotoTarget<'_>) -> bool {
    match goto_target {
        GotoTarget::Parameter(_)
        | GotoTarget::ExceptVariable(_)
        | GotoTarget::TypeParamTypeVarName(_)
        | GotoTarget::TypeParamParamSpecName(_)
        | GotoTarget::TypeParamTypeVarTupleName(_) => false,

        // Assume all other goto target types are potentially visible.

        // TODO: For local variables, we should be able to return false
        // except in cases where the variable is in the global scope
        // or uses a "global" binding.
        _ => true,
    }
}

/// Determine whether a parameter's owning callable is externally visible.
///
/// Parameters are local by scope, but their keyword-argument labels can appear across files
/// when the owning callable is visible outside of the current module.
fn parameter_owner_is_externally_visible(
    db: &dyn Db,
    target_definitions: &NavigationTargets,
) -> bool {
    target_definitions
        .iter()
        .any(|target| parameter_owner_is_externally_visible_for_target(db, target))
}

fn parameter_owner_is_externally_visible_for_target(
    db: &dyn Db,
    target: &NavigationTarget,
) -> bool {
    let file = target.file();
    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);

    let covering = covering_node(module.syntax().into(), target.focus_range());
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

/// AST visitor to find all references to a specific symbol by comparing semantic definitions
struct LocalReferencesFinder<'a> {
    model: &'a SemanticModel<'a>,
    tokens: &'a Tokens,
    target_definitions: &'a NavigationTargets,
    references: &'a mut Vec<ReferenceTarget>,
    mode: ReferencesMode,
    target_text: &'a str,
    ancestors: Vec<AnyNodeRef<'a>>,
}

/// AST visitor that searches only keyword-argument labels for semantic matches against a target.
struct KeywordArgumentReferencesFinder<'a>(LocalReferencesFinder<'a>);

impl<'a> SourceOrderVisitor<'a> for LocalReferencesFinder<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            AnyNodeRef::ExprName(name_expr) => {
                // If the name doesn't match our target text, this isn't a match
                if name_expr.id.as_str() != self.target_text {
                    return TraversalSignal::Traverse;
                }

                let covering_node = CoveringNode::from_ancestors(self.ancestors.clone());
                self.check_reference_from_covering_node(&covering_node);
            }
            AnyNodeRef::ExprAttribute(attr_expr) => {
                self.check_identifier_reference(&attr_expr.attr);
            }
            AnyNodeRef::StmtFunctionDef(func) if self.should_include_declaration() => {
                self.check_identifier_reference(&func.name);
            }
            AnyNodeRef::StmtClassDef(class) if self.should_include_declaration() => {
                self.check_identifier_reference(&class.name);
            }
            AnyNodeRef::Parameter(parameter) if self.should_include_declaration() => {
                self.check_identifier_reference(&parameter.name);
            }
            AnyNodeRef::Keyword(keyword) => {
                if let Some(arg) = &keyword.arg {
                    self.check_identifier_reference(arg);
                }
            }
            AnyNodeRef::StmtGlobal(global_stmt) if self.should_include_declaration() => {
                for name in &global_stmt.names {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::StmtNonlocal(nonlocal_stmt) if self.should_include_declaration() => {
                for name in &nonlocal_stmt.names {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::ExceptHandlerExceptHandler(handler)
                if self.should_include_declaration() =>
            {
                if let Some(name) = &handler.name {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::PatternMatchAs(pattern_as) if self.should_include_declaration() => {
                if let Some(name) = &pattern_as.name {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::PatternMatchStar(pattern_star) if self.should_include_declaration() => {
                if let Some(name) = &pattern_star.name {
                    self.check_identifier_reference(name);
                }
            }
            AnyNodeRef::PatternMatchMapping(pattern_mapping)
                if self.should_include_declaration() =>
            {
                if let Some(rest_name) = &pattern_mapping.rest {
                    self.check_identifier_reference(rest_name);
                }
            }
            AnyNodeRef::TypeParamParamSpec(param_spec) if self.should_include_declaration() => {
                self.check_identifier_reference(&param_spec.name);
            }
            AnyNodeRef::TypeParamTypeVarTuple(param_tuple) if self.should_include_declaration() => {
                self.check_identifier_reference(&param_tuple.name);
            }
            AnyNodeRef::TypeParamTypeVar(param_var) if self.should_include_declaration() => {
                self.check_identifier_reference(&param_var.name);
            }
            AnyNodeRef::ExprStringLiteral(string_expr) if self.should_include_declaration() => {
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
            AnyNodeRef::Alias(alias) if self.should_include_declaration() => {
                // Handle import alias declarations
                if let Some(asname) = &alias.asname {
                    self.check_identifier_reference(asname);
                }
                // Only check the original name if it matches our target text
                // This is for cases where we're renaming the imported symbol name itself
                if alias.name.id == self.target_text {
                    self.check_identifier_reference(&alias.name);
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

impl<'a> SourceOrderVisitor<'a> for KeywordArgumentReferencesFinder<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.0.ancestors.push(node);

        if let AnyNodeRef::Keyword(keyword) = node {
            if let Some(arg) = &keyword.arg {
                self.0.check_identifier_reference(arg);
            }
        }

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        debug_assert_eq!(self.0.ancestors.last(), Some(&node));
        self.0.ancestors.pop();
    }
}

impl LocalReferencesFinder<'_> {
    /// Check if we should include declarations based on the current mode
    fn should_include_declaration(&self) -> bool {
        matches!(
            self.mode,
            ReferencesMode::References
                | ReferencesMode::DocumentHighlights
                | ReferencesMode::Rename
                | ReferencesMode::RenameMultiFile
        )
    }

    /// Helper method to check identifier references for declarations
    fn check_identifier_reference(&mut self, identifier: &ast::Identifier) {
        // Quick text-based check first
        if identifier.id != self.target_text {
            return;
        }

        let mut ancestors_with_identifier = self.ancestors.clone();
        ancestors_with_identifier.push(AnyNodeRef::from(identifier));
        let covering_node = CoveringNode::from_ancestors(ancestors_with_identifier);
        self.check_reference_from_covering_node(&covering_node);
    }

    /// Returns true if the covering node's resolved definitions intersect `target_definitions`.
    fn matches_target_definitions(&self, covering_node: &CoveringNode<'_>) -> bool {
        // Use the start of the covering node as the offset. Any offset within
        // the node is fine here. Offsets matter only for import statements
        // where the identifier might be a multi-part module name.
        let offset = covering_node.node().start();
        let Some(goto_target) =
            GotoTarget::from_covering_node(self.model, covering_node, offset, self.tokens)
        else {
            return false;
        };

        // Get the definitions for this goto target
        let Some(current_definitions) = goto_target
            .get_definition_targets(self.model, self.mode.to_import_alias_resolution())
            .and_then(|definitions| definitions.declaration_targets(self.model, &goto_target))
        else {
            return false;
        };

        // Check if any of the current definitions match our target definitions
        navigation_targets_intersect(self.target_definitions, &current_definitions)
    }

    /// Pushes a reference target when the covering node resolves to any target definition
    fn check_reference_from_covering_node(&mut self, covering_node: &CoveringNode<'_>) {
        if self.matches_target_definitions(covering_node) {
            // Determine if this is a read or write reference
            let kind = self.determine_reference_kind(covering_node);
            let target =
                ReferenceTarget::new(self.model.file(), covering_node.node().range(), kind);
            self.references.push(target);
        }
    }

    /// Determine whether a reference is a read or write operation based on its context
    fn determine_reference_kind(&self, covering_node: &CoveringNode<'_>) -> ReferenceKind {
        // Reference kind is only meaningful for DocumentHighlights mode
        if !matches!(self.mode, ReferencesMode::DocumentHighlights) {
            return ReferenceKind::Other;
        }

        // Walk up the ancestors to find the context
        for ancestor in self.ancestors.iter().rev() {
            match ancestor {
                // Assignment targets are writes
                AnyNodeRef::StmtAssign(assign) => {
                    // Check if our node is in the targets (left side) of assignment
                    for target in &assign.targets {
                        if Self::expr_contains_range(target, covering_node.node().range()) {
                            return ReferenceKind::Write;
                        }
                    }
                }
                AnyNodeRef::StmtAnnAssign(ann_assign) => {
                    // Check if our node is the target (left side) of annotated assignment
                    if Self::expr_contains_range(&ann_assign.target, covering_node.node().range()) {
                        return ReferenceKind::Write;
                    }
                }
                AnyNodeRef::StmtAugAssign(aug_assign) => {
                    // Check if our node is the target (left side) of augmented assignment
                    if Self::expr_contains_range(&aug_assign.target, covering_node.node().range()) {
                        return ReferenceKind::Write;
                    }
                }
                // For loop targets are writes
                AnyNodeRef::StmtFor(for_stmt) => {
                    if Self::expr_contains_range(&for_stmt.target, covering_node.node().range()) {
                        return ReferenceKind::Write;
                    }
                }
                // With statement targets are writes
                AnyNodeRef::WithItem(with_item) => {
                    if let Some(optional_vars) = &with_item.optional_vars {
                        if Self::expr_contains_range(optional_vars, covering_node.node().range()) {
                            return ReferenceKind::Write;
                        }
                    }
                }
                // Exception handler names are writes
                AnyNodeRef::ExceptHandlerExceptHandler(handler) => {
                    if let Some(name) = &handler.name {
                        if Self::node_contains_range(
                            AnyNodeRef::from(name),
                            covering_node.node().range(),
                        ) {
                            return ReferenceKind::Write;
                        }
                    }
                }
                AnyNodeRef::StmtFunctionDef(func) => {
                    if Self::node_contains_range(
                        AnyNodeRef::from(&func.name),
                        covering_node.node().range(),
                    ) {
                        return ReferenceKind::Other;
                    }
                }
                AnyNodeRef::StmtClassDef(class) => {
                    if Self::node_contains_range(
                        AnyNodeRef::from(&class.name),
                        covering_node.node().range(),
                    ) {
                        return ReferenceKind::Other;
                    }
                }
                AnyNodeRef::Parameter(param) => {
                    if Self::node_contains_range(
                        AnyNodeRef::from(&param.name),
                        covering_node.node().range(),
                    ) {
                        return ReferenceKind::Other;
                    }
                }
                AnyNodeRef::StmtGlobal(_) | AnyNodeRef::StmtNonlocal(_) => {
                    return ReferenceKind::Other;
                }
                _ => {}
            }
        }

        // Default to read
        ReferenceKind::Read
    }

    /// Helper to check if a node contains a given range
    fn node_contains_range(node: AnyNodeRef<'_>, range: TextRange) -> bool {
        node.range().contains_range(range)
    }

    /// Helper to check if an expression contains a given range
    fn expr_contains_range(expr: &ast::Expr, range: TextRange) -> bool {
        expr.range().contains_range(range)
    }
}
