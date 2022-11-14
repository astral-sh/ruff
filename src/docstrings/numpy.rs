//! Abstractions for NumPy-style docstrings.

use fnv::FnvHashSet;
use once_cell::sync::Lazy;

pub(crate) static LOWERCASE_NUMPY_SECTION_NAMES: Lazy<FnvHashSet<&'static str>> = Lazy::new(|| {
    FnvHashSet::from_iter([
        "short summary",
        "extended summary",
        "parameters",
        "returns",
        "yields",
        "other parameters",
        "raises",
        "see also",
        "notes",
        "references",
        "examples",
        "attributes",
        "methods",
    ])
});

pub(crate) static NUMPY_SECTION_NAMES: Lazy<FnvHashSet<&'static str>> = Lazy::new(|| {
    FnvHashSet::from_iter([
        "Short Summary",
        "Extended Summary",
        "Parameters",
        "Returns",
        "Yields",
        "Other Parameters",
        "Raises",
        "See Also",
        "Notes",
        "References",
        "Examples",
        "Attributes",
        "Methods",
    ])
});
