//! This module should probably not exist in this shape or form.
use crate::codes::Rule;
use crate::registry::Linter;

#[derive(Hash, Eq, PartialEq, Copy, Clone, Debug)]
pub struct UpstreamCategoryAndPrefix {
    pub category: &'static str,
    pub prefix: &'static str,
}

const PLC: UpstreamCategoryAndPrefix = UpstreamCategoryAndPrefix {
    category: "Convention",
    prefix: "PLC",
};

const PLE: UpstreamCategoryAndPrefix = UpstreamCategoryAndPrefix {
    category: "Error",
    prefix: "PLE",
};

const PLR: UpstreamCategoryAndPrefix = UpstreamCategoryAndPrefix {
    category: "Refactor",
    prefix: "PLR",
};

const PLW: UpstreamCategoryAndPrefix = UpstreamCategoryAndPrefix {
    category: "Warning",
    prefix: "PLW",
};

const E: UpstreamCategoryAndPrefix = UpstreamCategoryAndPrefix {
    category: "Error",
    prefix: "E",
};

const W: UpstreamCategoryAndPrefix = UpstreamCategoryAndPrefix {
    category: "Warning",
    prefix: "W",
};

impl Rule {
    pub fn upstream_category(&self, linter: &Linter) -> Option<UpstreamCategoryAndPrefix> {
        let code = linter.code_for_rule(*self).unwrap();
        match linter {
            Linter::Pycodestyle => {
                if code.starts_with('E') {
                    Some(E)
                } else if code.starts_with('W') {
                    Some(W)
                } else {
                    None
                }
            }
            Linter::Pylint => {
                if code.starts_with("PLC") {
                    Some(PLC)
                } else if code.starts_with("PLE") {
                    Some(PLE)
                } else if code.starts_with("PLR") {
                    Some(PLR)
                } else if code.starts_with("PLW") {
                    Some(PLW)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
impl Linter {
    pub const fn upstream_categories(&self) -> Option<&'static [UpstreamCategoryAndPrefix]> {
        match self {
            Linter::Pycodestyle => Some(&[E, W]),
            Linter::Pylint => Some(&[PLC, PLE, PLR, PLW]),
            _ => None,
        }
    }
}
