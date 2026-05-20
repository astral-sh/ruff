//! Config schema deserializer and hand-rolled validator.
//!
//! No external schema-validation crate is used (sha2/jsonschema are not in
//! workspace deps). Validation is type-driven + structural checks below.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Deserialize;

/// Known top-level keys — used for `did_you_mean` on unknown-key errors.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &["$schema", "root", "include", "exclude", "match", "group"];

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Parsed and validated configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Root directory of the source tree to scan.
    pub root: Option<String>,
    /// Glob patterns for files to include (empty = include all `.py`).
    pub include: Vec<String>,
    /// Glob patterns for files to exclude.
    pub exclude: Vec<String>,
    /// Match rules — at least one required.
    pub match_rules: Vec<MatchRule>,
    /// Family-grouping configuration.
    pub group: GroupConfig,
}

impl Config {
    /// Load and validate a config from a JSON file at `path`.
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(path.display().to_string(), e))?;
        Self::from_json_str(&text)
    }

    /// Load and validate from a JSON string.
    pub fn from_json_str(text: &str) -> Result<Self, ConfigError> {
        // First, parse into a raw Value to catch unknown top-level keys.
        let raw: serde_json::Value =
            serde_json::from_str(text).map_err(|e| ConfigError::Json(e.to_string()))?;

        let obj = raw.as_object().ok_or_else(|| ConfigError::Json("root must be an object".to_string()))?;

        // Check for unknown top-level keys and emit did_you_mean hints.
        for key in obj.keys() {
            if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
                let hint = closest_match(key, KNOWN_TOP_LEVEL_KEYS);
                return Err(ConfigError::UnknownKey {
                    key: key.clone(),
                    did_you_mean: hint,
                });
            }
        }

        // Now deserialize into the typed raw struct.
        let raw_cfg: RawConfig =
            serde_json::from_value(raw.clone()).map_err(|e| ConfigError::Json(e.to_string()))?;

        // Validate match rules.
        if raw_cfg.match_rules.is_empty() {
            return Err(ConfigError::Validation(
                "\"match\" array must have at least one rule".to_string(),
            ));
        }

        let mut match_rules = Vec::with_capacity(raw_cfg.match_rules.len());
        for (idx, raw_rule) in raw_cfg.match_rules.into_iter().enumerate() {
            let rule = validate_match_rule(raw_rule, idx)?;
            match_rules.push(rule);
        }

        // Validate group config.
        let group = validate_group(raw_cfg.group)?;

        Ok(Config {
            root: raw_cfg.root,
            include: raw_cfg.include,
            exclude: raw_cfg.exclude,
            match_rules,
            group,
        })
    }
}

// ---------------------------------------------------------------------------
// Match rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MatchRule {
    /// Identifier emitted as `match_id` in output bundles.
    pub id: String,
    /// AST shape this rule targets.
    pub kind: MatchKind,
    /// Optional decorator selector (required for `function_with_decorator`).
    pub decorator: Option<DecoratorSelector>,
    /// Output field name → dot-path expression. Stable key order preserved.
    pub emit: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchKind {
    FunctionWithDecorator,
}

#[derive(Debug, Clone, Default)]
pub struct DecoratorSelector {
    /// Match decorators whose attribute name equals this value.
    pub attribute: Option<String>,
    /// Match decorators whose bare name equals this value.
    pub name: Option<String>,
    /// Match any decorator.
    pub any: bool,
    /// Minimum positional args the decorator call must supply.
    pub min_positional_args: Option<u32>,
}

// ---------------------------------------------------------------------------
// Group config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct GroupConfig {
    pub family_from_filename: Option<FamilyFromFilename>,
}

#[derive(Debug, Clone)]
pub struct FamilyFromFilename {
    pub regex: String,
    /// Pre-compiled regex (guaranteed to contain `family` named capture).
    pub compiled: regex::Regex,
}

// ---------------------------------------------------------------------------
// Raw deserialization types (private)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawConfig {
    root: Option<String>,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(rename = "match", default)]
    match_rules: Vec<RawMatchRule>,
    #[serde(default)]
    group: RawGroupConfig,
}

#[derive(Deserialize)]
struct RawMatchRule {
    id: String,
    kind: String,
    #[serde(default)]
    decorator: Option<RawDecoratorSelector>,
    #[serde(default)]
    emit: HashMap<String, String>,
}

#[derive(Deserialize, Default)]
struct RawDecoratorSelector {
    attribute: Option<String>,
    name: Option<String>,
    #[serde(rename = "any")]
    any: Option<bool>,
    min_positional_args: Option<u32>,
}

#[derive(Deserialize, Default)]
struct RawGroupConfig {
    family_from_filename: Option<RawFamilyFromFilename>,
}

#[derive(Deserialize)]
struct RawFamilyFromFilename {
    regex: String,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_match_rule(raw: RawMatchRule, idx: usize) -> Result<MatchRule, ConfigError> {
    let kind = match raw.kind.as_str() {
        "function_with_decorator" => MatchKind::FunctionWithDecorator,
        other => {
            return Err(ConfigError::Validation(format!(
                "match[{idx}].kind: unknown value {other:?}; only \"function_with_decorator\" is implemented"
            )));
        }
    };

    let decorator = raw.decorator.map(|d| DecoratorSelector {
        attribute: d.attribute,
        name: d.name,
        any: d.any.unwrap_or(false),
        min_positional_args: d.min_positional_args,
    });

    // Stable ordering of emit keys.
    let emit: BTreeMap<String, String> = raw.emit.into_iter().collect();

    Ok(MatchRule {
        id: raw.id,
        kind,
        decorator,
        emit,
    })
}

fn validate_group(raw: RawGroupConfig) -> Result<GroupConfig, ConfigError> {
    let family_from_filename = if let Some(raw_fff) = raw.family_from_filename {
        let compiled = regex::Regex::new(&raw_fff.regex).map_err(|e| {
            ConfigError::Validation(format!(
                "group.family_from_filename.regex is not a valid regex: {e}"
            ))
        })?;
        if compiled.capture_names().all(|n| n != Some("family")) {
            return Err(ConfigError::Validation(
                "group.family_from_filename.regex must contain a named capture group `family`"
                    .to_string(),
            ));
        }
        Some(FamilyFromFilename {
            regex: raw_fff.regex,
            compiled,
        })
    } else {
        None
    };

    Ok(GroupConfig { family_from_filename })
}

/// Levenshtein-distance-based closest match for a typo hint.
fn closest_match(key: &str, candidates: &[&str]) -> Option<String> {
    let best = candidates
        .iter()
        .min_by_key(|&&c| edit_distance(key, c))?;
    Some((*best).to_string())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    // prev[j] = edit distance between a[..i-1] and b[..j]
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j].min(curr[j - 1]).min(prev[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ConfigError {
    Io(String, std::io::Error),
    Json(String),
    UnknownKey {
        key: String,
        did_you_mean: Option<String>,
    },
    Validation(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "could not read {path}: {e}"),
            Self::Json(msg) => write!(f, "JSON parse error: {msg}"),
            Self::UnknownKey { key, did_you_mean } => {
                write!(f, "unknown config key {key:?}")?;
                if let Some(hint) = did_you_mean {
                    write!(f, "; did you mean {hint:?}?")?;
                }
                Ok(())
            }
            Self::Validation(msg) => write!(f, "config validation error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}
