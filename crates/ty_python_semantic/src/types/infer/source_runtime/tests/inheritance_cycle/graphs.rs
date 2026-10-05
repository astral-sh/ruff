use std::convert::Infallible;

use super::*;
use crate::types::class::static_literal::inheritance_cycle::{
    Enter, Step, SynchronousCycleTraversalEffects, Traversal, inheritance_cycle_inner_sync,
};
use crate::types::class::static_literal::{
    SynchronousInheritanceCycleEffects, inheritance_cycle_sync,
};
use crate::types::class::{DynamicClassAnchor, DynamicClassLiteral, DynamicClassScopeOffset};

struct Graph<'db> {
    db: &'db TestDb,
    classes: &'db [StaticClassLiteral<'db>],
    rows: &'db [Vec<Type<'db>>],
    expanded: RefCell<Vec<usize>>,
    examined: RefCell<Vec<Option<usize>>>,
}

impl Graph<'_> {
    fn index(&self, class: StaticClassLiteral<'_>) -> usize {
        self.classes
            .iter()
            .position(|candidate| *candidate == class)
            .unwrap()
    }
}

impl<'db> SynchronousCycleTraversalEffects<'db> for Graph<'db> {
    type Error = Infallible;

    fn start(&self) -> Result<Traversal<'db>, Infallible> {
        Ok(Traversal::default())
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        let index = self.index(class);
        self.expanded.borrow_mut().push(index);
        Ok(self.rows[index].as_slice())
    }

    fn base_class(&self, base: Type<'db>) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        let class = match base {
            Type::ClassLiteral(class) => class.as_static(),
            Type::GenericAlias(alias) => Some(alias.origin(self.db)),
            _ => None,
        };
        self.examined
            .borrow_mut()
            .push(class.map(|class| self.index(class)));
        Ok(class)
    }

    fn push(
        &self,
        traversal: &mut Traversal<'db>,
        bases: &'db [Type<'db>],
        introduced_base: bool,
    ) -> Result<(), Infallible> {
        traversal.push(bases, introduced_base);
        Ok(())
    }

    fn next(&self, traversal: &mut Traversal<'db>) -> Result<Option<Step<'db>>, Infallible> {
        Ok(traversal.next())
    }

    fn enter(
        &self,
        traversal: &mut Traversal<'db>,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(matches!(traversal.enter(class), Enter::Descend))
    }

    fn classify(
        &self,
        traversal: &Traversal<'db>,
        root: StaticClassLiteral<'db>,
    ) -> Result<Option<InheritanceCycle>, Infallible> {
        assert!(traversal.frames.is_empty());
        assert!(traversal.active.is_empty());
        Ok(traversal.classify(root))
    }
}

impl<'db> SynchronousInheritanceCycleEffects<'db> for Graph<'db> {
    type Error = Infallible;

    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(!self.rows[self.index(class)].is_empty())
    }

    fn inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<InheritanceCycle>, Infallible> {
        inheritance_cycle_inner_sync(class, self)
    }
}

fn classes<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> [StaticClassLiteral<'db>; 5] {
    ["Product", "First", "Second", "Third", "Generic"]
        .map(|name| class_in_file(db, prepared.program_file(), name))
}

fn fixture() -> TestDb {
    database(
        "class Product: ...\nclass First: ...\nclass Second: ...\nclass Third: ...\nclass Generic[T]: ...\n",
    )
}

/// Ordered graph fixtures verify cycle classification and which bases the shared DFS examines.
/// Source classes supply identities only; each row replaces that class's explicit bases for this reducer test.
/// Rows and edge indices follow `classes`; index zero is Product, the requested root.
#[test]
fn ordered_graphs_preserve_dfs_branch_order() {
    type Case = (
        &'static str,
        [Vec<usize>; 5],
        Option<InheritanceCycle>,
        Vec<usize>,
        Vec<usize>,
    );
    let cases: [Case; 8] = [
        (
            "empty",
            [vec![], vec![], vec![], vec![], vec![]],
            None,
            vec![0],
            vec![],
        ),
        (
            "chain",
            [vec![1], vec![2], vec![], vec![], vec![]],
            None,
            vec![0, 1, 2],
            vec![1, 2],
        ),
        (
            "self",
            [vec![0], vec![], vec![], vec![], vec![]],
            Some(InheritanceCycle::Participant),
            vec![0, 0],
            vec![0, 0],
        ),
        (
            "two participants",
            [vec![1], vec![0], vec![], vec![], vec![]],
            Some(InheritanceCycle::Participant),
            vec![0, 1, 0],
            vec![1, 0, 1],
        ),
        (
            "three participants",
            [vec![1], vec![2], vec![0], vec![], vec![]],
            Some(InheritanceCycle::Participant),
            vec![0, 1, 2, 0],
            vec![1, 2, 0, 1],
        ),
        (
            "inherited",
            [vec![1], vec![2], vec![1], vec![], vec![]],
            Some(InheritanceCycle::Inherited),
            vec![0, 1, 2],
            vec![1, 2, 1],
        ),
        (
            "diamond",
            [vec![1, 2], vec![3], vec![3], vec![], vec![]],
            None,
            vec![0, 1, 3, 2],
            vec![1, 3, 2, 3],
        ),
        (
            "later root path",
            [vec![1, 3], vec![2], vec![1], vec![0], vec![]],
            Some(InheritanceCycle::Participant),
            vec![0, 1, 2, 3, 0],
            vec![1, 2, 1, 3, 0, 1, 3],
        ),
    ];
    let db = fixture();
    let prepared = prepare(&db);
    let classes = classes(&db, &prepared);
    for (name, edges, expected, expanded, examined) in cases {
        let rows = edges.map(|row| {
            row.into_iter()
                .map(|index| Type::ClassLiteral(ClassLiteral::Static(classes[index])))
                .collect()
        });
        let graph = Graph {
            db: &db,
            classes: &classes,
            rows: &rows,
            expanded: RefCell::default(),
            examined: RefCell::default(),
        };
        assert_eq!(
            inheritance_cycle_inner_sync(classes[0], &graph),
            Ok(expected),
            "{name}"
        );
        assert_eq!(*graph.expanded.borrow(), expanded, "{name}");
        assert_eq!(
            *graph.examined.borrow(),
            examined.into_iter().map(Some).collect::<Vec<_>>(),
            "{name}"
        );
        assert_missing(&db, classes[0]);
    }
}

/// A repeated active base skips its frame's remaining bases while the parent resumes its next branch.
#[test]
fn active_repeat_skips_only_the_current_frames_remaining_bases() {
    let db = fixture();
    let prepared = prepare(&db);
    let classes = classes(&db, &prepared);
    let rows = [vec![1, 3], vec![2], vec![1, 4], vec![], vec![]].map(|row| {
        row.into_iter()
            .map(|index| Type::ClassLiteral(ClassLiteral::Static(classes[index])))
            .collect()
    });
    let graph = Graph {
        db: &db,
        classes: &classes,
        rows: &rows,
        expanded: RefCell::default(),
        examined: RefCell::default(),
    };
    assert_eq!(
        inheritance_cycle_inner_sync(classes[0], &graph),
        Ok(Some(InheritanceCycle::Inherited))
    );
    assert_eq!(*graph.expanded.borrow(), [0, 1, 2, 3]);
    assert_eq!(
        *graph.examined.borrow(),
        [Some(1), Some(2), Some(1), Some(3)]
    );
    assert_missing(&db, classes[0]);
}

/// Generic aliases expand their origin; dynamic classes and non-class bases are ignored without changing order.
#[test]
fn aliases_and_non_class_bases_preserve_the_selected_origin() {
    let db = fixture();
    let prepared = prepare(&db);
    let classes = classes(&db, &prepared);
    let context = classes[4].generic_context(&db).unwrap();
    let alias = GenericAlias::new(
        &db,
        classes[4],
        context.specialize(&db, &[Type::int_literal(1)]),
    );
    let dynamic = DynamicClassLiteral::new(
        &db,
        "Dynamic",
        DynamicClassAnchor::ScopeOffset {
            scope: classes[0].body_scope(&db),
            offset: DynamicClassScopeOffset::Node(0),
            explicit_bases: Box::from([Type::ClassLiteral(ClassLiteral::Static(classes[0]))]),
        },
        Box::default(),
        false,
        None,
    );
    let rows = [
        vec![
            Type::unknown(),
            Type::GenericAlias(alias),
            Type::int_literal(1),
            Type::ClassLiteral(ClassLiteral::Dynamic(dynamic)),
            Type::ClassLiteral(ClassLiteral::Static(classes[1])),
        ],
        vec![],
        vec![],
        vec![],
        vec![],
    ];
    let graph = Graph {
        db: &db,
        classes: &classes,
        rows: &rows,
        expanded: RefCell::default(),
        examined: RefCell::default(),
    };
    assert_eq!(inheritance_cycle_inner_sync(classes[0], &graph), Ok(None));
    assert_eq!(*graph.expanded.borrow(), [0, 4, 1]);
    assert_eq!(
        *graph.examined.borrow(),
        [None, Some(4), None, None, Some(1)]
    );
    assert_missing(&db, classes[0]);
}

/// The shared outer wrapper returns None for a class with no explicit bases without requesting the inner inheritance-cycle query.
#[test]
fn no_bases_wrapper_skips_the_cycle_query() {
    let db = fixture();
    let prepared = prepare(&db);
    let classes = classes(&db, &prepared);
    let rows = std::array::from_fn::<_, 5, _>(|_| Vec::new());
    let graph = Graph {
        db: &db,
        classes: &classes,
        rows: &rows,
        expanded: RefCell::default(),
        examined: RefCell::default(),
    };
    assert_eq!(inheritance_cycle_sync(classes[0], &graph), Ok(None));
    assert!(graph.expanded.borrow().is_empty());
    assert!(graph.examined.borrow().is_empty());
    assert_missing(&db, classes[0]);
}
