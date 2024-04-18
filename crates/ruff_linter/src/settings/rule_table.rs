use std::fmt::{Debug, Display, Formatter};

use crate::display_settings;
use ruff_macros::CacheKey;

use crate::registry::{Rule, RuleSet, RuleSetIterator};

/// A table to keep track of which rules are enabled and whether they should be fixed.
#[derive(Debug, Clone, CacheKey, Default)]
pub struct RuleTable {
    /// Maps rule codes to a boolean indicating if the rule should be fixed.
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
        self.enabled.any(rules)
    }

    /// Returns whether violations of the given rule should be fixed.
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

impl Display for RuleTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.rules",
            fields = [
                self.enabled,
                self.should_fix
            ]
        }
        Ok(())
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
