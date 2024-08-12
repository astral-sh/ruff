use std::collections::HashMap;

use once_cell::sync::Lazy;

/// Returns the redirect target for the given code.
pub(crate) fn get_redirect_target(code: &str) -> Option<&'static str> {
    REDIRECTS.get(code).copied()
}

/// Returns the code and the redirect target if the given code is a redirect.
/// (The same code is returned to obtain it with a static lifetime).
pub(crate) fn get_redirect(code: &str) -> Option<(&'static str, &'static str)> {
    REDIRECTS.get_key_value(code).map(|(k, v)| (*k, *v))
}

static REDIRECTS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from_iter([
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
        ("TYP", "TCH"),
        ("TYP001", "TCH001"),
        // TODO(charlie): Remove by 2023-06-01.
        ("RUF004", "B026"),
        ("PIE802", "C419"),
        ("PLW0130", "B033"),
        ("T001", "FIX001"),
        ("T002", "FIX002"),
        ("T003", "FIX003"),
        ("T004", "FIX004"),
        ("RUF011", "B035"),
        ("TCH006", "TCH010"),
        ("TRY200", "B904"),
        ("PGH001", "S307"),
        ("PGH002", "G010"),
        // flake8-trio and flake8-async merged with name flake8-async
        ("TRIO", "ASYNC1"),
        ("TRIO1", "ASYNC1"),
        ("TRIO10", "ASYNC10"),
        ("TRIO100", "ASYNC100"),
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
    ])
});
