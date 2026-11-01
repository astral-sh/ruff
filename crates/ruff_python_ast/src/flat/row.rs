//! A row's kind and inline fields share sixteen bytes. All writes go through a named
//! field setter so they cannot overwrite the kind in the last byte.

use std::fmt;
use std::mem::offset_of;

use ruff_text_size::{Ranged, TextRange};

use crate::NodeKind;

use super::children::{Children, Field, Pending, StatementLists};
use super::{AstNodeId, List, NodeList, SyntaxNode, split_words};

#[derive(Copy, Clone)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
#[repr(C)]
pub(super) struct Row {
    fields: [u8; 15],
    kind: NodeKind,
    span: TextRange,
}

impl Row {
    pub(super) const fn new(kind: NodeKind, span: TextRange) -> Self {
        Self {
            fields: [0; 15],
            kind,
            span,
        }
    }

    #[inline]
    pub(super) const fn kind(&self) -> NodeKind {
        self.kind
    }

    pub(super) fn set_range(&mut self, span: TextRange) {
        self.span = span;
    }

    /// Reads a field shared by several layouts in the generated descendant or string-part walk.
    #[inline]
    fn inline_word<const OFFSET: usize, const WIDTH: usize>(&self) -> u64 {
        const { assert!(WIDTH <= 8 && OFFSET + WIDTH <= offset_of!(Row, kind)) };
        let mut word = [0; 8];
        word[..WIDTH].copy_from_slice(&self.fields[OFFSET..OFFSET + WIDTH]);
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

// Offsets are generated from the same schema as the builder and borrowed nodes.
macro_rules! node_fields {
    ($($kind:ident {
        $($ty:ident at $offset:literal => $get:ident, $set:ident;)+
    })+) => {
        $(
            impl Row {
                $(
                    #[inline]
                    pub(super) fn $get(&self) -> $ty {
                        const { assert!($offset + size_of::<$ty>() <= offset_of!(Row, kind)) };
                        debug_assert_eq!(self.kind(), NodeKind::$kind);
                        let mut bytes = [0; size_of::<$ty>()];
                        bytes.copy_from_slice(&self.fields[$offset..$offset + size_of::<$ty>()]);
                        <$ty>::from_le_bytes(bytes)
                    }

                    #[inline]
                    pub(super) fn $set(&mut self, value: $ty) {
                        debug_assert_eq!(self.kind(), NodeKind::$kind);
                        self.fields[$offset..$offset + size_of::<$ty>()].copy_from_slice(&value.to_le_bytes());
                    }
                )+
            }
        )+
    };
}

include!("row/generated.rs");

const _: () = {
    assert!(size_of::<NodeKind>() == 1 && offset_of!(Row, kind) == 15);
    assert!(offset_of!(Row, span) == 16 && size_of::<Row>() == 24 && align_of::<Row>() == 4);
};

#[cfg(test)]
mod tests {
    use super::{NodeKind, Row, TextRange};

    #[test]
    fn updating_a_function_flag_preserves_the_name_and_kind() {
        let mut row = Row::new(NodeKind::StmtFunctionDef, TextRange::default());
        row.set_stmt_function_def_name(42);
        row.set_stmt_function_def_is_async(1);

        assert_eq!(row.kind(), NodeKind::StmtFunctionDef);
        assert_eq!(row.stmt_function_def_is_async(), 1);
        assert_eq!(row.stmt_function_def_name(), 42);
    }
}
