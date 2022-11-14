//! Abstractions for Google-style docstrings.

use fnv::FnvHashSet;
use once_cell::sync::Lazy;

pub(crate) static GOOGLE_SECTION_NAMES: Lazy<FnvHashSet<&'static str>> = Lazy::new(|| {
    FnvHashSet::from_iter([
        "Args",
        "Arguments",
        "Attention",
        "Attributes",
        "Caution",
        "Danger",
        "Error",
        "Example",
        "Examples",
        "Hint",
        "Important",
        "Keyword Args",
        "Keyword Arguments",
        "Methods",
        "Note",
        "Notes",
        "Return",
        "Returns",
        "Raises",
        "References",
        "See Also",
        "Tip",
        "Todo",
        "Warning",
        "Warnings",
        "Warns",
        "Yield",
        "Yields",
    ])
});

pub(crate) static LOWERCASE_GOOGLE_SECTION_NAMES: Lazy<FnvHashSet<&'static str>> =
    Lazy::new(|| {
        FnvHashSet::from_iter([
            "args",
            "arguments",
            "attention",
            "attributes",
            "caution",
            "danger",
            "error",
            "example",
            "examples",
            "hint",
            "important",
            "keyword args",
            "keyword arguments",
            "methods",
            "note",
            "notes",
            "return",
            "returns",
            "raises",
            "references",
            "see also",
            "tip",
            "todo",
            "warning",
            "warnings",
            "warns",
            "yield",
            "yields",
        ])
    });
