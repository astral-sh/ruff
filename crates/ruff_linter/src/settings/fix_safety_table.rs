use std::fmt::{Debug, Display, Formatter};

use ruff_diagnostics::Applicability;
use ruff_macros::CacheKey;
use rustc_hash::FxHashMap;
use strum::IntoEnumIterator;

use crate::{
    display_settings,
    registry::{Rule, RuleSet},
    rule_selector::{PreviewOptions, Specificity},
    RuleSelector,
};

/// A table to keep track of which rules fixes should have
/// their safety overridden.
#[derive(Debug, Clone, CacheKey, Default)]
pub struct FixSafetyTable {
    forced_safe: RuleSet,
    forced_unsafe: RuleSet,
}

impl FixSafetyTable {
    pub const fn resolve_applicability(
        &self,
        rule: Rule,
        applicability: Applicability,
    ) -> Applicability {
        match applicability {
            // If applicability is display-only we don't change it
            Applicability::DisplayOnly => applicability,
            Applicability::Safe | Applicability::Unsafe => {
                if self.forced_unsafe.contains(rule) {
                    Applicability::Unsafe
                } else if self.forced_safe.contains(rule) {
                    Applicability::Safe
                } else {
                    applicability
                }
            }
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

        let safety_override_map: FxHashMap<Rule, Override> = {
            Specificity::iter()
                .flat_map(|spec| {
                    let safe_overrides = extend_safe_fixes
                        .iter()
                        .filter(|selector| selector.specificity() == spec)
                        .flat_map(|selector| selector.rules(preview_options))
                        .map(|rule| (rule, Override::Safe));

                    let unsafe_overrides = extend_unsafe_fixes
                        .iter()
                        .filter(|selector| selector.specificity() == spec)
                        .flat_map(|selector| selector.rules(preview_options))
                        .map(|rule| (rule, Override::Unsafe));

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
                    Override::Safe => Some(*rule),
                    Override::Unsafe => None,
                })
                .collect(),
            forced_unsafe: safety_override_map
                .iter()
                .filter_map(|(rule, o)| match o {
                    Override::Unsafe => Some(*rule),
                    Override::Safe => None,
                })
                .collect(),
        }
    }
}

impl Display for FixSafetyTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.safety_table",
            fields = [
                self.forced_safe,
                self.forced_unsafe
            ]
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_applicability() {
        let table = FixSafetyTable {
            forced_safe: RuleSet::from_iter([Rule::RedefinedWhileUnused]),
            forced_unsafe: RuleSet::from_iter([Rule::UnusedImport]),
        };

        for applicability in &[Applicability::Safe, Applicability::Unsafe] {
            assert_eq!(
                table.resolve_applicability(Rule::RedefinedWhileUnused, *applicability),
                Applicability::Safe // It is forced to Safe
            );
        }
        for applicability in &[Applicability::Safe, Applicability::Unsafe] {
            assert_eq!(
                table.resolve_applicability(Rule::UnusedImport, *applicability),
                Applicability::Unsafe // It is forced to Unsafe
            );
        }
        for applicability in &[Applicability::Safe, Applicability::Unsafe] {
            assert_eq!(
                table.resolve_applicability(Rule::UndefinedName, *applicability),
                *applicability // Remains unchanged
            );
        }

        for rule in &[
            Rule::RedefinedWhileUnused,
            Rule::UnusedImport,
            Rule::UndefinedName,
        ] {
            assert_eq!(
                table.resolve_applicability(*rule, Applicability::DisplayOnly),
                Applicability::DisplayOnly // Display is never changed
            );
        }
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

    fn assert_rules_safety(
        table: &FixSafetyTable,
        assertions: &[(&str, Applicability, Applicability)],
    ) {
        for (code, applicability, expected) in assertions {
            assert_eq!(
                table.resolve_applicability(Rule::from_code(code).unwrap(), *applicability),
                *expected
            );
        }
    }

    #[test]
    fn test_from_rule_selectors_specificity() {
        use Applicability::{Safe, Unsafe};
        let table = mk_table(&["UP"], &["ALL", "UP001"]);

        assert_rules_safety(
            &table,
            &[
                ("E101", Safe, Unsafe),
                ("UP001", Safe, Unsafe),
                ("UP003", Unsafe, Safe),
            ],
        );
    }

    #[test]
    fn test_from_rule_selectors_unsafe_over_safe() {
        use Applicability::{Safe, Unsafe};
        let table = mk_table(&["UP"], &["UP"]);

        assert_rules_safety(&table, &[("E101", Safe, Safe), ("UP001", Safe, Unsafe)]);
    }
}
