use std::fmt::{Debug, Display, Formatter};
use std::iter::FusedIterator;

use ruff_macros::CacheKey;

use crate::registry::Rule;

const RULESET_SIZE: usize = 15;

/// A set of [`Rule`]s.
///
/// Uses a bitset where a bit of one signals that the Rule with that [u16] is in this set.
#[derive(Clone, Default, CacheKey, PartialEq, Eq)]
pub struct RuleSet([u64; RULESET_SIZE]);

impl RuleSet {
    const EMPTY: [u64; RULESET_SIZE] = [0; RULESET_SIZE];
    // 64 fits into a u16 without truncation
    #[allow(clippy::cast_possible_truncation)]
    const SLICE_BITS: u16 = u64::BITS as u16;

    /// Returns an empty rule set.
    pub const fn empty() -> Self {
        Self(Self::EMPTY)
    }

    pub fn clear(&mut self) {
        self.0 = Self::EMPTY;
    }

    #[inline]
    pub const fn from_rule(rule: Rule) -> Self {
        let rule = rule as u16;

        let index = (rule / Self::SLICE_BITS) as usize;

        debug_assert!(
            index < Self::EMPTY.len(),
            "Rule index out of bounds. Increase the size of the bitset array."
        );

        // The bit-position of this specific rule in the slice
        let shift = rule % Self::SLICE_BITS;
        // Set the index for that rule to 1
        let mask = 1 << shift;

        let mut bits = Self::EMPTY;
        bits[index] = mask;

        Self(bits)
    }

    #[inline]
    pub const fn from_rules(rules: &[Rule]) -> Self {
        let mut set = RuleSet::empty();

        let mut i = 0;

        // Uses a while because for loops are not allowed in const functions.
        while i < rules.len() {
            set = set.union(&RuleSet::from_rule(rules[i]));
            i += 1;
        }

        set
    }

    /// Returns the union of the two rule sets `self` and `other`
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let set_1 = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::AnyType]);
    /// let set_2 = RuleSet::from_rules(&[
    ///     Rule::BadQuotesInlineString,
    ///     Rule::BooleanPositionalValueInCall,
    /// ]);
    ///
    /// let union = set_1.union(&set_2);
    ///
    /// assert!(union.contains(Rule::AmbiguousFunctionName));
    /// assert!(union.contains(Rule::AnyType));
    /// assert!(union.contains(Rule::BadQuotesInlineString));
    /// assert!(union.contains(Rule::BooleanPositionalValueInCall));
    /// ```
    #[must_use]
    pub const fn union(mut self, other: &Self) -> Self {
        let mut i = 0;

        while i < self.0.len() {
            self.0[i] |= other.0[i];
            i += 1;
        }

        self
    }

    /// Returns `self` without any of the rules contained in `other`.
    ///
    /// ## Examples
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let set_1 = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::AnyType]);
    /// let set_2 = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::Debugger]);
    ///
    /// let subtract = set_1.subtract(&set_2);
    ///
    /// assert!(subtract.contains(Rule::AnyType));
    /// assert!(!subtract.contains(Rule::AmbiguousFunctionName));
    /// ```
    #[must_use]
    pub const fn subtract(mut self, other: &Self) -> Self {
        let mut i = 0;

        while i < self.0.len() {
            self.0[i] &= !other.0[i];
            i += 1;
        }

        self
    }

    /// Returns true if `self` and `other` contain at least one common rule.
    ///
    /// ## Examples
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let set_1 = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::AnyType]);
    ///
    /// assert!(set_1.intersects(&RuleSet::from_rules(&[
    ///     Rule::AnyType,
    ///     Rule::BadQuotesInlineString
    /// ])));
    ///
    /// assert!(!set_1.intersects(&RuleSet::from_rules(&[
    ///     Rule::BooleanPositionalValueInCall,
    ///     Rule::BadQuotesInlineString
    /// ])));
    /// ```
    pub const fn intersects(&self, other: &Self) -> bool {
        let mut i = 0;

        while i < self.0.len() {
            if self.0[i] & other.0[i] != 0 {
                return true;
            }
            i += 1;
        }

        false
    }

    /// Returns `true` if this set contains no rules, `false` otherwise.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// assert!(RuleSet::empty().is_empty());
    ///         assert!(
    ///             !RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::BadQuotesInlineString])
    ///                 .is_empty()
    ///         );
    /// ```
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of rules in this set.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// assert_eq!(RuleSet::empty().len(), 0);
    /// assert_eq!(
    ///     RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::BadQuotesInlineString]).len(),
    ///     2
    /// );
    pub const fn len(&self) -> usize {
        let mut len: u32 = 0;

        let mut i = 0;

        while i < self.0.len() {
            len += self.0[i].count_ones();
            i += 1;
        }

        len as usize
    }

    /// Inserts `rule` into the set.
    ///
    /// ## Examples
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let mut set = RuleSet::empty();
    ///
    /// assert!(!set.contains(Rule::AnyType));
    ///
    /// set.insert(Rule::AnyType);
    ///
    /// assert!(set.contains(Rule::AnyType));
    /// ```
    pub fn insert(&mut self, rule: Rule) {
        let set = std::mem::take(self);
        *self = set.union(&RuleSet::from_rule(rule));
    }

    #[inline]
    pub fn set(&mut self, rule: Rule, enabled: bool) {
        if enabled {
            self.insert(rule);
        } else {
            self.remove(rule);
        }
    }

    /// Removes `rule` from the set.
    ///
    /// ## Examples
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let mut set = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::AnyType]);
    ///
    /// set.remove(Rule::AmbiguousFunctionName);
    ///
    /// assert!(set.contains(Rule::AnyType));
    /// assert!(!set.contains(Rule::AmbiguousFunctionName));
    /// ```
    pub fn remove(&mut self, rule: Rule) {
        let set = std::mem::take(self);
        *self = set.subtract(&RuleSet::from_rule(rule));
    }

    /// Returns `true` if `rule` is in this set.
    ///
    /// ## Examples
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let set = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::AnyType]);
    ///
    /// assert!(set.contains(Rule::AmbiguousFunctionName));
    /// assert!(!set.contains(Rule::BreakOutsideLoop));
    /// ```
    #[inline]
    pub const fn contains(&self, rule: Rule) -> bool {
        let rule = rule as u16;
        let index = rule as usize / Self::SLICE_BITS as usize;
        let shift = rule % Self::SLICE_BITS;
        let mask = 1 << shift;

        self.0[index] & mask != 0
    }

    /// Returns `true` if any of the rules in `rules` are in this set.
    #[inline]
    pub const fn any(&self, rules: &[Rule]) -> bool {
        let mut any = false;
        let mut i = 0;

        while i < rules.len() {
            any |= self.contains(rules[i]);
            i += 1;
        }

        any
    }

    /// Returns an iterator over the rules in this set.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # use ruff_linter::registry::{Rule, RuleSet};
    /// let set = RuleSet::from_rules(&[Rule::AmbiguousFunctionName, Rule::AnyType]);
    ///
    /// let iter: Vec<_> = set.iter().collect();
    ///
    /// assert_eq!(iter, vec![Rule::AnyType, Rule::AmbiguousFunctionName]);
    /// ```
    pub fn iter(&self) -> RuleSetIterator {
        RuleSetIterator {
            set: self.clone(),
            index: 0,
        }
    }
}

impl Debug for RuleSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl Display for RuleSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "[]")?;
        } else {
            writeln!(f, "[")?;
            for rule in self {
                let name = rule.as_ref();
                let code = rule.noqa_code();
                writeln!(f, "\t{name} ({code}),")?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

impl FromIterator<Rule> for RuleSet {
    fn from_iter<T: IntoIterator<Item = Rule>>(iter: T) -> Self {
        let mut set = RuleSet::empty();

        for rule in iter {
            set.insert(rule);
        }

        set
    }
}

impl Extend<Rule> for RuleSet {
    fn extend<T: IntoIterator<Item = Rule>>(&mut self, iter: T) {
        let set = std::mem::take(self);
        *self = set.union(&RuleSet::from_iter(iter));
    }
}

impl IntoIterator for RuleSet {
    type IntoIter = RuleSetIterator;
    type Item = Rule;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl IntoIterator for &RuleSet {
    type IntoIter = RuleSetIterator;
    type Item = Rule;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct RuleSetIterator {
    set: RuleSet,
    index: u16,
}

impl Iterator for RuleSetIterator {
    type Item = Rule;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let slice = self.set.0.get_mut(self.index as usize)?;
            // `trailing_zeros` is guaranteed to return a value in [0;64]
            #[allow(clippy::cast_possible_truncation)]
            let bit = slice.trailing_zeros() as u16;

            if bit < RuleSet::SLICE_BITS {
                *slice ^= 1 << bit;
                let rule_value = self.index * RuleSet::SLICE_BITS + bit;
                // SAFETY: RuleSet guarantees that only valid rules are stored in the set.
                #[allow(unsafe_code)]
                return Some(unsafe { std::mem::transmute::<u16, Rule>(rule_value) });
            }

            self.index += 1;
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.set.len();

        (len, Some(len))
    }
}

impl ExactSizeIterator for RuleSetIterator {}

impl FusedIterator for RuleSetIterator {}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use crate::registry::{Rule, RuleSet};

    /// Tests that the set can contain all rules
    #[test]
    fn test_all_rules() {
        for rule in Rule::iter() {
            let set = RuleSet::from_rule(rule);

            assert!(set.contains(rule));
        }

        let all_rules_set: RuleSet = Rule::iter().collect();
        let all_rules: Vec<_> = all_rules_set.iter().collect();
        let expected_rules: Vec<_> = Rule::iter().collect();
        assert_eq!(all_rules, expected_rules);
    }

    #[test]
    fn remove_not_existing_rule_from_set() {
        let mut set = RuleSet::default();

        set.remove(Rule::AmbiguousFunctionName);

        assert!(!set.contains(Rule::AmbiguousFunctionName));
        assert!(set.is_empty());
        assert_eq!(set.into_iter().collect::<Vec<_>>(), vec![]);
    }
}
