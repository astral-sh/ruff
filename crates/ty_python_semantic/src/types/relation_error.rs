use std::cell::RefCell;
use std::rc::Rc;

use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::Type;
use crate::types::tuple::TupleLength;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeRelationHint<'db> {
    UnionElementNotAssignable {
        element: Type<'db>,
        union: Type<'db>,
        target: Type<'db>,
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
    fn render(&self, db: &'db dyn Db) -> String {
        match self {
            Self::UnionElementNotAssignable {
                element,
                union,
                target,
            } => format!(
                "element `{}` of union `{}` is not assignable to `{}`",
                element.display(db),
                union.display(db),
                target.display(db),
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
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct TypeRelationErrorContext<'db> {
    stack: Rc<RefCell<Vec<TypeRelationHint<'db>>>>,
}

impl PartialEq for TypeRelationErrorContext<'_> {
    fn eq(&self, other: &Self) -> bool {
        *self.stack.borrow() == *other.stack.borrow()
    }
}

impl Eq for TypeRelationErrorContext<'_> {}

impl<'db> TypeRelationErrorContext<'db> {
    pub(crate) fn new() -> Self {
        Self {
            stack: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub(crate) fn push(&self, message: TypeRelationHint<'db>) {
        self.stack.borrow_mut().push(message);
    }

    pub fn info_messages(&self, db: &'db dyn Db) -> Vec<String> {
        let stack = self.stack.borrow();
        let len = stack.len();
        stack
            .iter()
            .rev()
            .enumerate()
            .map(|(i, message)| {
                let message = message.render(db);
                if i == 0 {
                    message
                } else if i < len - 1 {
                    format!("├── {message}")
                } else {
                    format!("└── {message}")
                }
            })
            .collect()
    }
}
