use strum_macros::EnumIter;

use crate::docstrings::styles::SectionStyle;
use ruff_python_ast::whitespace;

#[derive(EnumIter, PartialEq, Eq, Debug, Clone, Copy)]
pub enum SectionKind {
    Args,
    Arguments,
    Attention,
    Attributes,
    Caution,
    Danger,
    Error,
    Example,
    Examples,
    ExtendedSummary,
    Hint,
    Important,
    KeywordArgs,
    KeywordArguments,
    Methods,
    Note,
    Notes,
    OtherParameters,
    Parameters,
    Raises,
    References,
    Return,
    Returns,
    SeeAlso,
    ShortSummary,
    Tip,
    Todo,
    Warning,
    Warnings,
    Warns,
    Yield,
    Yields,
}

impl SectionKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "args" => Some(Self::Args),
            "arguments" => Some(Self::Arguments),
            "attention" => Some(Self::Attention),
            "attributes" => Some(Self::Attributes),
            "caution" => Some(Self::Caution),
            "danger" => Some(Self::Danger),
            "error" => Some(Self::Error),
            "example" => Some(Self::Example),
            "examples" => Some(Self::Examples),
            "extended summary" => Some(Self::ExtendedSummary),
            "hint" => Some(Self::Hint),
            "important" => Some(Self::Important),
            "keyword args" => Some(Self::KeywordArgs),
            "keyword arguments" => Some(Self::KeywordArguments),
            "methods" => Some(Self::Methods),
            "note" => Some(Self::Note),
            "notes" => Some(Self::Notes),
            "other parameters" => Some(Self::OtherParameters),
            "parameters" => Some(Self::Parameters),
            "raises" => Some(Self::Raises),
            "references" => Some(Self::References),
            "return" => Some(Self::Return),
            "returns" => Some(Self::Returns),
            "see also" => Some(Self::SeeAlso),
            "short summary" => Some(Self::ShortSummary),
            "tip" => Some(Self::Tip),
            "todo" => Some(Self::Todo),
            "warning" => Some(Self::Warning),
            "warnings" => Some(Self::Warnings),
            "warns" => Some(Self::Warns),
            "yield" => Some(Self::Yield),
            "yields" => Some(Self::Yields),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Args => "Args",
            Self::Arguments => "Arguments",
            Self::Attention => "Attention",
            Self::Attributes => "Attributes",
            Self::Caution => "Caution",
            Self::Danger => "Danger",
            Self::Error => "Error",
            Self::Example => "Example",
            Self::Examples => "Examples",
            Self::ExtendedSummary => "Extended Summary",
            Self::Hint => "Hint",
            Self::Important => "Important",
            Self::KeywordArgs => "Keyword Args",
            Self::KeywordArguments => "Keyword Arguments",
            Self::Methods => "Methods",
            Self::Note => "Note",
            Self::Notes => "Notes",
            Self::OtherParameters => "Other Parameters",
            Self::Parameters => "Parameters",
            Self::Raises => "Raises",
            Self::References => "References",
            Self::Return => "Return",
            Self::Returns => "Returns",
            Self::SeeAlso => "See Also",
            Self::ShortSummary => "Short Summary",
            Self::Tip => "Tip",
            Self::Todo => "Todo",
            Self::Warning => "Warning",
            Self::Warnings => "Warnings",
            Self::Warns => "Warns",
            Self::Yield => "Yield",
            Self::Yields => "Yields",
        }
    }
}

#[derive(Debug)]
pub(crate) struct SectionContext<'a> {
    /// The "kind" of the section, e.g. "SectionKind::Args" or "SectionKind::Returns".
    pub(crate) kind: SectionKind,
    /// The name of the section as it appears in the docstring, e.g. "Args" or "Returns".
    pub(crate) section_name: &'a str,
    pub(crate) previous_line: &'a str,
    pub(crate) line: &'a str,
    pub(crate) following_lines: &'a [&'a str],
    pub(crate) is_last_section: bool,
    pub(crate) original_index: usize,
}

fn suspected_as_section(line: &str, style: SectionStyle) -> Option<SectionKind> {
    if let Some(kind) = SectionKind::from_str(whitespace::leading_words(line)) {
        if style.sections().contains(&kind) {
            return Some(kind);
        }
    }
    None
}

/// Check if the suspected context is really a section header.
fn is_docstring_section(context: &SectionContext) -> bool {
    let section_name_suffix = context
        .line
        .trim()
        .strip_prefix(context.section_name)
        .unwrap()
        .trim();
    let this_looks_like_a_section_name =
        section_name_suffix == ":" || section_name_suffix.is_empty();
    if !this_looks_like_a_section_name {
        return false;
    }

    let prev_line = context.previous_line.trim();
    let prev_line_ends_with_punctuation = [',', ';', '.', '-', '\\', '/', ']', '}', ')']
        .into_iter()
        .any(|char| prev_line.ends_with(char));
    let prev_line_looks_like_end_of_paragraph =
        prev_line_ends_with_punctuation || prev_line.is_empty();
    if !prev_line_looks_like_end_of_paragraph {
        return false;
    }

    true
}

/// Extract all `SectionContext` values from a docstring.
pub(crate) fn section_contexts<'a>(
    lines: &'a [&'a str],
    style: SectionStyle,
) -> Vec<SectionContext<'a>> {
    let mut contexts = vec![];
    for (kind, lineno) in lines
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(lineno, line)| suspected_as_section(line, style).map(|kind| (kind, lineno)))
    {
        let context = SectionContext {
            kind,
            section_name: whitespace::leading_words(lines[lineno]),
            previous_line: lines[lineno - 1],
            line: lines[lineno],
            following_lines: &lines[lineno + 1..],
            original_index: lineno,
            is_last_section: false,
        };
        if is_docstring_section(&context) {
            contexts.push(context);
        }
    }

    let mut truncated_contexts = Vec::with_capacity(contexts.len());
    let mut end: Option<usize> = None;
    for context in contexts.into_iter().rev() {
        let next_end = context.original_index;
        truncated_contexts.push(SectionContext {
            kind: context.kind,
            section_name: context.section_name,
            previous_line: context.previous_line,
            line: context.line,
            following_lines: end.map_or(context.following_lines, |end| {
                &lines[context.original_index + 1..end]
            }),
            original_index: context.original_index,
            is_last_section: end.is_none(),
        });
        end = Some(next_end);
    }
    truncated_contexts.reverse();
    truncated_contexts
}
