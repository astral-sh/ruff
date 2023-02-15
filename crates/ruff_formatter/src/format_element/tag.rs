use crate::format_element::PrintMode;
use crate::{GroupId, TextSize};
#[cfg(debug_assertions)]
use std::any::type_name;
use std::any::TypeId;
use std::cell::Cell;
use std::num::NonZeroU8;

/// A Tag marking the start and end of some content to which some special formatting should be applied.
///
/// Tags always come in pairs of a start and an end tag and the styling defined by this tag
/// will be applied to all elements in between the start/end tags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tag {
    /// Indents the content one level deeper, see [crate::builders::indent] for documentation and examples.
    StartIndent,
    EndIndent,

    /// Variant of [TagKind::Indent] that indents content by a number of spaces. For example, `Align(2)`
    /// indents any content following a line break by an additional two spaces.
    ///
    /// Nesting (Aligns)[TagKind::Align] has the effect that all except the most inner align are handled as (Indent)[TagKind::Indent].
    StartAlign(Align),
    EndAlign,

    /// Reduces the indention of the specified content either by one level or to the root, depending on the mode.
    /// Reverse operation of `Indent` and can be used to *undo* an `Align` for nested content.
    StartDedent(DedentMode),
    EndDedent,

    /// Creates a logical group where its content is either consistently printed:
    /// * on a single line: Omitting `LineMode::Soft` line breaks and printing spaces for `LineMode::SoftOrSpace`
    /// * on multiple lines: Printing all line breaks
    ///
    /// See [crate::builders::group] for documentation and examples.
    StartGroup(Group),
    EndGroup,

    /// Allows to specify content that gets printed depending on whatever the enclosing group
    /// is printed on a single line or multiple lines. See [crate::builders::if_group_breaks] for examples.
    StartConditionalContent(Condition),
    EndConditionalContent,

    /// Optimized version of [Tag::StartConditionalContent] for the case where some content
    /// should be indented if the specified group breaks.
    StartIndentIfGroupBreaks(GroupId),
    EndIndentIfGroupBreaks,

    /// Concatenates multiple elements together with a given separator printed in either
    /// flat or expanded mode to fill the print width. Expect that the content is a list of alternating
    /// [element, separator] See [crate::Formatter::fill].
    StartFill,
    EndFill,

    /// Entry inside of a [Tag::StartFill]
    StartEntry,
    EndEntry,

    /// Delay the printing of its content until the next line break
    StartLineSuffix,
    EndLineSuffix,

    /// A token that tracks tokens/nodes that are printed as verbatim.
    StartVerbatim(VerbatimKind),
    EndVerbatim,

    /// Special semantic element marking the content with a label.
    /// This does not directly influence how the content will be printed.
    ///
    /// See [crate::builders::labelled] for documentation.
    StartLabelled(LabelId),
    EndLabelled,
}

impl Tag {
    /// Returns `true` if `self` is any start tag.
    pub const fn is_start(&self) -> bool {
        matches!(
            self,
            Tag::StartIndent
                | Tag::StartAlign(_)
                | Tag::StartDedent(_)
                | Tag::StartGroup { .. }
                | Tag::StartConditionalContent(_)
                | Tag::StartIndentIfGroupBreaks(_)
                | Tag::StartFill
                | Tag::StartEntry
                | Tag::StartLineSuffix
                | Tag::StartVerbatim(_)
                | Tag::StartLabelled(_)
        )
    }

    /// Returns `true` if `self` is any end tag.
    pub const fn is_end(&self) -> bool {
        !self.is_start()
    }

    pub const fn kind(&self) -> TagKind {
        use Tag::*;

        match self {
            StartIndent | EndIndent => TagKind::Indent,
            StartAlign(_) | EndAlign => TagKind::Align,
            StartDedent(_) | EndDedent => TagKind::Dedent,
            StartGroup(_) | EndGroup => TagKind::Group,
            StartConditionalContent(_) | EndConditionalContent => TagKind::ConditionalContent,
            StartIndentIfGroupBreaks(_) | EndIndentIfGroupBreaks => TagKind::IndentIfGroupBreaks,
            StartFill | EndFill => TagKind::Fill,
            StartEntry | EndEntry => TagKind::Entry,
            StartLineSuffix | EndLineSuffix => TagKind::LineSuffix,
            StartVerbatim(_) | EndVerbatim => TagKind::Verbatim,
            StartLabelled(_) | EndLabelled => TagKind::Labelled,
        }
    }
}

/// The kind of a [Tag].
///
/// Each start end tag pair has its own [tag kind](TagKind).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TagKind {
    Indent,
    Align,
    Dedent,
    Group,
    ConditionalContent,
    IndentIfGroupBreaks,
    Fill,
    Entry,
    LineSuffix,
    Verbatim,
    Labelled,
}

#[derive(Debug, Copy, Default, Clone, Eq, PartialEq)]
pub enum GroupMode {
    /// Print group in flat mode.
    #[default]
    Flat,

    /// The group should be printed in expanded mode
    Expand,

    /// Expand mode has been propagated from an enclosing group to this group.
    Propagated,
}

impl GroupMode {
    pub const fn is_flat(&self) -> bool {
        matches!(self, GroupMode::Flat)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Group {
    id: Option<GroupId>,
    mode: Cell<GroupMode>,
}

impl Group {
    pub fn new() -> Self {
        Self {
            id: None,
            mode: Cell::new(GroupMode::Flat),
        }
    }

    pub fn with_id(mut self, id: Option<GroupId>) -> Self {
        self.id = id;
        self
    }

    pub fn with_mode(mut self, mode: GroupMode) -> Self {
        self.mode = Cell::new(mode);
        self
    }

    pub fn mode(&self) -> GroupMode {
        self.mode.get()
    }

    pub fn propagate_expand(&self) {
        if self.mode.get() == GroupMode::Flat {
            self.mode.set(GroupMode::Propagated)
        }
    }

    pub fn id(&self) -> Option<GroupId> {
        self.id
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DedentMode {
    /// Reduces the indent by a level (if the current indent is > 0)
    Level,

    /// Reduces the indent to the root
    Root,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Condition {
    /// * Flat -> Omitted if the enclosing group is a multiline group, printed for groups fitting on a single line
    /// * Multiline -> Omitted if the enclosing group fits on a single line, printed if the group breaks over multiple lines.
    pub(crate) mode: PrintMode,

    /// The id of the group for which it should check if it breaks or not. The group must appear in the document
    /// before the conditional group (but doesn't have to be in the ancestor chain).
    pub(crate) group_id: Option<GroupId>,
}

impl Condition {
    pub fn new(mode: PrintMode) -> Self {
        Self {
            mode,
            group_id: None,
        }
    }

    pub fn with_group_id(mut self, id: Option<GroupId>) -> Self {
        self.group_id = id;
        self
    }

    pub fn mode(&self) -> PrintMode {
        self.mode
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Align(pub(crate) NonZeroU8);

impl Align {
    pub fn count(&self) -> NonZeroU8 {
        self.0
    }
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub struct LabelId {
    id: TypeId,
    #[cfg(debug_assertions)]
    label: &'static str,
}

#[cfg(debug_assertions)]
impl std::fmt::Debug for LabelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

#[cfg(not(debug_assertions))]
impl std::fmt::Debug for LabelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::write!(f, "#{:?}", self.id)
    }
}

impl LabelId {
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            #[cfg(debug_assertions)]
            label: type_name::<T>(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum VerbatimKind {
    Bogus,
    Suppressed,
    Verbatim {
        /// the length of the formatted node
        length: TextSize,
    },
}

impl VerbatimKind {
    pub const fn is_bogus(&self) -> bool {
        matches!(self, VerbatimKind::Bogus)
    }
}
