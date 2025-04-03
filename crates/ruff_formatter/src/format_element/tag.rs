use crate::format_element::PrintMode;
use crate::{GroupId, TextSize};
use std::cell::Cell;
use std::num::NonZeroU8;

/// A Tag marking the start and end of some content to which some special formatting should be applied.
///
/// Tags always come in pairs of a start and an end tag and the styling defined by this tag
/// will be applied to all elements in between the start/end tags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tag {
    /// Indents the content one level deeper, see [`crate::builders::indent`] for documentation and examples.
    StartIndent,
    EndIndent,

    /// Variant of [`TagKind::Indent`] that indents content by a number of spaces. For example, `Align(2)`
    /// indents any content following a line break by an additional two spaces.
    ///
    /// Nesting (Aligns)[`TagKind::Align`] has the effect that all except the most inner align are handled as (Indent)[`TagKind::Indent`].
    StartAlign(Align),
    EndAlign,

    /// Reduces the indentation of the specified content either by one level or to the root, depending on the mode.
    /// Reverse operation of `Indent` and can be used to *undo* an `Align` for nested content.
    StartDedent(DedentMode),
    EndDedent,

    /// Creates a logical group where its content is either consistently printed:
    /// - on a single line: Omitting `LineMode::Soft` line breaks and printing spaces for `LineMode::SoftOrSpace`
    /// - on multiple lines: Printing all line breaks
    ///
    /// See [`crate::builders::group`] for documentation and examples.
    StartGroup(Group),
    EndGroup,

    /// Creates a logical group similar to [`Tag::StartGroup`] but only if the condition is met.
    /// This is an optimized representation for (assuming the content should only be grouped if another group fits):
    ///
    /// ```text
    /// if_group_breaks(content, other_group_id),
    /// if_group_fits_on_line(group(&content), other_group_id)
    /// ```
    StartConditionalGroup(ConditionalGroup),
    EndConditionalGroup,

    /// Allows to specify content that gets printed depending on whatever the enclosing group
    /// is printed on a single line or multiple lines. See [`crate::builders::if_group_breaks`] for examples.
    StartConditionalContent(Condition),
    EndConditionalContent,

    /// Optimized version of [`Tag::StartConditionalContent`] for the case where some content
    /// should be indented if the specified group breaks.
    StartIndentIfGroupBreaks(GroupId),
    EndIndentIfGroupBreaks,

    /// Concatenates multiple elements together with a given separator printed in either
    /// flat or expanded mode to fill the print width. Expect that the content is a list of alternating
    /// [element, separator] See [`crate::Formatter::fill`].
    StartFill,
    EndFill,

    /// Entry inside of a [`Tag::StartFill`]
    StartEntry,
    EndEntry,

    /// Delay the printing of its content until the next line break. Using reserved width will include
    /// the associated line suffix during measurement.
    StartLineSuffix {
        reserved_width: u32,
    },
    EndLineSuffix,

    /// A token that tracks tokens/nodes that are printed as verbatim.
    StartVerbatim(VerbatimKind),
    EndVerbatim,

    /// Special semantic element marking the content with a label.
    /// This does not directly influence how the content will be printed.
    ///
    /// See [`crate::builders::labelled`] for documentation.
    StartLabelled(LabelId),
    EndLabelled,

    StartFitsExpanded(FitsExpanded),
    EndFitsExpanded,

    /// Marks the start and end of a best-fitting variant.
    StartBestFittingEntry,
    EndBestFittingEntry,

    /// Parenthesizes the content but only if adding the parentheses and indenting the content
    /// makes the content fit in the configured line width.
    ///
    /// See [`crate::builders::best_fit_parenthesize`] for an in-depth explanation.
    StartBestFitParenthesize {
        id: Option<GroupId>,
    },
    EndBestFitParenthesize,
}

impl Tag {
    /// Returns `true` if `self` is any start tag.
    pub const fn is_start(&self) -> bool {
        matches!(
            self,
            Tag::StartIndent
                | Tag::StartAlign(_)
                | Tag::StartDedent(_)
                | Tag::StartGroup(_)
                | Tag::StartConditionalGroup(_)
                | Tag::StartConditionalContent(_)
                | Tag::StartIndentIfGroupBreaks(_)
                | Tag::StartFill
                | Tag::StartEntry
                | Tag::StartLineSuffix { .. }
                | Tag::StartVerbatim(_)
                | Tag::StartLabelled(_)
                | Tag::StartFitsExpanded(_)
                | Tag::StartBestFittingEntry
                | Tag::StartBestFitParenthesize { .. }
        )
    }

    /// Returns `true` if `self` is any end tag.
    pub const fn is_end(&self) -> bool {
        !self.is_start()
    }

    pub const fn kind(&self) -> TagKind {
        #[allow(clippy::enum_glob_use)]
        use Tag::*;

        match self {
            StartIndent | EndIndent => TagKind::Indent,
            StartAlign(_) | EndAlign => TagKind::Align,
            StartDedent(_) | EndDedent => TagKind::Dedent,
            StartGroup(_) | EndGroup => TagKind::Group,
            StartConditionalGroup(_) | EndConditionalGroup => TagKind::ConditionalGroup,
            StartConditionalContent(_) | EndConditionalContent => TagKind::ConditionalContent,
            StartIndentIfGroupBreaks(_) | EndIndentIfGroupBreaks => TagKind::IndentIfGroupBreaks,
            StartFill | EndFill => TagKind::Fill,
            StartEntry | EndEntry => TagKind::Entry,
            StartLineSuffix { reserved_width: _ } | EndLineSuffix => TagKind::LineSuffix,
            StartVerbatim(_) | EndVerbatim => TagKind::Verbatim,
            StartLabelled(_) | EndLabelled => TagKind::Labelled,
            StartFitsExpanded { .. } | EndFitsExpanded => TagKind::FitsExpanded,
            StartBestFittingEntry | EndBestFittingEntry => TagKind::BestFittingEntry,
            StartBestFitParenthesize { .. } | EndBestFitParenthesize => {
                TagKind::BestFitParenthesize
            }
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
    ConditionalGroup,
    ConditionalContent,
    IndentIfGroupBreaks,
    Fill,
    Entry,
    LineSuffix,
    Verbatim,
    Labelled,
    FitsExpanded,
    BestFittingEntry,
    BestFitParenthesize,
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
pub struct FitsExpanded {
    pub(crate) condition: Option<Condition>,
    pub(crate) propagate_expand: Cell<bool>,
}

impl FitsExpanded {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_condition(mut self, condition: Option<Condition>) -> Self {
        self.condition = condition;
        self
    }

    pub fn propagate_expand(&self) {
        self.propagate_expand.set(true);
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

    #[must_use]
    pub fn with_id(mut self, id: Option<GroupId>) -> Self {
        self.id = id;
        self
    }

    #[must_use]
    pub fn with_mode(mut self, mode: GroupMode) -> Self {
        self.mode = Cell::new(mode);
        self
    }

    pub fn mode(&self) -> GroupMode {
        self.mode.get()
    }

    pub fn propagate_expand(&self) {
        if self.mode.get() == GroupMode::Flat {
            self.mode.set(GroupMode::Propagated);
        }
    }

    pub fn id(&self) -> Option<GroupId> {
        self.id
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConditionalGroup {
    mode: Cell<GroupMode>,
    condition: Condition,
}

impl ConditionalGroup {
    pub fn new(condition: Condition) -> Self {
        Self {
            mode: Cell::new(GroupMode::Flat),
            condition,
        }
    }

    pub fn condition(&self) -> Condition {
        self.condition
    }

    pub fn propagate_expand(&self) {
        self.mode.set(GroupMode::Propagated);
    }

    pub fn mode(&self) -> GroupMode {
        self.mode.get()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DedentMode {
    /// Reduces the indent by a level (if the current indent is > 0)
    Level,

    /// Reduces the indent to the root
    Root,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Condition {
    /// - `Flat` -> Omitted if the enclosing group is a multiline group, printed for groups fitting on a single line
    /// - `Expanded` -> Omitted if the enclosing group fits on a single line, printed if the group breaks over multiple lines.
    pub(crate) mode: PrintMode,

    /// The id of the group for which it should check if it breaks or not. The group must appear in the document
    /// before the conditional group (but doesn't have to be in the ancestor chain).
    pub(crate) group_id: Option<GroupId>,
}

impl Condition {
    pub(crate) fn new(mode: PrintMode) -> Self {
        Self {
            mode,
            group_id: None,
        }
    }

    pub fn if_fits_on_line() -> Self {
        Self {
            mode: PrintMode::Flat,
            group_id: None,
        }
    }

    pub fn if_group_fits_on_line(group_id: GroupId) -> Self {
        Self {
            mode: PrintMode::Flat,
            group_id: Some(group_id),
        }
    }

    pub fn if_breaks() -> Self {
        Self {
            mode: PrintMode::Expanded,
            group_id: None,
        }
    }

    pub fn if_group_breaks(group_id: GroupId) -> Self {
        Self {
            mode: PrintMode::Expanded,
            group_id: Some(group_id),
        }
    }

    #[must_use]
    pub fn with_group_id(mut self, id: Option<GroupId>) -> Self {
        self.group_id = id;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Align(pub(crate) NonZeroU8);

impl Align {
    pub fn count(&self) -> NonZeroU8 {
        self.0
    }
}

#[derive(Debug, Eq, Copy, Clone)]
pub struct LabelId {
    value: u64,
    #[cfg(debug_assertions)]
    name: &'static str,
}

impl PartialEq for LabelId {
    fn eq(&self, other: &Self) -> bool {
        let is_equal = self.value == other.value;

        #[cfg(debug_assertions)]
        {
            if is_equal {
                assert_eq!(self.name, other.name, "Two `LabelId`s with different names have the same `value`. Are you mixing labels of two different `LabelDefinition` or are the values returned by the `LabelDefinition` not unique?");
            }
        }

        is_equal
    }
}

impl LabelId {
    #[allow(clippy::needless_pass_by_value)]
    pub fn of<T: LabelDefinition>(label: T) -> Self {
        Self {
            value: label.value(),
            #[cfg(debug_assertions)]
            name: label.name(),
        }
    }
}

/// Defines the valid labels of a language. You want to have at most one implementation per formatter
/// project.
pub trait LabelDefinition {
    /// Returns the `u64` uniquely identifying this specific label.
    fn value(&self) -> u64;

    /// Returns the name of the label that is shown in debug builds.
    fn name(&self) -> &'static str;
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
