use std::path::PathBuf;

use thiserror::Error;

use crate::external::ast::target::AstTargetParseError;

#[derive(Debug, Error)]
pub enum ExternalLinterError {
    #[error("failed to read external linter definition `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse external linter definition `{path}`: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid rule code `{code}` for external linter `{linter}`")]
    InvalidRuleCode { linter: String, code: String },

    #[error("unknown AST target `{target}` for external rule `{rule}` in linter `{linter}`")]
    // Targets must expand to one of the supported StmtKind or ExprKind enums; anything else is rejected.
    UnknownTarget {
        linter: String,
        rule: String,
        target: String,
        #[source]
        source: AstTargetParseError,
    },

    #[error("duplicate rule code `{code}` in external linter `{linter}`")]
    DuplicateRule { linter: String, code: String },

    #[error("duplicate external linter identifier `{id}`")]
    DuplicateLinter { id: String },

    #[error("external linter `{id}` defines no rules")]
    EmptyLinter { id: String },

    #[error("external rule `{rule}` in linter `{linter}` must declare at least one AST target")]
    MissingTargets { linter: String, rule: String },

    #[error(
        "failed to read script `{path}` for external rule `{rule}` in linter `{linter}`: {source}"
    )]
    ScriptIo {
        linter: String,
        rule: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no script body provided for external rule `{rule}` in linter `{linter}`")]
    // Raised when we read a script file but it is empty or whitespace-only.
    MissingScriptBody { linter: String, rule: String },

    #[error(
        "invalid `call-callee-regex` `{pattern}` for external rule `{rule}` in linter `{linter}`: {source}"
    )]
    InvalidCallCalleeRegex {
        linter: String,
        rule: String,
        pattern: String,
        #[source]
        source: regex::Error,
    },

    #[error(
        "external rule `{rule}` in linter `{linter}` declares `call-callee-regex` but does not target `expr:Call` nodes"
    )]
    CallCalleeRegexWithoutCallTarget { linter: String, rule: String },
}
