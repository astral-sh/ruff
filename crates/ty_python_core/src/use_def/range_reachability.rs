use ruff_text_size::{TextRange, TextSize};

use crate::reachability_constraints::ReachabilityConstraintsBuilder;

use super::RangeInfo;

/// Disjoint source ranges in increasing order, with default metadata omitted.
///
/// Statements and their subexpressions are recorded separately during indexing. Normalize their
/// overlapping ranges once so lookups can skip unrelated source code with a binary search.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct RangeReachability(Box<[(TextRange, RangeInfo)]>);

impl RangeReachability {
    pub(super) fn new(
        ranges: Vec<(TextRange, RangeInfo)>,
        constraints: &mut ReachabilityConstraintsBuilder,
    ) -> Self {
        let range_count = ranges.len();
        let mut events = Vec::with_capacity(range_count * 2);
        for (index, (range, info)) in ranges.into_iter().enumerate() {
            if !range.is_empty() && info != RangeInfo::default() {
                events.push((range.start(), index, info));
                events.push((range.end(), index, RangeInfo::default()));
            }
        }
        if events.is_empty() {
            return Self::default();
        }

        let mut active = ActiveRanges::new(range_count);
        // Remove expired ranges before adding new ones, so ranges that only touch never create
        // temporary conjunctions while the active constraints are updated.
        events.sort_unstable_by_key(|(offset, _, info)| (*offset, *info != RangeInfo::default()));
        let mut normalized: Vec<(TextRange, RangeInfo)> = Vec::new();
        let mut previous_offset = TextSize::default();

        for boundary in events.chunk_by(|left, right| left.0 == right.0) {
            let offset = boundary[0].0;
            let info = active.combined();
            if previous_offset < offset && info != RangeInfo::default() {
                let range = TextRange::new(previous_offset, offset);
                if let Some((last_range, last_info)) = normalized.last_mut()
                    && last_range.end() == range.start()
                    && *last_info == info
                {
                    *last_range = last_range.cover(range);
                } else {
                    normalized.push((range, info));
                }
            }

            for &(_, index, info) in boundary {
                active.update(constraints, index, info);
            }
            previous_offset = offset;
        }

        Self(normalized.into_boxed_slice())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &(TextRange, RangeInfo)> {
        self.0.iter()
    }

    /// Whether every part of `range` satisfies `predicate`, including gaps with default metadata.
    /// Empty ranges use the metadata at their offset.
    pub(super) fn all_in_range(
        &self,
        range: TextRange,
        mut predicate: impl FnMut(RangeInfo) -> bool,
    ) -> bool {
        let start = self
            .0
            .partition_point(|(entry, _)| entry.end() <= range.start());
        if range.is_empty() {
            let info = self
                .0
                .get(start)
                .filter(|(entry, _)| entry.contains(range.start()))
                .map(|(_, info)| *info)
                .unwrap_or_default();
            return predicate(info);
        }

        let mut covered_until = range.start();
        for &(entry, info) in &self.0[start..] {
            if entry.start() >= range.end() {
                break;
            }
            if covered_until < entry.start() && !predicate(RangeInfo::default()) {
                return false;
            }
            if !predicate(info) {
                return false;
            }
            covered_until = entry.end();
            if covered_until >= range.end() {
                return true;
            }
        }

        predicate(RangeInfo::default())
    }
}

/// A balanced reduction tree for the ranges active at a sweep boundary.
///
/// Adding or removing a range only recomputes its ancestors. Repeatedly folding all active ranges
/// would make normalization quadratic for deeply nested statements and expressions.
struct ActiveRanges {
    values: Vec<RangeInfo>,
    leaf_start: usize,
}

impl ActiveRanges {
    fn new(len: usize) -> Self {
        let leaf_start = len.next_power_of_two();
        Self {
            values: vec![RangeInfo::default(); leaf_start * 2],
            leaf_start,
        }
    }

    fn combined(&self) -> RangeInfo {
        self.values[1]
    }

    fn update(
        &mut self,
        constraints: &mut ReachabilityConstraintsBuilder,
        index: usize,
        info: RangeInfo,
    ) {
        let mut index = self.leaf_start + index;
        self.values[index] = info;
        while index > 1 {
            index /= 2;
            let left = self.values[index * 2];
            let right = self.values[index * 2 + 1];
            let info = RangeInfo {
                reachability: constraints.add_and_constraint(left.reachability, right.reachability),
                in_type_checking_block: left.in_type_checking_block || right.in_type_checking_block,
            };
            if self.values[index] == info {
                break;
            }
            self.values[index] = info;
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_index::Idx;

    use super::*;
    use crate::predicate::ScopedPredicateId;
    use crate::reachability_constraints::ScopedReachabilityConstraintId;

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(TextSize::new(start), TextSize::new(end))
    }

    /// Normalization produces sorted, disjoint segments. Overlaps conjoin reachability and
    /// inherit `TYPE_CHECKING` status from either input; touching segments with equal metadata
    /// merge. Duplicate, default, and empty inputs do not add segments to the result.
    #[test]
    fn overlapping_ranges_combine_and_coalesce() {
        let mut constraints = ReachabilityConstraintsBuilder::default();
        let first = constraints.add_atom(ScopedPredicateId::new(2));
        let second = constraints.add_atom(ScopedPredicateId::new(3));
        let both = constraints.add_and_constraint(first, second);
        let first_info = RangeInfo {
            reachability: first,
            in_type_checking_block: false,
        };
        let second_info = RangeInfo {
            reachability: second,
            in_type_checking_block: true,
        };
        let ranges = RangeReachability::new(
            vec![
                (range(4, 12), first_info),
                (range(0, 8), second_info),
                (range(12, 14), first_info),
                (range(0, 8), second_info),
                (range(20, 22), RangeInfo::default()),
                (range(25, 25), first_info),
            ],
            &mut constraints,
        );

        assert_eq!(
            ranges.0.as_ref(),
            &[
                (range(0, 4), second_info),
                (
                    range(4, 8),
                    RangeInfo {
                        reachability: both,
                        in_type_checking_block: true,
                    }
                ),
                (range(8, 14), first_info),
            ]
        );
    }

    /// Queries agree with a pointwise scan of the original ranges, including queries that cross
    /// metadata changes or uncovered gaps. Every point must satisfy the predicate, with default
    /// metadata used in gaps. Empty queries use the metadata at their offset, so an interval's
    /// start is included and its end is excluded.
    #[test]
    fn range_queries_include_every_segment_and_default_gap() {
        let unreachable = RangeInfo {
            reachability: ScopedReachabilityConstraintId::ALWAYS_FALSE,
            in_type_checking_block: false,
        };
        let type_checking = RangeInfo {
            in_type_checking_block: true,
            ..RangeInfo::default()
        };
        let original = [
            (range(2, 12), unreachable),
            (range(4, 8), type_checking),
            (range(16, 20), unreachable),
        ];
        let mut constraints = ReachabilityConstraintsBuilder::default();
        let ranges = RangeReachability::new(original.to_vec(), &mut constraints);

        for start in 0..22 {
            for end in start + 1..=22 {
                let expected_unreachable = (start..end).all(|offset| {
                    original.iter().any(|(range, info)| {
                        range.contains(TextSize::new(offset))
                            && info.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE
                    })
                });
                let expected_type_checking = (start..end).all(|offset| {
                    original.iter().any(|(range, info)| {
                        range.contains(TextSize::new(offset)) && info.in_type_checking_block
                    })
                });
                assert_eq!(
                    ranges.all_in_range(range(start, end), |info| info.reachability
                        == ScopedReachabilityConstraintId::ALWAYS_FALSE),
                    expected_unreachable,
                    "{start}..{end}",
                );
                assert_eq!(
                    ranges.all_in_range(range(start, end), |info| info.in_type_checking_block),
                    expected_type_checking,
                    "{start}..{end}",
                );
            }
        }

        assert!(ranges.all_in_range(range(4, 4), |info| info.in_type_checking_block));
        assert!(!ranges.all_in_range(range(8, 8), |info| info.in_type_checking_block));
        assert!(
            RangeReachability::default()
                .all_in_range(range(0, 1), |info| info == RangeInfo::default())
        );
    }
}
