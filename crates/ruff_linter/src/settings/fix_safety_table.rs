use std::fmt::Debug;

use ruff_macros::CacheKey;
use rustc_hash::FxHashMap;
use strum::IntoEnumIterator;

use crate::{
    registry::{Rule, RuleSet},
    rule_selector::{PreviewOptions, Specificity},
    RuleSelector,
};

/// A table to keep track of which rules fixes should have
/// their safety overridden.
#[derive(Debug, CacheKey, Default)]
pub struct FixSafetyTable {
    forced_safe: RuleSet,
    forced_unsafe: RuleSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixSafety {
    ForcedSafe,
    ForcedUnsafe,
    Default,
}

impl FixSafetyTable {
    pub const fn resolve_rule(&self, rule: Rule) -> FixSafety {
        if self.forced_safe.contains(rule) {
            FixSafety::ForcedSafe
        } else if self.forced_unsafe.contains(rule) {
            FixSafety::ForcedUnsafe
        } else {
            FixSafety::Default
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.forced_safe.is_empty() && self.forced_unsafe.is_empty()
    }

    pub fn from_rule_selectors(
        extend_safe_fixes: &[RuleSelector],
        extend_unsafe_fixes: &[RuleSelector],
        preview_options: &PreviewOptions,
    ) -> Self {
        enum Override {
            Safe,
            Unsafe,
        }
        use Override::{Safe, Unsafe};

        let safety_override_map: FxHashMap<Rule, Override> = {
            Specificity::iter()
                .flat_map(|spec| {
                    let safe_overrides = extend_safe_fixes
                        .iter()
                        .filter(|selector| selector.specificity() == spec)
                        .flat_map(|selector| selector.rules(preview_options))
                        .map(|rule| (rule, Safe));

                    let unsafe_overrides = extend_unsafe_fixes
                        .iter()
                        .filter(|selector| selector.specificity() == spec)
                        .flat_map(|selector| selector.rules(preview_options))
                        .map(|rule| (rule, Unsafe));

                    // Unsafe overrides take precedence over safe overrides
                    safe_overrides.chain(unsafe_overrides).collect::<Vec<_>>()
                })
                // More specified selectors take precedence over less specified selectors
                .collect()
        };

        FixSafetyTable {
            forced_safe: safety_override_map
                .iter()
                .filter_map(|(rule, o)| match o {
                    Safe => Some(*rule),
                    Unsafe => None,
                })
                .collect(),
            forced_unsafe: safety_override_map
                .iter()
                .filter_map(|(rule, o)| match o {
                    Unsafe => Some(*rule),
                    Safe => None,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_rule() {
        let table = FixSafetyTable {
            forced_safe: RuleSet::from_iter([Rule::RedefinedWhileUnused]),
            forced_unsafe: RuleSet::from_iter([Rule::UnusedImport]),
        };

        assert_eq!(
            table.resolve_rule(Rule::RedefinedWhileUnused),
            FixSafety::ForcedSafe
        );
        assert_eq!(
            table.resolve_rule(Rule::UnusedImport),
            FixSafety::ForcedUnsafe
        );
        assert_eq!(table.resolve_rule(Rule::UndefinedName), FixSafety::Default);
    }

    fn mk_table(safe_fixes: &[&str], unsafe_fixes: &[&str]) -> FixSafetyTable {
        FixSafetyTable::from_rule_selectors(
            &safe_fixes
                .iter()
                .map(|s| s.parse().unwrap())
                .collect::<Vec<_>>(),
            &unsafe_fixes
                .iter()
                .map(|s| s.parse().unwrap())
                .collect::<Vec<_>>(),
            &PreviewOptions::default(),
        )
    }

    fn assert_rules_safety(table: &FixSafetyTable, assertions: &[(&str, FixSafety)]) {
        for (code, expected) in assertions {
            assert_eq!(
                table.resolve_rule(Rule::from_code(code).unwrap()),
                *expected
            );
        }
    }

    #[test]
    fn test_from_rule_selectors_specificity() {
        let table = mk_table(&["UP"], &["ALL", "UP001"]);

        assert_rules_safety(
            &table,
            &[
                ("E101", FixSafety::ForcedUnsafe),
                ("UP001", FixSafety::ForcedUnsafe),
                ("UP003", FixSafety::ForcedSafe),
            ],
        );
    }

    #[test]
    fn test_from_rule_selectors_unsafe_over_safe() {
        let table = mk_table(&["UP"], &["UP"]);

        assert_rules_safety(
            &table,
            &[
                ("E101", FixSafety::Default),
                ("UP001", FixSafety::ForcedUnsafe),
            ],
        );
    }
}
