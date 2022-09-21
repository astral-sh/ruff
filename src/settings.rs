use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use crate::checks::CheckCode;

#[derive(Debug)]
pub struct Settings {
    pub line_length: usize,
    pub select: BTreeSet<CheckCode>,
}

impl Settings {
    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            line_length: 88,
            select: BTreeSet::from([check_code]),
        }
    }

    pub fn for_rules(check_codes: Vec<CheckCode>) -> Self {
        Self {
            line_length: 88,
            select: BTreeSet::from_iter(check_codes),
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.line_length.hash(state);
        for value in self.select.iter() {
            value.hash(state);
        }
    }
}
