use itertools::Itertools;
use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::future::ALL_FEATURE_NAMES;
use rustpython_parser::ast::Alias;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::{Availability, Violation};
use crate::AutofixKind;

define_violation!(
    pub struct UnusedImport {
        pub name: String,
        pub ignore_init: bool,
        pub multiple: bool,
    }
);
fn fmt_unused_import_autofix_msg(unused_import: &UnusedImport) -> String {
    let UnusedImport { name, multiple, .. } = unused_import;
    if *multiple {
        "Remove unused import".to_string()
    } else {
        format!("Remove unused import: `{name}`")
    }
}
impl Violation for UnusedImport {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport {
            name, ignore_init, ..
        } = self;
        if *ignore_init {
            format!(
                "`{name}` imported but unused; consider adding to `__all__` or using a redundant \
                 alias"
            )
        } else {
            format!("`{name}` imported but unused")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let UnusedImport { ignore_init, .. } = self;
        if *ignore_init {
            None
        } else {
            Some(fmt_unused_import_autofix_msg)
        }
    }
}
define_violation!(
    pub struct ImportShadowedByLoopVar {
        pub name: String,
        pub line: usize,
    }
);
impl Violation for ImportShadowedByLoopVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportShadowedByLoopVar { name, line } = self;
        format!("Import `{name}` from line {line} shadowed by loop variable")
    }
}

define_violation!(
    pub struct ImportStarUsed {
        pub name: String,
    }
);
impl Violation for ImportStarUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarUsed { name } = self;
        format!("`from {name} import *` used; unable to detect undefined names")
    }
}

define_violation!(
    pub struct LateFutureImport;
);
impl Violation for LateFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__` imports must occur at the beginning of the file")
    }
}

define_violation!(
    pub struct ImportStarUsage {
        pub name: String,
        pub sources: Vec<String>,
    }
);
impl Violation for ImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarUsage { name, sources } = self;
        let sources = sources
            .iter()
            .map(|source| format!("`{source}`"))
            .join(", ");
        format!("`{name}` may be undefined, or defined from star imports: {sources}")
    }
}

define_violation!(
    pub struct ImportStarNotPermitted {
        pub name: String,
    }
);
impl Violation for ImportStarNotPermitted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarNotPermitted { name } = self;
        format!("`from {name} import *` only allowed at module level")
    }
}

define_violation!(
    pub struct FutureFeatureNotDefined {
        pub name: String,
    }
);
impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined { name } = self;
        format!("Future feature `{name}` is not defined")
    }
}

pub fn future_feature_not_defined(checker: &mut Checker, alias: &Alias) {
    if !ALL_FEATURE_NAMES.contains(&&*alias.node.name) {
        checker.diagnostics.push(Diagnostic::new(
            FutureFeatureNotDefined {
                name: alias.node.name.to_string(),
            },
            Range::from_located(alias),
        ));
    }
}
