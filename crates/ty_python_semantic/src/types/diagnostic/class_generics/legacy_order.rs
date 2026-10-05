//! Finite base selection and message fragments for legacy class default ordering.

use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};

use crate::types::typevar::TypeVarInstance;

/// The parameters preceding the last two offenders, in their declared order.
#[derive(Debug)]
pub(in crate::types) struct OrderVariables<'a, 'db> {
    first: Option<TypeVarInstance<'db>>,
    remaining: &'a [TypeVarInstance<'db>],
}

impl<'db> OrderVariables<'_, 'db> {
    pub(in crate::types) fn len(&self) -> usize {
        usize::from(self.first.is_some()) + self.remaining.len()
    }

    pub(in crate::types) fn next(&mut self) -> Option<TypeVarInstance<'db>> {
        if let Some(first) = self.first.take() {
            return Some(first);
        }
        let (first, remaining) = self.remaining.split_first()?;
        self.remaining = remaining;
        Some(*first)
    }
}

/// A nonempty offender list split to preserve [`crate::diagnostic::format_enumeration`]'s name-read order.
/// Resolving names uses admitted source-field operations that can refuse. Keeping the same
/// read sequence preserves which name has been reached at a refusal; displayed names remain
/// in declaration order.
#[derive(Debug)]
pub(in crate::types) enum OrderTail<'a, 'db> {
    Single,
    Multiple {
        last: TypeVarInstance<'db>,
        penultimate: TypeVarInstance<'db>,
        earlier: OrderVariables<'a, 'db>,
    },
}

impl<'a, 'db> OrderTail<'a, 'db> {
    pub(in crate::types) fn new(
        first: TypeVarInstance<'db>,
        remaining: &'a [TypeVarInstance<'db>],
    ) -> Self {
        let Some((&last, preceding)) = remaining.split_last() else {
            return Self::Single;
        };
        let (penultimate, earlier) = if let Some((&penultimate, preceding)) = preceding.split_last() {
            (penultimate, OrderVariables { first: Some(first), remaining: preceding })
        } else {
            (first, OrderVariables { first: None, remaining: &[] })
        };
        Self::Multiple { last, penultimate, earlier }
    }
}

/// Text that uses borrowed names after their semantic reads have completed.
#[derive(Debug, Clone, Copy)]
pub(in crate::types) enum OrderText<'a> {
    Headline,
    Concise { offender: &'a Name, default: &'a Name },
    Single(&'a Name),
    Multiple { earlier: &'a [&'a Name], penultimate: &'a Name, last: &'a Name },
    EarlierDefault(&'a Name),
}

impl<'a> OrderText<'a> {
    /// Selects one indexed borrowed message fragment; `None` ends the enumeration.
    pub(in crate::types) fn part(self, index: usize) -> Option<&'a str> {
        match self {
            Self::Headline => match index {
                0 => Some("Type parameters without defaults cannot follow type parameters with defaults"),
                _ => None,
            },
            Self::Concise { offender, default } => match index {
                0 => Some("Type parameter `"),
                1 => Some(offender.as_str()),
                2 => Some("` without a default cannot follow earlier parameter `"),
                3 => Some(default.as_str()),
                4 => Some("` with a default"),
                _ => None,
            },
            Self::Single(name) => match index {
                0 => Some("Type variable `"),
                1 => Some(name.as_str()),
                2 => Some("` does not have a default"),
                _ => None,
            },
            Self::Multiple { earlier, penultimate, last } => {
                if index == 0 {
                    return Some("Type variables ");
                }
                let index = index - 1;
                if let Some(name) = earlier.get(index / 3) {
                    return match index % 3 {
                        0 => Some("`"),
                        1 => Some(name.as_str()),
                        _ => Some("`, "),
                    };
                }
                match index.checked_sub(earlier.len().checked_mul(3)?)? {
                    0 => Some("`"),
                    1 => Some(penultimate.as_str()),
                    2 => Some("` and `"),
                    3 => Some(last.as_str()),
                    4 => Some("` do not have defaults"),
                    _ => None,
                }
            }
            Self::EarlierDefault(name) => match index {
                0 => Some("Earlier TypeVar `"),
                1 => Some(name.as_str()),
                2 => Some("` does"),
                _ => None,
            },
        }
    }
}

/// Selects the matching AST base's slice, falling back to its whole expression.
pub(in crate::types) fn base_range(
    node: &ast::StmtClassDef,
    base_count: usize,
    index: Option<usize>,
) -> Result<TextRange, &'static str> {
    if node.bases().len() != base_count {
        return Err("legacy generic base types and AST entries differ");
    }
    let index = index.ok_or("legacy generic context has no Generic or Protocol base")?;
    let base = node.bases().get(index).ok_or("legacy generic base has no matching AST entry")?;
    Ok(base.as_subscript_expr().map(|subscript| &*subscript.slice).unwrap_or(base).range())
}
