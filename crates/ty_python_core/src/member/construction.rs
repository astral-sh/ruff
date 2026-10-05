//! Iterative construction of the compact text and segment representation of a member place.

use std::ops::ControlFlow;

use char_str::{CharStr, CharString, format_char};
use ruff_python_ast as ast;
use ruff_text_size::{TextLen as _, TextSize};
use smallvec::SmallVec;

use super::{MAX_OFFSET, MemberExprBuilder, MemberPathPart, SegmentInfo, SegmentKind};

pub(crate) enum MemberConstruction<'ast> {
    Name(&'ast ast::ExprName),
    Member(MemberExprBuilder),
}

#[derive(Clone, Copy)]
enum PendingSegment<'ast> {
    Attribute(&'ast ast::Identifier),
    Subscript(&'ast ast::Expr),
}

#[derive(Clone, Copy)]
enum State<'ast> {
    Descend(ast::ExprRef<'ast>),
    Append,
    Measure {
        source: SubscriptSource<'ast>,
        index: usize,
        bytes: usize,
    },
    Render {
        source: SubscriptSource<'ast>,
        bytes: usize,
        pieces: usize,
    },
    Concatenate,
    Finished,
}

/// The pending segments borrow the AST rather than owning a recursive continuation.
/// Subscripts are inspected only after their receiver has produced a valid place, and
/// are rendered from the innermost receiver outward, including on invalid outer paths.
pub(crate) struct MemberExprCursor<'ast> {
    state: State<'ast>,
    pending: SmallVec<[PendingSegment<'ast>; 8]>,
    parts: SmallVec<[MemberPathPart<'ast>; 8]>,
    segments: SmallVec<[SegmentInfo; 8]>,
    path_len: TextSize,
}

impl<'ast> MemberExprCursor<'ast> {
    pub(crate) fn new(expression: ast::ExprRef<'ast>) -> Self {
        Self {
            state: State::Descend(expression),
            pending: SmallVec::new_const(),
            parts: SmallVec::new_const(),
            segments: SmallVec::new_const(),
            path_len: TextSize::new(0),
        }
    }

    pub(crate) fn cost(&self) -> Option<(usize, usize)> {
        let cost = match self.state {
            State::Descend(ast::ExprRef::Attribute(_) | ast::ExprRef::Subscript(_)) => push_cost(
                self.pending.len(),
                self.pending.capacity(),
                size_of::<PendingSegment<'_>>(),
            )?,
            State::Descend(ast::ExprRef::Name(name)) if !self.pending.is_empty() => {
                u32::try_from(name.id.len()).ok()?;
                push_cost(
                    self.parts.len(),
                    self.parts.capacity(),
                    size_of::<MemberPathPart<'_>>(),
                )?
            }
            State::Append => match self.pending.last() {
                Some(PendingSegment::Attribute(attribute)) => {
                    self.append_cost(attribute.id.len())?
                }
                _ => Cost::ZERO,
            },
            State::Measure {
                source,
                index,
                bytes,
            } => {
                if let Some(next) = source.piece_len(index) {
                    bytes.checked_add(next)?;
                    index.checked_add(1)?;
                }
                Cost::ZERO
            }
            State::Render {
                source,
                bytes,
                pieces,
            } => self
                .append_cost(source.rendered_len_bound(bytes)?)?
                .checked_add(source.rendering_cost(bytes, pieces)?)?,
            State::Concatenate => {
                let len = self.path_len.to_usize();
                Cost {
                    // Concatenation visits every fragment twice and copies the complete text.
                    // Clearing the fragments retires each owned temporary before the next step.
                    work: len
                        .checked_mul(2)?
                        .checked_add(self.parts.len().checked_mul(6)?)?,
                    bytes: if len <= size_of::<CharStr>() {
                        0
                    } else {
                        // Exact CharStr storage has a refcount header and, on targets where
                        // this length cannot fit in the handle, a stored length as well.
                        len.checked_add(2 * size_of::<usize>())?
                    },
                }
            }
            _ => Cost::ZERO,
        };
        // Fixed work includes state transitions and retirement of fixed-size handles.
        Some((cost.work.checked_add(32)?, cost.bytes))
    }

    fn append_cost(&self, text_len: usize) -> Option<Cost> {
        let start = self.path_len.to_usize();
        if start >= TextSize::new(MAX_OFFSET).to_usize() {
            return None;
        }
        u32::try_from(start.checked_add(text_len)?).ok()?;
        push_cost(
            self.parts.len(),
            self.parts.capacity(),
            size_of::<MemberPathPart<'_>>(),
        )?
        .checked_add(push_cost(
            self.segments.len(),
            self.segments.capacity(),
            size_of::<SegmentInfo>(),
        )?)
    }

    pub(crate) fn advance(&mut self) -> ControlFlow<Option<MemberConstruction<'ast>>> {
        match std::mem::replace(&mut self.state, State::Finished) {
            State::Descend(expression) => match expression {
                ast::ExprRef::Name(name) => {
                    if self.pending.is_empty() {
                        return ControlFlow::Break(Some(MemberConstruction::Name(name)));
                    }
                    let text = name.id.as_str();
                    self.path_len += text.text_len();
                    self.parts.push(MemberPathPart::Borrowed(text));
                    self.state = State::Append;
                }
                // The grammar permits only immediate names as walrus targets. Parser
                // recovery can produce other targets, which do not define a place.
                ast::ExprRef::Named(named) if named.target.is_name_expr() => {
                    self.state = State::Descend(named.target.as_ref().into());
                }
                ast::ExprRef::Attribute(attribute) => {
                    self.pending
                        .push(PendingSegment::Attribute(&attribute.attr));
                    self.state = State::Descend(attribute.value.as_ref().into());
                }
                ast::ExprRef::Subscript(subscript) => {
                    self.pending
                        .push(PendingSegment::Subscript(&subscript.slice));
                    self.state = State::Descend(subscript.value.as_ref().into());
                }
                _ => return ControlFlow::Break(None),
            },
            State::Append => match self.pending.pop() {
                Some(PendingSegment::Attribute(attribute)) => {
                    self.append(
                        SegmentKind::Attribute,
                        MemberPathPart::Borrowed(attribute.id.as_str()),
                    );
                }
                Some(PendingSegment::Subscript(slice)) => {
                    let Some(source) = SubscriptSource::new(slice) else {
                        return ControlFlow::Break(None);
                    };
                    self.state = State::Measure {
                        source,
                        index: 0,
                        bytes: 0,
                    };
                }
                None => self.state = State::Concatenate,
            },
            State::Measure {
                source,
                index,
                bytes,
            } => {
                self.state = if let Some(next) = source.piece_len(index) {
                    State::Measure {
                        source,
                        index: index + 1,
                        bytes: bytes + next,
                    }
                } else {
                    State::Render {
                        source,
                        bytes,
                        pieces: index,
                    }
                };
            }
            State::Render { source, .. } => {
                let (kind, part) = source.render();
                self.append(kind, part);
            }
            State::Concatenate => {
                let path = CharStr::concat(&self.parts);
                self.parts.clear();
                return ControlFlow::Break(Some(MemberConstruction::Member(MemberExprBuilder {
                    path,
                    segments: std::mem::take(&mut self.segments),
                })));
            }
            State::Finished => return ControlFlow::Break(None),
        }
        ControlFlow::Continue(())
    }

    fn append(&mut self, kind: SegmentKind, part: MemberPathPart<'ast>) {
        let start = self.path_len;
        self.path_len += part.as_ref().text_len();
        self.parts.push(part);
        self.segments.push(SegmentInfo::new(kind, start));
        self.state = State::Append;
    }
}

#[derive(Clone, Copy)]
pub(super) enum SubscriptSource<'ast> {
    Integer {
        value: &'ast ast::Int,
        negative: bool,
    },
    Boolean(bool),
    String(&'ast ast::StringLiteralValue),
    Bytes(&'ast ast::BytesLiteralValue),
}

impl<'ast> SubscriptSource<'ast> {
    pub(super) fn new(slice: &'ast ast::Expr) -> Option<Self> {
        match slice {
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(value),
                ..
            }) => Some(Self::Integer {
                value,
                negative: false,
            }),
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: op @ (ast::UnaryOp::USub | ast::UnaryOp::UAdd),
                operand,
                ..
            }) => match operand.as_ref() {
                ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(value),
                    ..
                }) => Some(Self::Integer {
                    value,
                    negative: matches!(op, ast::UnaryOp::USub),
                }),
                _ => None,
            },
            ast::Expr::BooleanLiteral(boolean) => Some(Self::Boolean(boolean.value)),
            ast::Expr::StringLiteral(string) => Some(Self::String(&string.value)),
            ast::Expr::BytesLiteral(bytes) => Some(Self::Bytes(&bytes.value)),
            _ => None,
        }
    }

    fn piece_len(self, index: usize) -> Option<usize> {
        match self {
            Self::String(value) => value.as_slice().get(index).map(|part| part.value.len()),
            Self::Bytes(value) => value.as_slice().get(index).map(|part| part.value.len()),
            Self::Integer { .. } | Self::Boolean(_) => None,
        }
    }

    fn rendered_len_bound(self, bytes: usize) -> Option<usize> {
        match self {
            Self::Integer { value, negative } => {
                value.display_len_bound().checked_add(usize::from(negative))
            }
            Self::Boolean(_) => Some(1),
            Self::String(_) => Some(bytes),
            Self::Bytes(_) => bytes.checked_mul(3),
        }
    }

    fn rendering_cost(self, bytes: usize, pieces: usize) -> Option<Cost> {
        match self {
            Self::Integer { .. } => {
                let len = self.rendered_len_bound(bytes)?;
                Some(Cost {
                    work: len.checked_mul(2)?.checked_add(8)?,
                    // Formatting writes the sign while still inline, then the complete
                    // integer. At most one growable CharString allocation is needed.
                    bytes: if len <= size_of::<CharString>() {
                        0
                    } else {
                        len.checked_add(3 * size_of::<usize>())?
                    },
                })
            }
            Self::Boolean(_) => Some(Cost::ZERO),
            Self::String(value) if !value.is_implicit_concatenated() => Some(Cost::ZERO),
            Self::String(_) => {
                // The first to_str call collects into String and boxes it in the AST.
                // Geometric growth requests at most 4*bytes + 16 in total; boxing may
                // request another `bytes`. Include fragment visits and moving reallocations.
                Some(Cost {
                    work: bytes
                        .checked_mul(8)?
                        .checked_add(pieces.checked_mul(4)?)?
                        .checked_add(32)?,
                    bytes: bytes.checked_mul(6)?.checked_add(32)?,
                })
            }
            Self::Bytes(_) => {
                // Joining raw bytes uses at most 4*bytes + 16 of geometric allocations.
                // Lossy UTF-8 has at most 3*bytes of output, with at most 12*bytes + 16
                // of String allocations; the final CharString adds at most 3*bytes + 32.
                // These bounds also cover copying and disposal of all temporary buffers.
                Some(Cost {
                    work: bytes
                        .checked_mul(32)?
                        .checked_add(pieces.checked_mul(4)?)?
                        .checked_add(64)?,
                    bytes: bytes.checked_mul(20)?.checked_add(64)?,
                })
            }
        }
    }

    pub(super) fn render(self) -> (SegmentKind, MemberPathPart<'ast>) {
        match self {
            Self::Integer { value, negative } => (
                SegmentKind::IntSubscript,
                MemberPathPart::Owned(if negative {
                    format_char!("-{value}")
                } else {
                    format_char!("{value}")
                }),
            ),
            // In Python, True and False are equivalent to 1 and 0 for indexing.
            Self::Boolean(value) => (
                SegmentKind::IntSubscript,
                MemberPathPart::Borrowed(if value { "1" } else { "0" }),
            ),
            Self::String(value) => (
                SegmentKind::StringSubscript,
                MemberPathPart::Borrowed(value.to_str()),
            ),
            Self::Bytes(value) => {
                // A UTF-8 character can span literal parts, so concatenate before decoding.
                let bytes: Vec<u8> = value.bytes().collect();
                let text = String::from_utf8_lossy(&bytes);
                (
                    SegmentKind::BytesSubscript,
                    MemberPathPart::Owned(CharString::from(text.as_ref())),
                )
            }
        }
    }
}

struct Cost {
    work: usize,
    bytes: usize,
}

impl Cost {
    const ZERO: Self = Self { work: 0, bytes: 0 };

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            work: self.work.checked_add(other.work)?,
            bytes: self.bytes.checked_add(other.bytes)?,
        })
    }
}

fn push_cost(len: usize, capacity: usize, element_size: usize) -> Option<Cost> {
    if len < capacity {
        // Each initialized element prepays its eventual drop, including owned fragments.
        return Some(Cost { work: 2, bytes: 0 });
    }
    let next_capacity = len.checked_add(1)?.checked_next_power_of_two()?;
    Some(Cost {
        // SmallVec rounds growth up to a power of two. Prepay copying the old backing
        // and retiring the new backing, so every suspension point can release it safely.
        work: capacity
            .checked_add(next_capacity)?
            .checked_mul(element_size)?
            .checked_add(2)?,
        bytes: next_capacity.checked_mul(element_size)?,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::{Context as _, Result};
    use ruff_python_ast::name::Name;
    use ruff_python_parser::parse_expression;
    use ruff_text_size::TextRange;

    use super::*;
    use crate::member::Segments;
    use crate::place::{PlaceExpr, PlaceExprCursor};

    fn admitted_place(expression: &ast::Expr) -> Result<Option<PlaceExpr>> {
        let mut cursor = PlaceExprCursor::new(expression);
        loop {
            let step = cursor.prepare();
            step.cost()
                .context("place operation must have a finite cost")?;
            if let ControlFlow::Break(place) = step.advance() {
                return Ok(place);
            }
        }
    }

    #[test]
    fn mixed_literals_preserve_text_and_segments() -> Result<()> {
        let parsed = parse_expression(
            r#"(root := ignored).field[+1][-0][True][False]["a" "b"][b"\xc3" b"\xa9"][b"\xff"][0x1_00000000000000000]"#,
        )?;
        let place = admitted_place(parsed.expr())?.context("valid member expression")?;
        let PlaceExpr::Member(member) = &place else {
            anyhow::bail!("expected a member expression");
        };
        let expression = member.expression();
        assert_eq!(expression.symbol_name(), "root");
        assert!(matches!(expression.segments, Segments::Heap(_)));
        assert_eq!(
            expression
                .segments()
                .map(|segment| (segment.kind, segment.text))
                .collect::<Vec<_>>(),
            [
                (SegmentKind::Attribute, "field"),
                (SegmentKind::IntSubscript, "1"),
                (SegmentKind::IntSubscript, "-0"),
                (SegmentKind::IntSubscript, "1"),
                (SegmentKind::IntSubscript, "0"),
                (SegmentKind::StringSubscript, "ab"),
                (SegmentKind::BytesSubscript, "é"),
                (SegmentKind::BytesSubscript, "�"),
                (SegmentKind::IntSubscript, "0x1_00000000000000000"),
            ],
        );
        assert_eq!(PlaceExpr::try_from_expr(parsed.expr()), Some(place));
        Ok(())
    }

    #[test]
    fn named_targets_and_invalid_paths() -> Result<()> {
        let valid = parse_expression("(a_long_symbol_name := f())")?;
        let place = admitted_place(valid.expr())?.context("valid named expression")?;
        let PlaceExpr::Symbol(symbol) = place else {
            anyhow::bail!("a named expression must yield its target symbol");
        };
        assert_eq!(symbol.name(), "a_long_symbol_name");

        let target = parse_expression("x.attribute")?;
        let malformed = ast::Expr::Named(ast::ExprNamed {
            node_index: ast::AtomicNodeIndex::NONE,
            range: TextRange::default(),
            target: Box::new(target.expr().clone()),
            value: Box::new(valid.expr().clone()),
        });
        assert!(admitted_place(&malformed)?.is_none());
        assert!(PlaceExpr::try_from_expr(&malformed).is_none());

        for expression in [
            "f().x",
            "x[-True]",
            "x[--1]",
            "x[1:]",
            "x[other]",
            "(x + y).z",
            "x[1.5]",
            "x[f'a']",
            "x[~1]",
        ] {
            let parsed = parse_expression(expression)?;
            assert!(admitted_place(parsed.expr())?.is_none(), "{expression}");
        }
        Ok(())
    }

    /// Own the test's deep receiver chain without recursively dropping its boxed AST.
    struct AttributeSpine(Option<ast::Expr>);

    impl AttributeSpine {
        fn new(depth: usize) -> Self {
            let mut expression = ast::Expr::Name(ast::ExprName {
                node_index: ast::AtomicNodeIndex::NONE,
                range: TextRange::default(),
                id: Name::new_static("x"),
                ctx: ast::ExprContext::Load,
            });
            for _ in 0..depth {
                expression = ast::Expr::Attribute(ast::ExprAttribute {
                    node_index: ast::AtomicNodeIndex::NONE,
                    range: TextRange::default(),
                    value: Box::new(expression),
                    attr: ast::Identifier::new("a", TextRange::default()),
                    ctx: ast::ExprContext::Load,
                });
            }
            Self(Some(expression))
        }
    }

    impl Drop for AttributeSpine {
        fn drop(&mut self) {
            let mut current = self.0.take();
            while let Some(ast::Expr::Attribute(attribute)) = current {
                current = Some(*attribute.value);
            }
        }
    }

    #[test]
    fn deep_receiver_construction_and_interruption() -> Result<()> {
        let depth = 50_000;
        let spine = AttributeSpine::new(depth);
        let expression = spine.0.as_ref().context("receiver chain")?;

        let mut interrupted = PlaceExprCursor::new(expression);
        for _ in 0..32 {
            let step = interrupted.prepare();
            step.cost().context("finite descent operation")?;
            assert!(step.advance().is_continue());
        }
        drop(interrupted);

        let place = admitted_place(expression)?.context("valid deep member expression")?;
        let PlaceExpr::Member(member) = &place else {
            anyhow::bail!("expected a member expression");
        };
        assert_eq!(member.expression().num_segments(), depth);
        assert_eq!(member.expression().path.len(), depth + 1);
        assert_eq!(PlaceExpr::try_from_expr(expression), Some(place));
        Ok(())
    }

    #[test]
    fn offset_and_literal_size_overflow_refuse_before_execution() -> Result<()> {
        let parsed = parse_expression("x.attribute")?;
        let bytes = parse_expression("b'bytes'")?;
        let ast::Expr::Attribute(attribute) = parsed.expr() else {
            anyhow::bail!("expected an attribute expression");
        };
        let mut cursor = MemberExprCursor::new(parsed.expr().into());
        cursor.path_len = TextSize::new(MAX_OFFSET);
        cursor
            .pending
            .push(PendingSegment::Attribute(&attribute.attr));
        cursor.state = State::Append;
        assert!(cursor.cost().is_none());
        assert_eq!(cursor.pending.len(), 1);

        let source = SubscriptSource::new(bytes.expr()).context("bytes subscript")?;
        cursor.state = State::Render {
            source,
            bytes: usize::MAX,
            pieces: 1,
        };
        cursor.path_len = TextSize::new(1);
        assert!(cursor.cost().is_none());
        Ok(())
    }
}
