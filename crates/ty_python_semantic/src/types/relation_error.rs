use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::Type;
use crate::types::tuple::TupleLength;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeRelationHint<'db> {
    Root,
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
    TypedDictNotAssignableToDict,
    IncompatibleReturnTypes {
        source: Type<'db>,
        target: Type<'db>,
    },
    IncompatibleParameterTypes {
        source: Type<'db>,
        target: Type<'db>,
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

impl<'db> TypeRelationHint<'db> {
    fn render(&self, db: &'db dyn Db) -> Option<String> {
        Some(match self {
            Self::Root => {
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
            Self::TypedDictNotAssignableToDict => {
                "`TypedDict` types are not assignable to `dict` (consider using `Mapping` instead)"
                    .to_string()
            }
            Self::IncompatibleReturnTypes { source, target } => format!(
                "incompatible return types `{}` and `{}`",
                source.display(db),
                target.display(db),
            ),
            Self::IncompatibleParameterTypes { source, target } => format!(
                "incompatible parameter types `{}` and `{}`",
                source.display(db),
                target.display(db),
            ),
            Self::ParameterNameMismatch {
                source_name,
                target_name,
            } => format!(
                "parameter `{source_name}` does not match `{target_name}` (and can be used as a keyword parameter)",
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
            Self::TypeNotCompatibleWithProtocol { ty, protocol } => format!(
                "type `{}` is not compatible with protocol `{}`",
                ty.display(db),
                protocol.display(db),
            ),
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ErrorContextNode<'db> {
    hint: TypeRelationHint<'db>,
    children: Vec<ErrorContextNode<'db>>,
}

impl Default for ErrorContextNode<'_> {
    fn default() -> Self {
        Self {
            hint: TypeRelationHint::Root,
            children: Vec::new(),
        }
    }
}

impl<'db> ErrorContextNode<'db> {
    /// Returns `true` if this node has no renderable content.
    fn is_empty(&self) -> bool {
        matches!(self.hint, TypeRelationHint::Root) && self.children.is_empty()
    }

    fn render_messages(
        &self,
        db: &'db dyn Db,
        messages: &mut Vec<String>,
        prefix: &str,
        continuation: &str,
    ) {
        if let Some(message) = self.hint.render(db) {
            messages.push(format!("{prefix}{message}"));
        }

        let num_children = self.children.len();
        for (index, child) in self.children.iter().enumerate() {
            let is_last = index == num_children - 1;
            let (child_prefix, child_continuation) = if is_last {
                (format!("{continuation}└── "), format!("{continuation}    "))
            } else {
                (format!("{continuation}├── "), format!("{continuation}│   "))
            };
            child.render_messages(db, messages, &child_prefix, &child_continuation);
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypeRelationErrorContext<'db> {
    root: Rc<RefCell<ErrorContextNode<'db>>>,
    enabled: Cell<bool>,
}

impl PartialEq for TypeRelationErrorContext<'_> {
    fn eq(&self, other: &Self) -> bool {
        *self.root.borrow() == *other.root.borrow()
    }
}

impl Eq for TypeRelationErrorContext<'_> {}

impl<'db> From<TypeRelationHint<'db>> for TypeRelationErrorContext<'db> {
    fn from(hint: TypeRelationHint<'db>) -> Self {
        let context = TypeRelationErrorContext::enabled();
        context.push(hint);
        context
    }
}

impl<'db> TypeRelationErrorContext<'db> {
    pub(crate) fn disabled() -> Self {
        Self {
            root: Rc::default(),
            enabled: Cell::new(false),
        }
    }

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

    pub(crate) fn is_empty(&self) -> bool {
        self.root.borrow().is_empty()
    }

    pub(crate) fn push(&self, hint: TypeRelationHint<'db>) {
        if !self.is_enabled() {
            return;
        }
        let root = self.root.take();
        let children = if root.is_empty() { vec![] } else { vec![root] };
        *self.root.borrow_mut() = ErrorContextNode { hint, children };
    }

    pub(crate) fn set_root(
        &self,
        hint: TypeRelationHint<'db>,
        children: Vec<TypeRelationErrorContext<'db>>,
    ) {
        if !self.is_enabled() {
            return;
        }
        *self.root.borrow_mut() = ErrorContextNode {
            hint,
            children: children
                .into_iter()
                .map(|child_context| child_context.root.take())
                .filter(|child| !child.is_empty())
                .collect(),
        };
    }

    pub(crate) fn take(&self) -> Self {
        TypeRelationErrorContext {
            root: Rc::new(RefCell::new(std::mem::take(&mut *self.root.borrow_mut()))),
            enabled: Cell::new(self.enabled.get()),
        }
    }

    pub fn info_messages(&self, db: &'db dyn Db) -> Vec<String> {
        let mut messages = Vec::new();
        self.root
            .borrow()
            .render_messages(db, &mut messages, "", "");
        messages
    }
}
