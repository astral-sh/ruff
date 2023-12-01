use std::fmt::{Debug, Formatter};

use ruff_text_size::{Ranged, TextRange};

/// The source code of a document that gets formatted
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct SourceCode<'a> {
    text: &'a str,
}

impl<'a> SourceCode<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn slice(self, range: TextRange) -> SourceCodeSlice {
        assert!(
            usize::from(range.end()) <= self.text.len(),
            "Range end {:?} out of bounds {}.",
            range.end(),
            self.text.len()
        );

        assert!(
            self.text.is_char_boundary(usize::from(range.start())),
            "The range start position {:?} is not a char boundary.",
            range.start()
        );

        assert!(
            self.text.is_char_boundary(usize::from(range.end())),
            "The range end position {:?} is not a char boundary.",
            range.end()
        );

        SourceCodeSlice {
            range,
            #[cfg(debug_assertions)]
            text: String::from(&self.text[range]).into_boxed_str(),
        }
    }

    pub fn as_str(&self) -> &'a str {
        self.text
    }
}

impl Debug for SourceCode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceCode").field(&self.text).finish()
    }
}

/// A slice into the source text of a document.
///
/// It only stores the range in production builds for a more compact representation, but it
/// keeps the original text in debug builds for better developer experience.
#[derive(Clone, Eq, PartialEq)]
pub struct SourceCodeSlice {
    range: TextRange,
    #[cfg(debug_assertions)]
    text: Box<str>,
}

impl SourceCodeSlice {
    /// Returns the slice's text.
    pub fn text<'a>(&self, code: SourceCode<'a>) -> &'a str {
        assert!(usize::from(self.range.end()) <= code.text.len(), "The range of this slice is out of bounds. Did you provide the correct source code for this slice?");
        &code.text[self.range]
    }
}

impl Ranged for SourceCodeSlice {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Debug for SourceCodeSlice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut tuple = f.debug_tuple("SourceCodeSlice");

        #[cfg(debug_assertions)]
        tuple.field(&self.text);

        tuple.field(&self.range).finish()
    }
}
