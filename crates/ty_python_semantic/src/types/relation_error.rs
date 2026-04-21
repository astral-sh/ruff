/// This module defines a tree structure for collecting contextual information about type relation errors
/// ("why is this complex type not assignable to that other complex type?").
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ruff_python_ast::name::Name;

use crate::types::context::LintDiagnosticGuard;
use crate::types::tuple::TupleLength;
use crate::types::{Type, TypedDictType};
use crate::{Db, FxOrderSet};

/// Identifies a parameter, either by name or by position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParameterDescription {
    Named(Name),
    /// 0-based index
    Index(usize),
}

impl ParameterDescription {
    pub(crate) fn new(index: usize, name: Option<&Name>) -> Self {
        match name {
            Some(name) => Self::Named(name.clone()),
            None => Self::Index(index),
        }
    }
}

impl std::fmt::Display for ParameterDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(name) => write!(f, "parameter `{name}`"),
            Self::Index(0) => f.write_str("the first parameter"),
            Self::Index(1) => f.write_str("the second parameter"),
            Self::Index(2) => f.write_str("the third parameter"),
            Self::Index(n) => write!(f, "parameter {}", n + 1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorContext<'db> {
    /// No additional context is available.
    Empty,
    NotAllUnionElementsAssignable {
        element: Type<'db>,
        union: Type<'db>,
        target: Type<'db>,
    },
    NotAssignableToAnyUnionElement {
        source: Type<'db>,
        union: Type<'db>,
    },
    NotAssignableToNOtherUnionElements {
        n: usize,
    },
    TypedDictNotAssignableToDict(TypedDictType<'db>),
    IncompatibleReturnTypes {
        source: Type<'db>,
        target: Type<'db>,
    },
    IncompatibleParameterTypes {
        source: Type<'db>,
        target: Type<'db>,
        parameter: ParameterDescription,
    },
    ParameterNameMismatch {
        source_name: Name,
        target_name: Name,
    },
    ParameterMustAcceptKeywordArguments {
        source_name: Option<Name>,
        target_name: Name,
    },
    ParameterMustAcceptPositionalArguments {
        name: Name,
    },
    TupleLengthMismatch {
        source_len: usize,
        target_len: TupleLength,
    },
    TupleElementNotCompatible {
        source: Type<'db>,
        target: Type<'db>,
        element_index: usize,
        element_count: usize,
    },
    TypeNotCompatibleWithProtocol {
        ty: Type<'db>,
        protocol: Type<'db>,
    },
    ProtocolMemberNotDefined {
        member_name: Name,
        ty: Type<'db>,
    },
    ProtocolMemberIncompatible {
        member_name: Name,
    },
}

impl<'db> ErrorContext<'db> {
    fn render(
        &self,
        db: &'db dyn Db,
        help_messages: &mut FxOrderSet<HelpMessages>,
    ) -> Option<String> {
        Some(match self {
            Self::Empty => {
                return None;
            }
            Self::NotAllUnionElementsAssignable {
                element,
                union,
                target,
            } => format!(
                "element `{}` of union `{}` is not assignable to `{}`",
                element.display(db),
                union.display(db),
                target.display(db),
            ),
            Self::NotAssignableToAnyUnionElement { source, union } => format!(
                "type `{}` is not assignable to any element of the union `{}`",
                source.display(db),
                union.display(db),
            ),
            Self::NotAssignableToNOtherUnionElements { n } => format!(
                "... omitted {n} union element{} without additional context",
                if *n == 1 { "" } else { "s" }
            ),
            Self::TypedDictNotAssignableToDict(typed_dict) => {
                help_messages.insert(HelpMessages::TypedDictNotAssignableToDict);
                help_messages.insert(HelpMessages::ConsiderUsingMappingInsteadOfDict);

                let name = match typed_dict {
                    TypedDictType::Class(class) => format!("TypedDict `{}`", class.name(db)),
                    TypedDictType::Synthesized(_) => "TypedDict".to_string(),
                };
                format!("{name} is not assignable to `dict`")
            }
            Self::IncompatibleReturnTypes { source, target } => format!(
                "incompatible return types: `{source}` is not assignable to `{target}`",
                source = source.display(db),
                target = target.display(db),
            ),
            Self::IncompatibleParameterTypes {
                source,
                target,
                parameter,
            } => {
                // reversed order due to covariance
                format!(
                    "{parameter} has an incompatible type: `{target}` is not assignable to `{source}`",
                    source = source.display(db),
                    target = target.display(db),
                )
            }
            Self::ParameterNameMismatch {
                source_name,
                target_name,
            } => format!(
                "the parameter named `{source_name}` does not match `{target_name}` (and can be used as a keyword parameter)",
            ),
            Self::ParameterMustAcceptKeywordArguments {
                source_name,
                target_name,
            } => {
                if let Some(source_name) = source_name {
                    format!(
                        "parameter `{source_name}` is positional-only but must also accept keyword arguments",
                    )
                } else {
                    format!("parameter `{target_name}` must accept keyword arguments")
                }
            }
            Self::ParameterMustAcceptPositionalArguments { name } => format!(
                "parameter `{name}` is keyword-only but must also accept positional arguments",
            ),
            Self::TupleLengthMismatch {
                source_len,
                target_len,
            } => format!(
                "a tuple of length {source_len} is not assignable to a tuple of length {}",
                target_len.display_minimum(),
            ),
            Self::TupleElementNotCompatible {
                source,
                target,
                element_index,
                element_count,
            } => {
                let which = match (*element_index, *element_count) {
                    (1, _) => "the first tuple element".to_string(),
                    (2, _) => "the second tuple element".to_string(),
                    (n, c) if n == c => "the last tuple element".to_string(),
                    (3, _) => "the third tuple element".to_string(),
                    (n, c) => format!("tuple element {n} of {c}"),
                };
                format!(
                    "{which} is not compatible: `{source}` is not assignable to `{target}`",
                    source = source.display(db),
                    target = target.display(db)
                )
            }
            Self::TypeNotCompatibleWithProtocol { ty, protocol } => {
                if let Type::ProtocolInstance(_) = ty {
                    format!(
                        "protocol `{}` is not assignable to protocol `{}`",
                        ty.display(db),
                        protocol.display(db),
                    )
                } else {
                    format!(
                        "type `{}` is not assignable to protocol `{}`",
                        ty.display(db),
                        protocol.display(db),
                    )
                }
            }
            Self::ProtocolMemberNotDefined { member_name, ty } => format!(
                "protocol member `{member_name}` is not defined on type `{}`",
                ty.display(db),
            ),
            Self::ProtocolMemberIncompatible { member_name } => {
                format!("protocol member `{member_name}` is incompatible")
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum HelpMessages {
    TypedDictNotAssignableToDict,
    ConsiderUsingMappingInsteadOfDict,
}

impl std::fmt::Display for HelpMessages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelpMessages::TypedDictNotAssignableToDict => {
                f.write_str("A TypedDict is not usually assignable to any `dict[..]` type; `dict` types allow destructive operations like `clear()`.")
            }
            HelpMessages::ConsiderUsingMappingInsteadOfDict => {
                f.write_str("Consider using `Mapping[..]` instead of `dict[..]`.")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ErrorContextNode<'db> {
    context: ErrorContext<'db>,
    children: Vec<ErrorContextNode<'db>>,
}

impl Default for ErrorContextNode<'_> {
    fn default() -> Self {
        Self {
            context: ErrorContext::Empty,
            children: Vec::new(),
        }
    }
}

impl<'db> ErrorContextNode<'db> {
    /// Returns `true` if this node has no renderable content.
    fn is_empty(&self) -> bool {
        matches!(self.context, ErrorContext::Empty) && self.children.is_empty()
    }

    fn render_tree(
        &self,
        db: &'db dyn Db,
        output_lines: &mut Vec<String>,
        help_messages: &mut FxOrderSet<HelpMessages>,
        prefix: &str,
        continuation: &str,
    ) {
        if let Some(line) = self.context.render(db, help_messages) {
            output_lines.push(format!("{prefix}{line}"));
        }

        let num_children = self.children.len();
        for (index, child) in self.children.iter().enumerate() {
            let is_last = index == num_children - 1;
            let (child_prefix, child_continuation) = if is_last {
                (format!("{continuation}└── "), format!("{continuation}    "))
            } else {
                (format!("{continuation}├── "), format!("{continuation}│   "))
            };
            child.render_tree(
                db,
                output_lines,
                help_messages,
                &child_prefix,
                &child_continuation,
            );
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ErrorContextTree<'db> {
    root: Rc<RefCell<ErrorContextNode<'db>>>,
    enabled: Cell<bool>,
}

impl PartialEq for ErrorContextTree<'_> {
    fn eq(&self, other: &Self) -> bool {
        *self.root.borrow() == *other.root.borrow()
    }
}

impl Eq for ErrorContextTree<'_> {}

impl<'db> From<ErrorContext<'db>> for ErrorContextTree<'db> {
    fn from(context: ErrorContext<'db>) -> Self {
        Self {
            root: Rc::new(RefCell::new(ErrorContextNode {
                context,
                children: Vec::new(),
            })),
            enabled: Cell::new(true),
        }
    }
}

impl<'db> ErrorContextTree<'db> {
    /// Create a new, empty error context tree with collection disabled.
    pub(crate) fn disabled() -> Self {
        Self {
            root: Rc::default(),
            enabled: Cell::new(false),
        }
    }

    /// Create a new, empty error context tree with collection enabled.
    pub(crate) fn enabled() -> Self {
        Self {
            root: Rc::default(),
            enabled: Cell::new(true),
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    /// Returns `true` if the tree has no renderable content.
    pub(crate) fn is_empty(&self) -> bool {
        self.root.borrow().is_empty()
    }

    /// Push a new error context node, making the existing tree a child of the new context.
    pub(crate) fn push(&self, get_context: impl FnOnce() -> ErrorContext<'db>) {
        if !self.is_enabled() {
            return;
        }
        let context = get_context();
        let root = self.root.take();
        let children = if root.is_empty() { vec![] } else { vec![root] };
        *self.root.borrow_mut() = ErrorContextNode { context, children };
    }

    /// Overwrite the error context tree with a new root context and child nodes.
    pub(crate) fn set(
        &self,
        context: ErrorContext<'db>,
        children: impl IntoIterator<Item = ErrorContextTree<'db>>,
    ) {
        if !self.is_enabled() {
            return;
        }
        *self.root.borrow_mut() = ErrorContextNode {
            context,
            children: children
                .into_iter()
                .map(|child_context| child_context.root.take())
                .filter(|child| !child.is_empty())
                .collect(),
        };
    }

    /// Return the full tree, replacing it with an empty tree.
    pub(crate) fn take(&self) -> Self {
        ErrorContextTree {
            root: Rc::new(RefCell::new(std::mem::take(&mut *self.root.borrow_mut()))),
            enabled: Cell::new(self.enabled.get()),
        }
    }

    /// Render the error context tree as info sub-diagnostics on `diag`.
    pub(in crate::types) fn attach_to(
        &self,
        db: &'db dyn Db,
        diag: &mut LintDiagnosticGuard<'_, '_>,
    ) {
        let mut output_lines = Vec::new();
        let mut help_messages = FxOrderSet::default();
        self.root
            .borrow()
            .render_tree(db, &mut output_lines, &mut help_messages, "", "");
        for line in output_lines {
            diag.info(line);
        }
        for help_message in help_messages {
            diag.help(help_message.to_string());
        }
    }
}
