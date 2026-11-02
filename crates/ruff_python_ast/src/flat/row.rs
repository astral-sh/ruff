//! A row's kind and inline fields share sixteen bytes. All writes go through a named
//! field setter so they cannot overwrite the kind in the last byte.

use std::fmt;
use std::mem::offset_of;

use ruff_text_size::{Ranged, TextRange};

use crate::NodeKind;

use super::children::{Children, Field, Pending, StatementLists};
use super::{AstNodeId, List, NodeList, SyntaxNode, split_words};

#[derive(Copy, Clone)]
#[repr(C)]
struct TaggedBytes {
    fields: [u8; 15],
    kind: NodeKind,
}

#[derive(Copy, Clone)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
#[repr(C)]
pub(super) struct Row {
    payload: Payload,
    span: TextRange,
}

impl Row {
    pub(super) const fn new(kind: NodeKind, span: TextRange) -> Self {
        Self {
            payload: Payload {
                raw: TaggedBytes {
                    fields: [0; 15],
                    kind,
                },
            },
            span,
        }
    }

    #[inline]
    pub(super) const fn kind(&self) -> NodeKind {
        // SAFETY: Construction initializes the kind. The generated setters write only
        // individual integer fields whose ends are checked to precede the kind.
        #[expect(unsafe_code)]
        unsafe {
            self.payload.raw.kind
        }
    }

    pub(super) fn set_range(&mut self, span: TextRange) {
        self.span = span;
    }

    /// Borrows adjacent child handles in source order. An absent optional endpoint
    /// is stored as zero. Inline integers use little-endian byte order.
    #[inline]
    fn inline_children<
        const START: usize,
        const LEN: usize,
        const OPTIONAL_FIRST: bool,
        const OPTIONAL_LAST: bool,
    >(
        &self,
    ) -> &[u32] {
        const { assert!(START + LEN <= 3) };
        // SAFETY: Row construction initializes every byte of the payload. Each union
        // setter writes one integer field, and every bit pattern is valid for u32.
        #[expect(unsafe_code)]
        let words = unsafe { &self.payload.words };
        let words = &words[START..START + LEN];
        let words = if OPTIONAL_FIRST {
            words.strip_prefix(&[0]).unwrap_or(words)
        } else {
            words
        };
        if OPTIONAL_LAST {
            words.strip_suffix(&[0]).unwrap_or(words)
        } else {
            words
        }
    }

    /// Reads a field shared by several layouts in the generated descendant or string-part walk.
    #[inline]
    fn inline_word<const OFFSET: usize, const WIDTH: usize>(&self) -> u64 {
        const { assert!(WIDTH <= 8 && OFFSET + WIDTH <= offset_of!(TaggedBytes, kind)) };
        // SAFETY: Construction initializes every byte and setters only write integers.
        // No setter replaces a whole union member, which could copy uninitialized padding.
        #[expect(unsafe_code)]
        let bytes = unsafe { &self.payload.raw.fields };
        let mut word = [0; 8];
        word[..WIDTH].copy_from_slice(&bytes[OFFSET..OFFSET + WIDTH]);
        u64::from_le_bytes(word)
    }
}

impl Ranged for Row {
    fn range(&self) -> TextRange {
        self.span
    }
}

impl fmt::Debug for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Row")
            .field("kind", &self.kind())
            .field("span", &self.range())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for Payload {}

// Generate each union member, its accessors, and its layout checks from one field list.
// The members stay private even to the rest of the AST implementation.
macro_rules! node_payloads {
    ($($kind:ident($arm:ident) {
        $($( [$pad:ident; $gap:literal] )?
          $field:ident: $ty:ident at $offset:literal => $get:ident, $set:ident;)+
    })+) => {
        #[derive(Clone, Copy)]
        #[repr(C, align(8))]
        union Payload {
            raw: TaggedBytes,
            words: [u32; 3],
            $($arm: $kind,)+
        }

        $(
            #[derive(Clone, Copy)]
            #[repr(C)]
            struct $kind {
                $($( $pad: [u8; $gap], )? $field: $ty,)+
            }

            const _: () = {
                $(assert!(offset_of!($kind, $field) == $offset);
                  assert!($offset + size_of::<$ty>() <= offset_of!(TaggedBytes, kind));)+
            };

            impl Row {
                $(
                    #[inline]
                    pub(super) fn $get(&self) -> $ty {
                        debug_assert_eq!(self.kind(), NodeKind::$kind);
                        // SAFETY: Every byte is initialized and all bit patterns are valid
                        // for the unsigned integer stored at this aligned field offset.
                        #[expect(unsafe_code)]
                        $ty::from_le(unsafe { self.payload.$arm.$field })
                    }

                    #[inline]
                    pub(super) fn $set(&mut self, value: $ty) {
                        debug_assert_eq!(self.kind(), NodeKind::$kind);
                        self.payload.$arm.$field = value.to_le();
                    }
                )+
            }
        )+
    };
}

include!("row/generated.rs");

const _: () = {
    assert!(size_of::<NodeKind>() == 1 && offset_of!(TaggedBytes, kind) == 15);
    assert!(size_of::<TaggedBytes>() == 16 && size_of::<Payload>() == 16);
    assert!(offset_of!(Row, span) == 16 && size_of::<Row>() == 24 && align_of::<Row>() == 8);
};

#[cfg(test)]
mod tests {
    use super::{NodeKind, Row, TextRange};

    #[test]
    fn updating_a_function_body_preserves_the_adjacent_flag_and_kind() {
        let mut row = Row::new(NodeKind::StmtFunctionDef, TextRange::default());
        row.set_stmt_function_def_name(42);
        row.set_stmt_function_def_is_async(1);
        row.set_stmt_function_def_body_row_count(u16::MAX);

        assert_eq!(row.kind(), NodeKind::StmtFunctionDef);
        assert_eq!(row.stmt_function_def_is_async(), 1);
        assert_eq!(row.stmt_function_def_name(), 42);
        assert_eq!(row.stmt_function_def_body_row_count(), u16::MAX);
    }
}
