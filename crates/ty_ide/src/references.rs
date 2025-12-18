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
use crate::{Db, NavigationTargets, ReferenceKind, ReferenceTarget};
use ruff_db::files::File;
use ruff_python_ast::find_node::CoveringNode;
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
        .declaration_targets(db)?;

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

    // Check if the symbol is potentially visible outside of this module
    if search_across_files && is_symbol_externally_visible(goto_target) {
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

    if references.is_empty() {
        None
    } else {
        Some(references)
    }
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

    /// Determines whether the given covering node is a reference to
    /// the symbol we are searching for
    fn check_reference_from_covering_node(&mut self, covering_node: &CoveringNode<'_>) {
        // Use the start of the covering node as the offset. Any offset within
        // the node is fine here. Offsets matter only for import statements
        // where the identifier might be a multi-part module name.
        let offset = covering_node.node().start();
        if let Some(goto_target) =
            GotoTarget::from_covering_node(self.model, covering_node, offset, self.tokens)
        {
            // Get the definitions for this goto target
            if let Some(current_definitions) = goto_target
                .get_definition_targets(self.model, self.mode.to_import_alias_resolution())
                .and_then(|definitions| definitions.declaration_targets(self.model.db()))
            {
                // Check if any of the current definitions match our target definitions
                if self.navigation_targets_match(&current_definitions) {
                    // Determine if this is a read or write reference
                    let kind = self.determine_reference_kind(covering_node);
                    let target =
                        ReferenceTarget::new(self.model.file(), covering_node.node().range(), kind);
                    self.references.push(target);
                }
            }
        }
    }

    /// Check if `Vec<NavigationTarget>` match our target definitions
    fn navigation_targets_match(&self, current_targets: &NavigationTargets) -> bool {
        // Since we're comparing the same symbol, all definitions should be equivalent
        // We only need to check against the first target definition
        if let Some(first_target) = self.target_definitions.iter().next() {
            for current_target in current_targets {
                if current_target.file == first_target.file
                    && current_target.focus_range == first_target.focus_range
                {
                    return true;
                }
            }
        }
        false
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
