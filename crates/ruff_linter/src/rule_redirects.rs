use std::collections::HashMap;
use std::sync::LazyLock;

use crate::registry::Rule;

/// Returns the redirect target for a rule code, prefix, or name.
pub(crate) fn get_redirect_target(selector: &str) -> Option<&'static str> {
    REDIRECTS.get(selector).copied()
}

/// Returns the original selector and its redirect target, both with a static lifetime.
pub(crate) fn get_redirect(selector: &str) -> Option<(&'static str, &'static str)> {
    REDIRECTS.get_key_value(selector).map(|(k, v)| (*k, *v))
}

/// Former rule names and their current rules.
pub const RULE_NAME_REDIRECTS: &[(&str, Rule)] = &[(
    "missing-return-type-undocumented-public-function",
    Rule::MissingReturnTypePublicFunction,
)];

static REDIRECTS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut redirects = HashMap::from([
        // The following are here because we don't yet have the many-to-one mapping enabled.
        ("SIM111", "SIM110"),
        // The following are deprecated.
        ("C9", "C90"),
        ("T1", "T10"),
        ("T2", "T20"),
        // TODO(charlie): Remove by 2023-02-01.
        ("R", "RET"),
        ("R5", "RET5"),
        ("R50", "RET50"),
        ("R501", "RET501"),
        ("R502", "RET502"),
        ("R503", "RET503"),
        ("R504", "RET504"),
        ("R505", "RET505"),
        ("R506", "RET506"),
        ("R507", "RET507"),
        ("R508", "RET508"),
        ("IC", "ICN"),
        ("IC0", "ICN0"),
        ("IC00", "ICN00"),
        ("IC001", "ICN001"),
        ("IC002", "ICN001"),
        ("IC003", "ICN001"),
        ("IC004", "ICN001"),
        // TODO(charlie): Remove by 2023-01-01.
        ("U", "UP"),
        ("U0", "UP0"),
        ("U00", "UP00"),
        ("U001", "UP001"),
        ("U003", "UP003"),
        ("U004", "UP004"),
        ("U005", "UP005"),
        ("U006", "UP006"),
        ("U007", "UP007"),
        ("U008", "UP008"),
        ("U009", "UP009"),
        ("U01", "UP01"),
        ("U010", "UP010"),
        ("U011", "UP011"),
        ("U012", "UP012"),
        ("U013", "UP013"),
        ("U014", "UP014"),
        ("U015", "UP015"),
        ("U016", "UP016"),
        ("U017", "UP017"),
        ("U019", "UP019"),
        // TODO(charlie): Remove by 2023-02-01.
        ("I2", "TID2"),
        ("I25", "TID25"),
        ("I252", "TID252"),
        ("M", "RUF100"),
        ("M0", "RUF100"),
        ("M001", "RUF100"),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV", "PD"),
        ("PDV0", "PD0"),
        ("PDV002", "PD002"),
        ("PDV003", "PD003"),
        ("PDV004", "PD004"),
        ("PDV007", "PD007"),
        ("PDV008", "PD008"),
        ("PDV009", "PD009"),
        ("PDV01", "PD01"),
        ("PDV010", "PD010"),
        ("PDV011", "PD011"),
        ("PDV012", "PD012"),
        ("PDV013", "PD013"),
        ("PDV015", "PD015"),
        ("PDV9", "PD9"),
        ("PDV90", "PD90"),
        ("PDV901", "PD901"),
        // TODO(charlie): Remove by 2023-04-01.
        ("TYP", "TC"),
        ("TYP001", "TC001"),
        // TODO(charlie): Remove by 2023-06-01.
        ("RUF004", "B026"),
        ("PIE802", "C419"),
        ("PLW0130", "B033"),
        ("T001", "FIX001"),
        ("T002", "FIX002"),
        ("T003", "FIX003"),
        ("T004", "FIX004"),
        ("RUF011", "B035"),
        ("TRY200", "B904"),
        ("PGH001", "S307"),
        ("PGH002", "G010"),
        // flake8-trio and flake8-async merged with name flake8-async
        ("TRIO", "ASYNC1"),
        ("TRIO1", "ASYNC1"),
        ("TRIO10", "ASYNC10"),
        ("TRIO100", "ASYNC100"),
        ("TRIO103", "ASYNC103"),
        ("TRIO105", "ASYNC105"),
        ("TRIO109", "ASYNC109"),
        ("TRIO11", "ASYNC11"),
        ("TRIO110", "ASYNC110"),
        ("TRIO115", "ASYNC115"),
        // Removed in v0.5
        ("PLR1701", "SIM101"),
        // Test redirect by exact code
        #[cfg(any(feature = "test-rules", test))]
        ("RUF940", "RUF950"),
        // Test redirect by prefix
        #[cfg(any(feature = "test-rules", test))]
        ("RUF96", "RUF95"),
        // See: https://github.com/astral-sh/ruff/issues/10791
        ("PLW0117", "PLW0177"),
        // See: https://github.com/astral-sh/ruff/issues/12110
        ("RUF025", "C420"),
        // See: https://github.com/astral-sh/ruff/issues/13492
        ("TRY302", "TRY203"),
        // TCH renamed to TC to harmonize with flake8 plugin
        ("TCH", "TC"),
        ("TCH001", "TC001"),
        ("TCH002", "TC002"),
        ("TCH003", "TC003"),
        ("TCH004", "TC004"),
        ("TCH005", "TC005"),
        ("TCH006", "TC010"),
        ("TCH010", "TC010"),
        ("RUF035", "S704"),
    ]);
    redirects.extend(
        RULE_NAME_REDIRECTS
            .iter()
            .map(|&(old_name, rule)| (old_name, rule.name().as_str())),
    );
    redirects
});

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::codes::{Rule, RuleStatus};
    use crate::rule_redirects::{REDIRECTS, RULE_NAME_REDIRECTS};
    use strum::IntoEnumIterator;

    /// Tests for rule names and codes that overlap with a redirect.
    #[test]
    fn overshadowing_redirects() {
        for rule in Rule::iter() {
            if matches!(rule.status(), RuleStatus::Removed { .. }) {
                continue;
            }

            let name = rule.name();
            if let Some(redirect_target) = REDIRECTS.get(name.as_str()) {
                panic!(
                    "Rule {name} is overshadowed by a redirect, which points to {redirect_target}."
                );
            }
            if let Some(code) = rule.noqa_code()
                && let Some(redirect_target) = REDIRECTS.get(&*code.to_string())
            {
                panic!(
                    "Rule {code} is overshadowed by a redirect, which points to {redirect_target}."
                );
            }
        }
    }

    /// Tests for redirected rule names and codes that overlap with each other.
    #[test]
    fn unique_redirects() {
        let codes = Rule::iter()
            .filter_map(|rule| rule.noqa_code())
            .flat_map(|code| {
                let code = code.to_string();
                [code.to_ascii_lowercase(), code]
            });
        let old_names = RULE_NAME_REDIRECTS.iter().map(|&(name, _)| name.to_owned());

        let mut sources = HashSet::new();
        for source in codes.chain(old_names) {
            assert!(
                sources.insert(source.clone()),
                "Duplicate redirect for `{source}`",
            );
        }
    }
}
