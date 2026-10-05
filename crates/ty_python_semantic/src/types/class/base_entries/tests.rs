use std::borrow::Cow;
use std::cell::RefCell;
use std::convert::Infallible;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ruff_python_ast::PythonVersion;
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::Definition;
use ty_python_core::{ProgramFile, semantic_index};

use super::{
    ClassBaseEntryEffects, ClassBaseEntryWork, InlineClassBaseEntryEffects,
    expanded_class_base_entries_with, sealed,
};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::tuple::TupleSpec;
use crate::types::{KnownClass, Type};

const SOURCE: &str = r#"
class A: ...
class B: ...
class C: ...
pair = (A, B)
variable: tuple[type[A], ...]

class Direct(A, B): ...
class Literal(*(A, B), C): ...
class Named(*pair): ...
class NestedScan(*(A, *(B,), C)): ...
class NestedLength(*(*(A, B),)): ...
class Variable(*variable): ...
class NonTuple(*1): ...
class Empty(*()): ...
class Growth(*(A, B, A, B, C)): ...
"#;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/base_entries.py", SOURCE)
        .build()
}

fn file(db: &TestDb) -> anyhow::Result<ProgramFile<'_>> {
    let env = db.program_environment();
    Ok(ProgramFile::new(
        db,
        system_path_to_file(db, "/src/base_entries.py")?,
        env.program(db),
    ))
}

fn class<'a>(module: &'a ParsedModuleRef, name: &str) -> anyhow::Result<&'a ast::StmtClassDef> {
    module
        .suite()
        .iter()
        .find_map(|stmt| match stmt {
            ast::Stmt::ClassDef(class) if class.name.as_str() == name => Some(class),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn source(range: TextRange) -> &'static str {
    &SOURCE[usize::from(range.start())..usize::from(range.end())]
}

#[test]
fn expansion_preserves_raw_types_and_source_associations() -> anyhow::Result<()> {
    let db = database()?;
    let file = file(&db)?;
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let a = global_symbol(&db, file, "A").place.expect_type();
    let b = global_symbol(&db, file, "B").place.expect_type();
    let c = global_symbol(&db, file, "C").place.expect_type();
    let cases = [
        ("Direct", vec![("A", a), ("B", b)]),
        ("Literal", vec![("A", a), ("B", b), ("C", c)]),
        ("Named", vec![("*pair", a), ("*pair", b)]),
        (
            "NestedScan",
            vec![
                ("*(A, *(B,), C)", a),
                ("*(A, *(B,), C)", b),
                ("*(A, *(B,), C)", c),
            ],
        ),
        ("NestedLength", vec![("*(*(A, B),)", a), ("*(*(A, B),)", b)]),
        ("Variable", vec![("*variable", Type::unknown())]),
        ("NonTuple", vec![("*1", Type::unknown())]),
        ("Empty", vec![]),
    ];
    for (name, expected) in cases {
        let node = class(&module, name)?;
        let definition = semantic_index(&db, file).expect_single_definition(node);
        let entries = infallible(expanded_class_base_entries_with(
            None,
            node,
            definition,
            &InlineClassBaseEntryEffects::new(&db),
        ));
        let actual: Vec<_> = entries
            .into_iter()
            .map(|entry| (source(entry.source_node().range()), entry.ty()))
            .collect();
        assert_eq!(actual, expected, "{name}");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Work(ClassBaseEntryWork),
    ExpressionBefore(TextRange),
    ExpressionAfter(TextRange),
    TupleBefore,
    TupleAfter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Refused(usize);

struct Recording<'db> {
    inline: InlineClassBaseEntryEffects<'db>,
    events: RefCell<Vec<Event>>,
    refusal: Option<usize>,
}

impl<'db> Recording<'db> {
    fn new(db: &'db dyn Db, refusal: Option<usize>) -> Self {
        Self {
            inline: InlineClassBaseEntryEffects::new(db),
            events: RefCell::new(Vec::new()),
            refusal,
        }
    }

    fn record(&self, event: Event) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refusal == Some(index) {
            Err(Refused(index))
        } else {
            Ok(())
        }
    }
}

impl sealed::Sealed for Recording<'_> {}

impl<'db> ClassBaseEntryEffects<'db> for Recording<'db> {
    type Error = Refused;

    fn checkpoint(&self, work: ClassBaseEntryWork) -> Result<(), Refused> {
        self.record(Event::Work(work))
    }

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Refused> {
        self.record(Event::ExpressionBefore(expression.range()))?;
        let ty = infallible(self.inline.expression_type(definition, expression));
        self.record(Event::ExpressionAfter(expression.range()))?;
        Ok(ty)
    }

    fn tuple_spec(
        &self,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Refused> {
        self.record(Event::TupleBefore)?;
        let tuple = infallible(self.inline.tuple_spec(definition, ty));
        self.record(Event::TupleAfter)?;
        Ok(tuple)
    }
}

#[test]
fn direct_and_starred_dependencies_preserve_order_and_short_circuit_tuple_scans()
-> anyhow::Result<()> {
    let db = database()?;
    let file = file(&db)?;
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    for (name, expressions, tuple_reads, source_scans, tuple_elements) in [
        ("Direct", vec!["A", "B"], 0, 0, 0),
        ("Literal", vec!["(A, B)", "C"], 1, 2, 2),
        ("Named", vec!["pair"], 1, 0, 2),
        ("NestedScan", vec!["(A, *(B,), C)"], 1, 2, 3),
        ("NestedLength", vec!["(*(A, B),)"], 1, 0, 2),
        ("Variable", vec!["variable"], 1, 0, 0),
        ("NonTuple", vec!["1"], 1, 0, 0),
        ("Empty", vec!["()"], 1, 0, 0),
    ] {
        let node = class(&module, name)?;
        let definition = semantic_index(&db, file).expect_single_definition(node);
        let effects = Recording::new(&db, None);
        assert!(expanded_class_base_entries_with(None, node, definition, &effects).is_ok());
        let events = effects.events.into_inner();
        let actual: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::ExpressionBefore(range) => Some(source(*range)),
                _ => None,
            })
            .collect();
        assert_eq!(actual, expressions, "{name}");
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::TupleBefore))
                .count(),
            tuple_reads,
            "{name}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Work(ClassBaseEntryWork::TupleSource { .. })
                ))
                .count(),
            source_scans,
            "{name}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Work(ClassBaseEntryWork::TupleElement { .. })
                ))
                .count(),
            tuple_elements,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn not_implemented_skips_inference_and_allocation() -> anyhow::Result<()> {
    let db = database()?;
    let file = file(&db)?;
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let node = class(&module, "Literal")?;
    let definition = semantic_index(&db, file).expect_single_definition(node);
    let effects = Recording::new(&db, None);
    let result = expanded_class_base_entries_with(
        Some(KnownClass::NotImplementedType),
        node,
        definition,
        &effects,
    );
    assert!(matches!(result, Ok(entries) if entries.is_empty()));
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Work(ClassBaseEntryWork::Begin),
            Event::Work(ClassBaseEntryWork::Publish { len: 0 })
        ]
    );
    Ok(())
}

#[test]
fn every_refusal_stops_before_later_source_reads_growth_append_or_publication() -> anyhow::Result<()>
{
    let db = database()?;
    let file = file(&db)?;
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    for name in ["Direct", "Literal", "NestedScan", "Variable", "Growth"] {
        let node = class(&module, name)?;
        let definition = semantic_index(&db, file).expect_single_definition(node);
        let baseline = Recording::new(&db, None);
        assert!(expanded_class_base_entries_with(None, node, definition, &baseline).is_ok());
        let expected = baseline.events.into_inner();
        if name == "Growth" {
            assert!(expected.iter().any(|event| matches!(event, Event::Work(ClassBaseEntryWork::Capacity { prefix_len, .. }) if *prefix_len > 0)));
        }
        for index in 0..expected.len() {
            let effects = Recording::new(&db, Some(index));
            let result = expanded_class_base_entries_with(None, node, definition, &effects);
            assert!(
                matches!(result, Err(reason) if reason == Refused(index)),
                "{name}: {index}"
            );
            assert_eq!(
                *effects.events.borrow(),
                expected[..=index],
                "{name}: {index}"
            );
        }
    }
    Ok(())
}
