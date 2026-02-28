use std::hash::Hasher;

use crate::external::ast::rule::{CallCalleeMatcher, ExternalAstLinter, ExternalAstRule};
use crate::external::ast::target::{AstTarget, ExprKind, StmtKind};
use crate::external::error::ExternalLinterError;
use ruff_index::{IndexVec, newtype_index};
use rustc_hash::{FxHashMap, FxHashSet};

#[newtype_index]
pub struct LinterIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuleLocator {
    pub linter_index: LinterIndex,
    pub rule_index: usize,
}

impl RuleLocator {
    pub const fn new(linter_index: LinterIndex, rule_index: usize) -> Self {
        Self {
            linter_index,
            rule_index,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExternalLintRegistry {
    linters: IndexVec<LinterIndex, ExternalAstLinter>,
    index_by_code: FxHashMap<String, RuleLocator>,
    stmt_index: FxHashMap<StmtKind, Vec<RuleLocator>>,
    expr_index: FxHashMap<ExprKind, Vec<RuleLocator>>,
}

impl ExternalLintRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.linters.is_empty()
    }

    pub fn linters(&self) -> &[ExternalAstLinter] {
        self.linters.raw.as_slice()
    }

    pub fn insert_linter(&mut self, linter: ExternalAstLinter) -> Result<(), ExternalLinterError> {
        if self.linters.iter().any(|existing| existing.id == linter.id) {
            return Err(ExternalLinterError::DuplicateLinter { id: linter.id });
        }

        let mut codes = FxHashSet::default();
        for rule in &linter.rules {
            let code = rule.code.as_str();
            if !codes.insert(code) || self.index_by_code.contains_key(code) {
                return Err(ExternalLinterError::DuplicateRule {
                    linter: linter.id.clone(),
                    code: code.to_string(),
                });
            }
        }

        let linter_index = self.linters.next_index();
        for (rule_index, rule) in linter.rules.iter().enumerate() {
            let code = rule.code.as_str().to_string();
            self.index_by_code
                .insert(code, RuleLocator::new(linter_index, rule_index));

            if !linter.enabled {
                continue;
            }

            for target in &rule.targets {
                match target {
                    AstTarget::Stmt(kind) => self
                        .stmt_index
                        .entry(*kind)
                        .or_default()
                        .push(RuleLocator::new(linter_index, rule_index)),
                    AstTarget::Expr(kind) => self
                        .expr_index
                        .entry(*kind)
                        .or_default()
                        .push(RuleLocator::new(linter_index, rule_index)),
                }
            }
        }
        self.linters.push(linter);
        Ok(())
    }

    pub fn get_rule(&self, locator: RuleLocator) -> Option<&ExternalAstRule> {
        self.linters
            .get(locator.linter_index)
            .and_then(|linter| linter.rules.get(locator.rule_index))
    }

    pub fn get_linter(&self, locator: RuleLocator) -> Option<&ExternalAstLinter> {
        self.linters.get(locator.linter_index)
    }

    pub fn find_rule_by_code(
        &self,
        code: &str,
    ) -> Option<(RuleLocator, &ExternalAstRule, &ExternalAstLinter)> {
        let locator = *self.index_by_code.get(code)?;
        let linter = self.linters.get(locator.linter_index)?;
        if !linter.enabled {
            return None;
        }
        let rule = linter.rules.get(locator.rule_index)?;
        Some((locator, rule, linter))
    }

    pub fn rules_for_stmt(&self, kind: StmtKind) -> impl Iterator<Item = RuleLocator> + '_ {
        self.stmt_index.get(&kind).into_iter().flatten().copied()
    }

    pub fn rules_for_expr(&self, kind: ExprKind) -> impl Iterator<Item = RuleLocator> + '_ {
        self.expr_index.get(&kind).into_iter().flatten().copied()
    }

    pub fn rule_entry(
        &self,
        locator: RuleLocator,
    ) -> Option<(&ExternalAstRule, &ExternalAstLinter)> {
        let linter = self.linters.get(locator.linter_index)?;
        let rule = linter.rules.get(locator.rule_index)?;
        Some((rule, linter))
    }
}

impl ruff_cache::CacheKey for ExternalLintRegistry {
    fn cache_key(&self, key: &mut ruff_cache::CacheKeyHasher) {
        key.write_usize(self.linters.len());
        for linter in &self.linters {
            linter.id.as_str().cache_key(key);
            linter.enabled.cache_key(key);
            linter.name.as_str().cache_key(key);
            linter.description.as_deref().cache_key(key);
            key.write_usize(linter.rules.len());
            for rule in &linter.rules {
                rule.code.as_str().cache_key(key);
                rule.name.as_str().cache_key(key);
                rule.summary.as_deref().cache_key(key);
                rule.call_callee()
                    .map(CallCalleeMatcher::pattern)
                    .cache_key(key);
                key.write_usize(rule.targets.len());
                for target in &rule.targets {
                    match target {
                        AstTarget::Stmt(kind) => {
                            key.write_u8(0);
                            key.write_u16(*kind as u16);
                        }
                        AstTarget::Expr(kind) => {
                            key.write_u8(1);
                            key.write_u16(*kind as u16);
                        }
                    }
                }
                let path_str = rule.script.path().to_string_lossy();
                key.write_usize(path_str.len());
                key.write(path_str.as_bytes());
                let contents_str = rule.script.body();
                key.write_usize(contents_str.len());
                key.write(contents_str.as_bytes());
            }
        }
    }
}
