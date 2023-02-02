use std::collections::hash_map;

use rustc_hash::FxHashMap;

use super::hashable::HashableHashMap;
use crate::registry::Rule;

/// A table to keep track of which rules are enabled
/// and Whether they should be autofixed.
#[derive(Debug, Hash)]
pub struct RuleTable {
    /// Maps rule codes to a boolean indicating if the rule should be autofixed.
    enabled: HashableHashMap<Rule, bool>,
}

impl RuleTable {
    /// Creates a new empty rule table.
    pub fn empty() -> Self {
        Self {
            enabled: HashableHashMap::default(),
        }
    }

    /// Returns whether the given rule should be checked.
    pub fn enabled(&self, code: &Rule) -> bool {
        self.enabled.contains_key(code)
    }

    /// Returns whether violations of the given rule should be autofixed.
    pub fn should_fix(&self, code: &Rule) -> bool {
        *self.enabled.get(code).unwrap_or(&false)
    }

    /// Returns an iterator over all enabled rules.
    pub fn iter_enabled(&self) -> hash_map::Keys<Rule, bool> {
        self.enabled.keys()
    }

    /// Enables the given rule.
    pub fn enable(&mut self, code: Rule, should_fix: bool) {
        self.enabled.insert(code, should_fix);
    }

    /// Disables the given rule.
    pub fn disable(&mut self, rule: &Rule) {
        self.enabled.remove(rule);
    }
}

impl<I: IntoIterator<Item = Rule>> From<I> for RuleTable {
    fn from(codes: I) -> Self {
        let mut enabled = FxHashMap::default();
        for code in codes {
            enabled.insert(code, true);
        }
        Self {
            enabled: enabled.into(),
        }
    }
}
