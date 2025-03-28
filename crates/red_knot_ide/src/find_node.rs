use ruff_python_ast as ast;
use ruff_python_ast::visitor::source_order;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Keyword};
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::fmt::Formatter;

/// Returns the node with a minimal range that fully contains `range`.
///
/// Returns the first node if multiple nodes fully cover `range`.
///
/// Returns `root` if no child node fully covers `range` or if `range` is outside `root`.
pub(crate) fn covering_node(root: AnyNodeRef, range: TextRange) -> CoveringNode {
    struct Visitor<'a> {
        range: TextRange,
        ancestors: Vec<AnyNodeRef<'a>>,
    }

    impl<'a> Visitor<'a> {
        /// The [`SourceOrderVisitor`] doesn't visit identifiers even though they're nodes in the AST.
        /// This is something that we should fix but it has a rather big fallout because it may change
        /// how the formatter places comments. This implementation manually traverses into identifiers where necessary.
        ///
        /// Note: The `visit_identifier` method isn't called in-source-order. Doing so would require duplicating
        /// more code and isn't necessary for finding the node with the minimal covering range.
        fn visit_identifier(&mut self, identifier: &'a ast::Identifier) {
            if identifier.range.contains_range(self.range) {
                self.ancestors.push(identifier.into());
            }
        }
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor<'a> {
        fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
            // If the node fully contains the range, than it is a possible match but traverse into its children
            // to see if there's a closer node.
            if node.range().contains_range(self.range) {
                self.ancestors.push(node);
                TraversalSignal::Traverse
            } else {
                TraversalSignal::Skip
            }
        }

        fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
            source_order::walk_stmt(self, stmt);

            match stmt {
                ast::Stmt::ClassDef(class) => {
                    self.visit_identifier(&class.name);
                }
                ast::Stmt::FunctionDef(function) => {
                    self.visit_identifier(&function.name);
                }
                ast::Stmt::ImportFrom(import) => {
                    if let Some(module) = import.module.as_ref() {
                        self.visit_identifier(module);
                    }
                }
                _ => {}
            }
        }

        fn visit_except_handler(&mut self, except_handler: &'a ast::ExceptHandler) {
            source_order::walk_except_handler(self, except_handler);

            if let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                name: Some(name),
                ..
            }) = except_handler
            {
                self.visit_identifier(name);
            }
        }

        fn visit_parameter(&mut self, arg: &'a ast::Parameter) {
            source_order::walk_parameter(self, arg);

            self.visit_identifier(&arg.name);
        }

        fn visit_alias(&mut self, alias: &'a ast::Alias) {
            source_order::walk_alias(self, alias);

            self.visit_identifier(&alias.name);
        }

        fn visit_keyword(&mut self, keyword: &'a Keyword) {
            source_order::walk_keyword(self, keyword);

            if let Some(arg) = keyword.arg.as_ref() {
                self.visit_identifier(arg);
            }
        }
    }

    let mut visitor = Visitor {
        range,
        ancestors: Vec::new(),
    };

    root.visit_source_order(&mut visitor);

    let minimal = visitor.ancestors.pop().unwrap_or(root);
    CoveringNode {
        node: minimal,
        ancestors: visitor.ancestors,
    }
}

/// The node with a minimal range that fully contains `range`.
pub(crate) struct CoveringNode<'a> {
    node: AnyNodeRef<'a>,
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> CoveringNode<'a> {
    pub(crate) fn node(&self) -> AnyNodeRef<'a> {
        self.node
    }

    pub(crate) fn parent(&self) -> Option<AnyNodeRef<'a>> {
        self.ancestors.last().copied()
    }

    /// Finds the first node that fully covers the range and fulfills the given predicate.
    pub(crate) fn find(mut self, f: impl Fn(AnyNodeRef<'a>) -> bool) -> Result<Self, Self> {
        if f(self.node) {
            return Ok(self);
        }

        match self.ancestors.iter().rposition(|node| f(*node)) {
            Some(index) => {
                let node = { self.ancestors.drain(index..).next().unwrap() };

                Ok(Self {
                    node,
                    ancestors: self.ancestors,
                })
            }
            None => Err(self),
        }
    }
}

impl fmt::Debug for CoveringNode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeWithAncestors")
            .field(&self.node)
            .finish()
    }
}
