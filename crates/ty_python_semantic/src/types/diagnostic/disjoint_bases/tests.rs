use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;

use super::*;
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::class::DisjointBaseKind;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{ClassLiteral, Type};

struct Recording<'db> {
    comparisons: RefCell<Vec<(DisjointBase<'db>, DisjointBase<'db>)>>,
    subtype_pair: (DisjointBase<'db>, DisjointBase<'db>),
    refuse_at: Option<usize>,
}

macro_rules! recording_effects {
    ($(
        fn $name:ident ($this:ident $(, $argument:ident: $ty:ty)*) -> $output:ty $body:block
    )*) => {
        impl<'db> SynchronousDisjointBaseEffects<'db> for Recording<'db> {
            type Error = &'static str;
            $(
                fn $name (&$this $(, $argument: $ty)*) -> Result<$output, Self::Error> $body
            )*
        }

        impl<'db> DisjointBaseEffects<'db> for Recording<'db> {
            type Error = &'static str;
            $(
                async fn $name (&$this $(, $argument: $ty)*) -> Result<$output, Self::Error> {
                    SynchronousDisjointBaseEffects::$name($this $(, $argument)*)
                }
            )*
        }
    };
}

recording_effects! {
    fn empty_retained_bases(self) -> IncompatibleBases<'db> {
        Ok(IncompatibleBases::default())
    }

    fn next_disjoint_base(self, bases: &IncompatibleBases<'db>, cursor: &mut usize) -> Option<(DisjointBase<'db>, IncompatibleBaseInfo<'db>)> {
        Ok(next_disjoint_base_entry(bases, cursor))
    }

    fn is_layout_subtype(self, base: DisjointBase<'db>, other: DisjointBase<'db>) -> bool {
        let mut comparisons = self.comparisons.borrow_mut();
        let position = comparisons.len();
        comparisons.push((base, other));
        if self.refuse_at == Some(position) {
            return Err("refused");
        }
        Ok((base, other) == self.subtype_pair)
    }

    fn retain_disjoint_base(self, retained: &mut IncompatibleBases<'db>, base: DisjointBase<'db>, info: IncompatibleBaseInfo<'db>) -> () {
        retained.0.insert(base, info);
        Ok(())
    }

    fn replace_disjoint_bases(self, bases: &mut IncompatibleBases<'db>, retained: &mut IncompatibleBases<'db>) -> () {
        std::mem::swap(bases, retained);
        Ok(())
    }
}

fn entries<'db>(
    bases: &IncompatibleBases<'db>,
) -> Vec<(DisjointBase<'db>, usize, ClassLiteral<'db>)> {
    bases
        .into_iter()
        .map(|(base, info)| (*base, info.node_index, info.originating_base))
        .collect()
}

#[test]
fn pruning_preserves_comparison_order_and_original_state_on_refusal() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file(
            "/src/disjoint_bases.py",
            "class A: ...\nclass B: ...\nclass C: ...\n",
        )
        .build()?;
    let file = db.program_file(system_path_to_file(&db, "/src/disjoint_bases.py")?);
    let base = |name| {
        let class = global_symbol(&db, file, name)
            .place
            .ignore_possibly_undefined()
            .and_then(Type::as_class_literal)
            .ok_or_else(|| anyhow::anyhow!("missing {name} class"))?;
        Ok::<_, anyhow::Error>(DisjointBase {
            class,
            kind: DisjointBaseKind::DefinesSlots,
        })
    };
    let [a, b, c] = [base("A")?, base("B")?, base("C")?];
    let original = [(a, 5, c.class), (b, 9, a.class), (c, 13, b.class)];
    let make_bases = || {
        let mut bases = IncompatibleBases::default();
        for (base, index, class) in original {
            bases.insert(base, index, class);
        }
        bases
    };

    // B is removed as soon as A matches, but C still compares against B in the original map.
    let expected = [(a, b), (a, c), (b, a), (c, a), (c, b)];
    for refuse_at in (0..expected.len()).map(Some).chain([None]) {
        let recording = || Recording {
            comparisons: RefCell::default(),
            subtype_pair: (b, a),
            refuse_at,
        };
        let synchronous = recording();
        let asynchronous = recording();
        let mut sync_bases = make_bases();
        let mut async_bases = make_bases();
        let sync_result =
            prune_disjoint_bases_sync(&mut sync_bases, DisjointBaseFacts, &synchronous);
        let Poll::Ready(async_result) = try_poll_immediate(prune_disjoint_bases_with(
            &mut async_bases,
            DisjointBaseFacts,
            &asynchronous,
        )) else {
            anyhow::bail!("recording effects unexpectedly suspended");
        };
        let end = refuse_at.map_or(expected.len(), |index| index + 1);
        assert_eq!(*synchronous.comparisons.borrow(), expected[..end]);
        assert_eq!(*asynchronous.comparisons.borrow(), expected[..end]);
        for (result, bases) in [(sync_result, sync_bases), (async_result, async_bases)] {
            if refuse_at.is_some() {
                assert_eq!(result, Err("refused"));
                assert_eq!(entries(&bases), original);
            } else {
                result.map_err(anyhow::Error::msg)?;
                assert_eq!(entries(&bases), [original[0], original[2]]);
            }
        }
    }
    Ok(())
}
