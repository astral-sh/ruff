use std::fmt::Debug;

use ruff_macros::CacheKey;

use crate::registry::{Rule, RuleSet, RuleSetIterator};

/// A table to keep track of which rules are enabled
/// and Whether they should be autofixed.
#[derive(Debug, CacheKey, Default)]
pub struct RuleTable {
    /// Maps rule codes to a boolean indicating if the rule should be autofixed.
    enabled: RuleSet,
    should_fix: RuleSet,
}

impl RuleTable {
    /// Creates a new empty rule table.
    pub const fn empty() -> Self {
        Self {
            enabled: RuleSet::empty(),
            should_fix: RuleSet::empty(),
        }
    }

    /// Returns whether the given rule should be checked.
    #[inline]
    pub const fn enabled(&self, rule: Rule) -> bool {
        self.enabled.contains(rule)
    }

    /// Returns whether any of the given rules should be checked.
    #[inline]
    pub const fn any_enabled(&self, rules: &[Rule]) -> bool {
        self.enabled.intersects(&RuleSet::from_rules(rules))
    }

    /// Returns whether violations of the given rule should be autofixed.
    #[inline]
    pub const fn should_fix(&self, rule: Rule) -> bool {
        self.should_fix.contains(rule)
    }

    /// Returns an iterator over all enabled rules.
    pub fn iter_enabled(&self) -> RuleSetIterator {
        self.enabled.iter()
    }

    /// Enables the given rule.
    #[inline]
    pub fn enable(&mut self, rule: Rule, should_fix: bool) {
        self.enabled.insert(rule);

        if should_fix {
            self.should_fix.insert(rule);
        }
    }

    /// Disables the given rule.
    #[inline]
    pub fn disable(&mut self, rule: Rule) {
        self.enabled.remove(rule);
        self.should_fix.remove(rule);
    }
}

impl FromIterator<Rule> for RuleTable {
    fn from_iter<T: IntoIterator<Item = Rule>>(iter: T) -> Self {
        let rules = RuleSet::from_iter(iter);
        Self {
            enabled: rules.clone(),
            should_fix: rules,
        }
    }
}
