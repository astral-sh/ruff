use super::bitset::{BitSet, BitSetArray, BitSetArrayIterator, BitSetIterator};
use ruff_index::newtype_index;

#[newtype_index]
pub(super) struct ScopedDefinitionId;

#[newtype_index]
pub(super) struct ScopedConstraintId;

/// Can reference this * 128 definitions efficiently; tune for performance vs memory.
const DEFINITION_BLOCKS: usize = 4;

type Definitions = BitSet<DEFINITION_BLOCKS>;
type DefinitionsIterator<'a> = BitSetIterator<'a, DEFINITION_BLOCKS>;

/// Can reference this * 128 constraints efficiently; tune for performance vs memory.
const CONSTRAINT_BLOCKS: usize = 4;

/// Can handle this many visible definitions per symbol at a given time efficiently.
const MAX_EXPECTED_VISIBLE_DEFINITIONS_PER_SYMBOL: usize = 16;

type Constraints = BitSetArray<CONSTRAINT_BLOCKS, MAX_EXPECTED_VISIBLE_DEFINITIONS_PER_SYMBOL>;
type ConstraintsIterator<'a> =
    BitSetArrayIterator<'a, CONSTRAINT_BLOCKS, MAX_EXPECTED_VISIBLE_DEFINITIONS_PER_SYMBOL>;

/// Constrained definitions visible for a symbol at a particular point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConstrainedDefinitions {
    /// Which [`ScopedDefinitionId`] are visible?
    visible_definitions: Definitions,

    /// For each definition, which [`ScopedConstraintId`] apply?
    ///
    /// This is a [`BitSetArray`] which should always have one bitset of constraints per definition
    /// in `visible_definitions`.
    constraints: Constraints,

    /// Is unbound a visible definition as well?
    may_be_unbound: bool,
}

/// A single definition with an iterator of its applicable constraints.
#[derive(Debug)]
pub(super) struct DefinitionIdWithConstraints<'a> {
    pub(super) definition: ScopedDefinitionId,
    pub(super) constraint_ids: ConstraintIdIterator<'a>,
}

impl ConstrainedDefinitions {
    pub(super) fn unbound() -> Self {
        Self {
            visible_definitions: Definitions::default(),
            constraints: Constraints::default(),
            may_be_unbound: true,
        }
    }

    pub(super) fn with(definition_id: ScopedDefinitionId) -> Self {
        Self {
            visible_definitions: Definitions::with(definition_id.into()),
            constraints: Constraints::of_size(1),
            may_be_unbound: false,
        }
    }

    /// Add Unbound as a possibility.
    pub(super) fn add_unbound(&mut self) {
        self.may_be_unbound = true;
    }

    /// Add given constraint index to all definitions
    pub(super) fn add_constraint(&mut self, constraint_id: ScopedConstraintId) {
        self.constraints.insert_in_each(constraint_id.into());
    }

    /// Merge two [`ConstrainedDefinitions`].
    pub(super) fn merge(
        a: &ConstrainedDefinitions,
        b: &ConstrainedDefinitions,
    ) -> ConstrainedDefinitions {
        let mut ret = Self {
            visible_definitions: Definitions::default(),
            constraints: Constraints::default(),
            may_be_unbound: a.may_be_unbound || b.may_be_unbound,
        };
        let mut a_defs_iter = a.visible_definitions.iter();
        let mut b_defs_iter = b.visible_definitions.iter();
        let mut a_constraints_iter = a.constraints.iter();
        let mut b_constraints_iter = b.constraints.iter();

        let mut opt_a_def: Option<u32> = a_defs_iter.next();
        let mut opt_b_def: Option<u32> = b_defs_iter.next();

        // Iterate through the definitions from `a` and `b` in sync (always processing the lower
        // definition ID first), and pushing each definition onto the merged
        // `ConstrainedDefinitions` with its constraints. If a definition is found in both `a` and
        // `b`, push it with the intersection of the constraints from the two paths (a constraint
        // that applies from only one path is irrelevant.)

        let push = |def, constraints_iter: &mut ConstraintsIterator, ret: &mut Self| {
            ret.visible_definitions.insert(def);
            let Some(constraints) = constraints_iter.next() else {
                panic!("definitions and constraints length mismatch");
            };
            ret.constraints.push(constraints.clone());
        };

        loop {
            match (opt_a_def, opt_b_def) {
                (Some(a_def), Some(b_def)) => match a_def.cmp(&b_def) {
                    std::cmp::Ordering::Less => {
                        push(a_def, &mut a_constraints_iter, &mut ret);
                        opt_a_def = a_defs_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        push(b_def, &mut b_constraints_iter, &mut ret);
                        opt_b_def = b_defs_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        push(a_def, &mut a_constraints_iter, &mut ret);
                        let Some(b_constraints) = b_constraints_iter.next() else {
                            panic!("definitions and constraints length mismatch");
                        };
                        // If the same definition is visible through both paths, any constraint
                        // that applies on only one path is irrelevant to the type, so we intersect.
                        ret.constraints.last_mut().unwrap().intersect(b_constraints);
                        opt_a_def = a_defs_iter.next();
                        opt_b_def = b_defs_iter.next();
                    }
                },
                (Some(a_def), None) => {
                    push(a_def, &mut a_constraints_iter, &mut ret);
                    opt_a_def = a_defs_iter.next();
                }
                (None, Some(b_def)) => {
                    push(b_def, &mut b_constraints_iter, &mut ret);
                    opt_b_def = b_defs_iter.next();
                }
                (None, None) => break,
            }
        }
        ret
    }

    /// Get iterator over visible definitions with constraints.
    pub(super) fn iter_visible_definitions(&self) -> DefinitionIdWithConstraintsIterator {
        DefinitionIdWithConstraintsIterator {
            definitions: self.visible_definitions.iter(),
            constraints: self.constraints.iter(),
        }
    }

    /// Could the symbol be unbound?
    pub(super) fn may_be_unbound(&self) -> bool {
        self.may_be_unbound
    }
}

impl Default for ConstrainedDefinitions {
    fn default() -> Self {
        ConstrainedDefinitions::unbound()
    }
}

#[derive(Debug)]
pub(super) struct DefinitionIdWithConstraintsIterator<'a> {
    definitions: DefinitionsIterator<'a>,
    constraints: ConstraintsIterator<'a>,
}

impl<'a> Iterator for DefinitionIdWithConstraintsIterator<'a> {
    type Item = DefinitionIdWithConstraints<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.definitions.next(), self.constraints.next()) {
            (None, None) => None,
            (Some(def), Some(constraints)) => Some(DefinitionIdWithConstraints {
                definition: ScopedDefinitionId::from_u32(def),
                constraint_ids: ConstraintIdIterator {
                    wrapped: constraints.iter(),
                },
            }),
            _ => panic!("definitions and constraints length mismatch"),
        }
    }
}

impl std::iter::FusedIterator for DefinitionIdWithConstraintsIterator<'_> {}

#[derive(Debug)]
pub(super) struct ConstraintIdIterator<'a> {
    wrapped: BitSetIterator<'a, CONSTRAINT_BLOCKS>,
}

impl Iterator for ConstraintIdIterator<'_> {
    type Item = ScopedConstraintId;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped.next().map(ScopedConstraintId::from_u32)
    }
}

impl std::iter::FusedIterator for ConstraintIdIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::{ConstrainedDefinitions, ScopedConstraintId, ScopedDefinitionId};

    impl ConstrainedDefinitions {
        fn defs(&self) -> Vec<String> {
            self.iter_visible_definitions()
                .map(|def_id_with_constraints| {
                    format!(
                        "{}<{}>",
                        def_id_with_constraints.definition.as_u32(),
                        def_id_with_constraints
                            .constraint_ids
                            .map(ScopedConstraintId::as_u32)
                            .map(|idx| idx.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect()
        }
    }

    #[test]
    fn unbound() {
        let cd = ConstrainedDefinitions::unbound();

        assert!(cd.may_be_unbound());
        assert_eq!(cd.defs().len(), 0);
    }

    #[test]
    fn with() {
        let cd = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(0));

        assert!(!cd.may_be_unbound());
        assert_eq!(cd.defs(), &["0<>"]);
    }

    #[test]
    fn add_unbound() {
        let mut cd = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(0));
        cd.add_unbound();

        assert!(cd.may_be_unbound());
        assert_eq!(cd.defs(), &["0<>"]);
    }

    #[test]
    fn add_constraint() {
        let mut cd = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(0));
        cd.add_constraint(ScopedConstraintId::from_u32(0));

        assert!(!cd.may_be_unbound());
        assert_eq!(cd.defs(), &["0<0>"]);
    }

    #[test]
    fn merge() {
        // merging the same definition with the same constraint keeps the constraint
        let mut cd0a = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(0));
        cd0a.add_constraint(ScopedConstraintId::from_u32(0));

        let mut cd0b = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(0));
        cd0b.add_constraint(ScopedConstraintId::from_u32(0));

        let cd0 = ConstrainedDefinitions::merge(&cd0a, &cd0b);
        assert!(!cd0.may_be_unbound());
        assert_eq!(cd0.defs(), &["0<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut cd1a = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(1));
        cd1a.add_constraint(ScopedConstraintId::from_u32(1));

        let mut cd1b = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(1));
        cd1b.add_constraint(ScopedConstraintId::from_u32(2));

        let cd1 = ConstrainedDefinitions::merge(&cd1a, &cd1b);
        assert!(!cd1.may_be_unbound());
        assert_eq!(cd1.defs(), &["1<>"]);

        // merging a constrained definition with unbound keeps both
        let mut cd2a = ConstrainedDefinitions::with(ScopedDefinitionId::from_u32(2));
        cd2a.add_constraint(ScopedConstraintId::from_u32(3));

        let cd2b = ConstrainedDefinitions::unbound();

        let cd2 = ConstrainedDefinitions::merge(&cd2a, &cd2b);
        assert!(cd2.may_be_unbound());
        assert_eq!(cd2.defs(), &["2<3>"]);

        // merging different definitions keeps them each with their existing constraints
        let cd = ConstrainedDefinitions::merge(&cd0, &cd2);
        assert!(cd.may_be_unbound());
        assert_eq!(cd.defs(), &["0<0>", "2<3>"]);
    }
}
