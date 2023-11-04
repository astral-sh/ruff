use anyhow::Result;
use glob::Pattern;
use ruff_macros::CacheKey;
use smallvec::{smallvec, SmallVec};
use std::fmt;

use crate::call_path::CallPath;

#[derive(CacheKey, Debug)]
enum CallPathPatternPart {
    String(String),
    Pattern(Pattern),
}

/// A representation of an expression to match a qualified name, like `os.set*`.
/// TODO: CallPathPatterns currently don't do `**` well.
#[derive(CacheKey, Debug)]
pub struct CallPathPattern {
    parts: SmallVec<[CallPathPatternPart; 8]>,
}

fn to_call_path_pattern_part(part: &str) -> Result<CallPathPatternPart> {
    if part.contains(|c| c == '*' || c == '?' || c == '[' || c == ']') {
        Ok(CallPathPatternPart::Pattern(Pattern::new(part)?))
    } else {
        Ok(CallPathPatternPart::String(String::from(part)))
    }
}

/// Create a [`CallPathPattern`] from a fully-qualified name.
/// ```rust
/// # use ruff_python_ast::call_path;
/// # use ruff_python_ast::call_path_pattern;
///
/// let pat1 = call_path_pattern::from_qualified_name("http.client.HTTP*").unwrap();
/// let pat2 = call_path_pattern::from_qualified_name("http.client").unwrap();
/// let pat3 = call_path_pattern::from_qualified_name("http.*.HTTP*").unwrap();
/// let pth1 = call_path::from_qualified_name("http.client.HTTPConnection");
/// let pth2 = call_path::from_qualified_name("http.click");
/// assert!(pat1.matches_call_path(&pth1, false));
/// assert!(pat2.matches_call_path(&pth1, true));
/// assert!(pat3.matches_call_path(&pth1, false));
/// assert!(!pat1.matches_call_path(&pth2, false));
/// assert!(!pat2.matches_call_path(&pth2, true));
/// ```
pub fn from_qualified_name(name: &str) -> Result<CallPathPattern> {
    if name.contains('.') {
        let parts = name
            .split('.')
            .map(to_call_path_pattern_part)
            .collect::<Result<_>>()?;
        Ok(CallPathPattern { parts })
    } else {
        // Special-case: for builtins, return `["", "int"]` instead of `["int"]`.
        let part = to_call_path_pattern_part(name)?;
        Ok(CallPathPattern {
            parts: smallvec![CallPathPatternPart::String(String::from("")), part],
        })
    }
}

impl CallPathPattern {
    pub fn matches_call_path(&self, call_path: &CallPath, prefix: bool) -> bool {
        if !prefix {
            if self.parts.len() > call_path.len() {
                return false;
            }
        }
        for (part, segment) in self.parts.iter().zip(call_path.iter()) {
            match part {
                CallPathPatternPart::String(part) => {
                    if part != segment {
                        return false;
                    }
                }
                CallPathPatternPart::Pattern(pattern) => {
                    if !pattern.matches(segment) {
                        return false;
                    }
                }
            }
        }
        return true;
    }
}
impl fmt::Display for CallPathPattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, part) in self.parts.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            match part {
                CallPathPatternPart::String(part) => write!(f, "{}", part)?,
                CallPathPatternPart::Pattern(pattern) => write!(f, "{}", pattern)?,
            }
        }
        Ok(())
    }
}
