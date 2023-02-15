use std::{
    borrow::Borrow,
    fmt,
    mem::{self, ManuallyDrop},
    ops, ptr,
};

use countme::Count;

use crate::green::trivia::GreenTrivia;
use crate::{
    arc::{Arc, HeaderSlice, ThinArc},
    green::RawSyntaxKind,
    TextSize,
};

#[derive(PartialEq, Eq, Hash)]
struct GreenTokenHead {
    kind: RawSyntaxKind,
    leading: GreenTrivia,
    trailing: GreenTrivia,
    _c: Count<GreenToken>,
}

pub(crate) fn has_live() -> bool {
    countme::get::<GreenToken>().live > 0
}

type Repr = HeaderSlice<GreenTokenHead, [u8]>;
type ReprThin = HeaderSlice<GreenTokenHead, [u8; 0]>;
#[repr(transparent)]
pub(crate) struct GreenTokenData {
    data: ReprThin,
}

impl PartialEq for GreenTokenData {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind() && self.text() == other.text()
    }
}

/// Leaf node in the immutable tree.
#[derive(PartialEq, Eq, Hash, Clone)]
#[repr(transparent)]
pub(crate) struct GreenToken {
    ptr: ThinArc<GreenTokenHead, u8>,
}

impl ToOwned for GreenTokenData {
    type Owned = GreenToken;

    #[inline]
    fn to_owned(&self) -> GreenToken {
        unsafe {
            let green = GreenToken::from_raw(ptr::NonNull::from(self));
            let green = ManuallyDrop::new(green);
            GreenToken::clone(&green)
        }
    }
}

impl Borrow<GreenTokenData> for GreenToken {
    #[inline]
    fn borrow(&self) -> &GreenTokenData {
        self
    }
}

impl fmt::Debug for GreenTokenData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GreenToken")
            .field("kind", &self.kind())
            .field("text", &self.text())
            .field("leading", &self.leading_trivia())
            .field("trailing", &self.trailing_trivia())
            .finish()
    }
}

impl fmt::Debug for GreenToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data: &GreenTokenData = self;
        fmt::Debug::fmt(data, f)
    }
}

impl fmt::Display for GreenToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data: &GreenTokenData = self;
        fmt::Display::fmt(data, f)
    }
}

impl fmt::Display for GreenTokenData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text())
    }
}

impl GreenTokenData {
    /// Kind of this Token.
    #[inline]
    pub fn kind(&self) -> RawSyntaxKind {
        self.data.header.kind
    }

    /// Whole text of this Token, including all trivia.
    #[inline]
    pub fn text(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.data.slice()) }
    }

    pub(crate) fn leading_trailing_total_len(&self) -> (TextSize, TextSize, TextSize) {
        let leading_len = self.data.header.leading.text_len();
        let trailing_len = self.data.header.trailing.text_len();
        let total_len = self.data.slice().len() as u32;
        (leading_len, trailing_len, total_len.into())
    }

    /// Text of this Token, excluding all trivia.
    #[inline]
    pub fn text_trimmed(&self) -> &str {
        let (leading_len, trailing_len, total_len) = self.leading_trailing_total_len();

        let start: usize = leading_len.into();
        let end: usize = (total_len - trailing_len).into();
        let text = unsafe { std::str::from_utf8_unchecked(self.data.slice()) };
        &text[start..end]
    }

    /// Returns the length of the text covered by this token.
    #[inline]
    pub fn text_len(&self) -> TextSize {
        TextSize::of(self.text())
    }

    #[inline]
    pub fn leading_trivia(&self) -> &GreenTrivia {
        &self.data.header.leading
    }

    #[inline]
    pub fn trailing_trivia(&self) -> &GreenTrivia {
        &self.data.header.trailing
    }
}

impl GreenToken {
    #[inline]
    #[cfg(test)]
    pub fn new(kind: RawSyntaxKind, text: &str) -> GreenToken {
        let leading = GreenTrivia::empty();
        let trailing = leading.clone();

        Self::with_trivia(kind, text, leading, trailing)
    }

    #[inline]
    pub fn with_trivia(
        kind: RawSyntaxKind,
        text: &str,
        leading: GreenTrivia,
        trailing: GreenTrivia,
    ) -> GreenToken {
        let head = GreenTokenHead {
            kind,
            leading,
            trailing,
            _c: Count::new(),
        };
        let ptr = ThinArc::from_header_and_iter(head, text.bytes());
        GreenToken { ptr }
    }

    #[inline]
    pub(crate) unsafe fn from_raw(ptr: ptr::NonNull<GreenTokenData>) -> GreenToken {
        let arc = Arc::from_raw(&ptr.as_ref().data as *const ReprThin);
        let arc = mem::transmute::<Arc<ReprThin>, ThinArc<GreenTokenHead, u8>>(arc);
        GreenToken { ptr: arc }
    }
}

impl ops::Deref for GreenToken {
    type Target = GreenTokenData;

    #[inline]
    fn deref(&self) -> &GreenTokenData {
        unsafe {
            let repr: &Repr = &self.ptr;
            let repr: &ReprThin = &*(repr as *const Repr as *const ReprThin);
            mem::transmute::<&ReprThin, &GreenTokenData>(repr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::*;

    #[test]
    fn green_token_text_and_len() {
        let t = GreenToken::with_trivia(
            RawSyntaxKind(0),
            "\n\t let \t\t",
            GreenTrivia::whitespace(3),
            GreenTrivia::whitespace(3),
        );

        assert_eq!("\n\t let \t\t", t.text());
        assert_eq!(TextSize::from(9), t.text_len());

        assert_eq!("let", t.text_trimmed());

        assert_eq!("\n\t let \t\t", format!("{}", t));
    }

    #[test]
    fn empty_text_len() {
        assert_eq!(TextSize::from(0), GreenTrivia::empty().text_len());
    }

    #[quickcheck]
    fn whitespace_and_comments_text_len(len: u32) {
        let len = TextSize::from(len);
        assert_eq!(len, GreenTrivia::whitespace(len).text_len());
        assert_eq!(len, GreenTrivia::single_line_comment(len).text_len());
    }

    #[test]
    fn sizes() {
        assert_eq!(24, std::mem::size_of::<GreenTokenHead>());
        assert_eq!(8, std::mem::size_of::<GreenToken>());
    }
}
