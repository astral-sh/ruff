//! Utilities for semantic analysis of `__all__` definitions

use bitflags::bitflags;

use ruff_python_ast::{self as ast, helpers::map_subscript, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::SemanticModel;

bitflags! {
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    pub struct DunderAllFlags: u8 {
        /// The right-hand-side assignment to __all__ uses an invalid expression (i.e., an
        /// expression that doesn't evaluate to a container type, like `__all__ = 1`).
        const INVALID_FORMAT = 1 << 0;
        /// The right-hand-side assignment to __all__ contains an invalid member (i.e., a
        /// non-string, like `__all__ = [1]`).
        const INVALID_OBJECT = 1 << 1;
    }
}

/// Abstraction for a string inside an `__all__` definition,
/// e.g. `"foo"` in `__all__ = ["foo"]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DunderAllName<'a> {
    /// The value of the string inside the `__all__` definition
    name: &'a str,

    /// The range of the string inside the `__all__` definition
    range: TextRange,
}

impl DunderAllName<'_> {
    pub fn name(&self) -> &str {
        self.name
    }
}

impl Ranged for DunderAllName<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Abstraction for a collection of names inside an `__all__` definition,
/// e.g. `["foo", "bar"]` in `__all__ = ["foo", "bar"]`
#[derive(Debug, Clone)]
pub struct DunderAllDefinition<'a> {
    /// The range of the `__all__` identifier.
    range: TextRange,
    /// The names inside the `__all__` definition.
    names: Vec<DunderAllName<'a>>,
}

impl<'a> DunderAllDefinition<'a> {
    /// Initialize a new [`DunderAllDefinition`] instance.
    pub fn new(range: TextRange, names: Vec<DunderAllName<'a>>) -> Self {
        Self { range, names }
    }

    /// The names inside the `__all__` definition.
    pub fn names(&self) -> &[DunderAllName<'a>] {
        &self.names
    }
}

impl Ranged for DunderAllDefinition<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl<'a> SemanticModel<'a> {
    /// Extract the names bound to a given __all__ assignment.
    pub fn extract_dunder_all_names<'expr>(
        &self,
        stmt: &'expr Stmt,
    ) -> (Vec<DunderAllName<'expr>>, DunderAllFlags) {
        fn add_to_names<'expr>(
            elts: &'expr [Expr],
            names: &mut Vec<DunderAllName<'expr>>,
            flags: &mut DunderAllFlags,
        ) {
            for elt in elts {
                if let Expr::StringLiteral(ast::ExprStringLiteral { value, range }) = elt {
                    names.push(DunderAllName {
                        name: value.to_str(),
                        range: *range,
                    });
                } else {
                    *flags |= DunderAllFlags::INVALID_OBJECT;
                }
            }
        }

        let mut names: Vec<DunderAllName> = vec![];
        let mut flags = DunderAllFlags::empty();

        if let Some(value) = match stmt {
            Stmt::Assign(ast::StmtAssign { value, .. }) => Some(value),
            Stmt::AnnAssign(ast::StmtAnnAssign { value, .. }) => value.as_ref(),
            Stmt::AugAssign(ast::StmtAugAssign { value, .. }) => Some(value),
            _ => None,
        } {
            if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = value.as_ref() {
                let mut current_left = left;
                let mut current_right = right;
                loop {
                    // Process the right side, which should be a "real" value.
                    let (elts, new_flags) = self.extract_dunder_all_elts(current_right);
                    flags |= new_flags;
                    if let Some(elts) = elts {
                        add_to_names(elts, &mut names, &mut flags);
                    }

                    // Process the left side, which can be a "real" value or the "rest" of the
                    // binary operation.
                    if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = current_left.as_ref() {
                        current_left = left;
                        current_right = right;
                    } else {
                        let (elts, new_flags) = self.extract_dunder_all_elts(current_left);
                        flags |= new_flags;
                        if let Some(elts) = elts {
                            add_to_names(elts, &mut names, &mut flags);
                        }
                        break;
                    }
                }
            } else {
                let (elts, new_flags) = self.extract_dunder_all_elts(value);
                flags |= new_flags;
                if let Some(elts) = elts {
                    add_to_names(elts, &mut names, &mut flags);
                }
            }
        }

        (names, flags)
    }

    fn extract_dunder_all_elts<'expr>(
        &self,
        expr: &'expr Expr,
    ) -> (Option<&'expr [Expr]>, DunderAllFlags) {
        match expr {
            Expr::List(ast::ExprList { elts, .. }) => {
                return (Some(elts), DunderAllFlags::empty());
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                return (Some(elts), DunderAllFlags::empty());
            }
            Expr::ListComp(_) => {
                // Allow comprehensions, even though we can't statically analyze them.
                return (None, DunderAllFlags::empty());
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                // Ex) `__all__ = __all__ + multiprocessing.__all__`
                if id == "__all__" {
                    return (None, DunderAllFlags::empty());
                }
            }
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                // Ex) `__all__ = __all__ + multiprocessing.__all__`
                if attr == "__all__" {
                    return (None, DunderAllFlags::empty());
                }
            }
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                // Allow `tuple()`, `list()`, and their generic forms, like `list[int]()`.
                if arguments.keywords.is_empty() && arguments.args.len() <= 1 {
                    if self
                        .resolve_builtin_symbol(map_subscript(func))
                        .is_some_and(|symbol| matches!(symbol, "tuple" | "list"))
                    {
                        let [arg] = arguments.args.as_ref() else {
                            return (None, DunderAllFlags::empty());
                        };
                        match arg {
                            Expr::List(ast::ExprList { elts, .. })
                            | Expr::Set(ast::ExprSet { elts, .. })
                            | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                return (Some(elts), DunderAllFlags::empty());
                            }
                            _ => {
                                // We can't analyze other expressions, but they must be
                                // valid, since the `list` or `tuple` call will ultimately
                                // evaluate to a list or tuple.
                                return (None, DunderAllFlags::empty());
                            }
                        }
                    }
                }
            }
            Expr::Named(ast::ExprNamed { value, .. }) => {
                // Allow, e.g., `__all__ += (value := ["A", "B"])`.
                return self.extract_dunder_all_elts(value);
            }
            _ => {}
        }
        (None, DunderAllFlags::INVALID_FORMAT)
    }
}
