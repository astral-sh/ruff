/// Match an imported member against the ban policy. For example, given `from foo import bar`,
/// `foo` is the module and `bar` is the member. Performs an exact match.
#[derive(Debug)]
pub(crate) struct MatchName<'a> {
    pub(crate) module: &'a str,
    pub(crate) member: &'a str,
}

impl MatchName<'_> {
    fn is_match(&self, banned_module: &str) -> bool {
        // Ex) Match banned `foo.bar` to import `foo.bar`, without allocating, assuming that
        // `module` is `foo`, `member` is `bar`, and `banned_module` is `foo.bar`.
        banned_module
            .strip_prefix(self.module)
            .and_then(|banned_module| banned_module.strip_prefix('.'))
            .and_then(|banned_module| banned_module.strip_prefix(self.member))
            .is_some_and(str::is_empty)
    }
}

/// Match an imported module against the ban policy. For example, given `import foo.bar`,
/// `foo.bar` is the module. Matches against the module name or any of its parents.
#[derive(Debug)]
pub(crate) struct MatchNameOrParent<'a> {
    pub(crate) module: &'a str,
}

impl MatchNameOrParent<'_> {
    fn is_match(&self, banned_module: &str) -> bool {
        // Ex) Match banned `foo` to import `foo`.
        if self.module == banned_module {
            return true;
        }

        // Ex) Match banned `foo` to import `foo.bar`.
        if self
            .module
            .strip_prefix(banned_module)
            .is_some_and(|suffix| suffix.starts_with('.'))
        {
            return true;
        }

        false
    }
}

#[derive(Debug)]
pub(crate) enum NameMatchPolicy<'a> {
    /// Only match an exact module name (e.g., given `import foo.bar`, only match `foo.bar`).
    MatchName(MatchName<'a>),
    /// Match an exact module name or any of its parents (e.g., given `import foo.bar`, match
    /// `foo.bar` or `foo`).
    MatchNameOrParent(MatchNameOrParent<'a>),
}

impl NameMatchPolicy<'_> {
    pub(crate) fn find<'a>(&self, banned_modules: impl Iterator<Item = &'a str>) -> Option<String> {
        for banned_module in banned_modules {
            match self {
                NameMatchPolicy::MatchName(matcher) => {
                    if matcher.is_match(banned_module) {
                        return Some(banned_module.to_string());
                    }
                }
                NameMatchPolicy::MatchNameOrParent(matcher) => {
                    if matcher.is_match(banned_module) {
                        return Some(banned_module.to_string());
                    }
                }
            }
        }
        None
    }
}
