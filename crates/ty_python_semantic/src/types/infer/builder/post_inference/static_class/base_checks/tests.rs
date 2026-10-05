use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::PythonVersion;
use ty_python_core::semantic_index;

use super::*;
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::ClassLiteral;
use crate::types::class::DisjointBaseKind;
use crate::types::class::base_entries::base_entry;
use crate::types::signatures::effects::try_poll_immediate;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event<'db> {
    Step(&'static str),
    Entry(usize),
    Source(usize),
    Missing(TextRange),
    NamedTuple(TextRange),
    ProtocolGeneric(TextRange, GenericContext<'db>, GenericContext<'db>),
    TypeParams(TextRange),
    Expression(TextRange),
    Unsupported(TextRange),
}

struct Recording<'a, 'db> {
    index: &'a SemanticIndex<'db>,
    types: &'a [Type<'db>],
    events: RefCell<Vec<Event<'db>>>,
    reject_at: Option<usize>,
}

impl<'db> Recording<'_, 'db> {
    fn record(&self, event: Event<'db>) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let position = events.len();
        events.push(event);
        if self.reject_at == Some(position) {
            Err("refused")
        } else {
            Ok(())
        }
    }
}

// Both drivers receive identical scripted answers and stop at the same selected effect.
// Ordinary mdtests cover the semantic providers and diagnostic contents.
macro_rules! recording_effects {
    ($(
        fn $name:ident $(<$node:lifetime>)? ($this:ident $(, $argument:ident: $ty:ty)*) -> $output:ty $body:block
    )*) => {
        impl<'db> SynchronousExplicitBaseCheckEffects<'db> for Recording<'_, 'db> {
            type Error = &'static str;
            $(
                fn $name $(<$node>)? (&$this $(, $argument: $ty)*) -> Result<$output, Self::Error> $body
            )*
        }

        impl<'db> ExplicitBaseCheckEffects<'db> for Recording<'_, 'db> {
            type Error = &'static str;
            $(
                async fn $name $(<$node>)? (&$this $(, $argument: $ty)*) -> Result<$output, Self::Error> {
                    SynchronousExplicitBaseCheckEffects::$name($this $(, $argument)*)
                }
            )*
        }
    };
}

recording_effects! {
    fn empty_disjoint_bases(self) -> IncompatibleBases<'db> {
        self.record(Event::Step("empty disjoint bases"))?;
        Ok(IncompatibleBases::default())
    }

    fn empty_typed_dict_bases(self) -> Vec<ClassType<'db>> {
        self.record(Event::Step("empty typed dict bases"))?;
        Ok(Vec::new())
    }

    fn class_definition(self, node: &ast::StmtClassDef) -> Definition<'db> {
        self.record(Event::Step("definition"))?;
        Ok(self.index.expect_single_definition(node))
    }

    fn expand<'node>(self, _class: StaticClassLiteral<'db>, node: &'node ast::StmtClassDef, _definition: Definition<'db>) -> Vec<ExpandedClassBaseEntry<'node, 'db>> {
        self.record(Event::Step("expand"))?;
        assert_eq!(node.bases().len(), self.types.len());
        Ok(node.bases().iter().zip(self.types).map(|(node, ty)| base_entry(node, *ty)).collect())
    }

    fn explicit_variance_enabled(self) -> bool {
        self.record(Event::Step("variance enabled"))?;
        Ok(true)
    }

    fn next_entry<'node>(self, entries: &[ExpandedClassBaseEntry<'node, 'db>], cursor: &mut usize) -> Option<(usize, ExpandedClassBaseEntry<'node, 'db>)> {
        self.record(Event::Entry(*cursor))?;
        Ok(next_expanded_base_entry(entries, cursor))
    }

    fn report_missing_arguments(self, _base: Type<'db>, node: &ast::Expr) -> () {
        self.record(Event::Missing(node.range()))
    }

    fn report_named_tuple(self, _class: StaticClassLiteral<'db>, node: &ast::Expr) -> () {
        self.record(Event::NamedTuple(node.range()))
    }

    fn report_plain_generic(self, _node: &ast::Expr) -> () {
        self.record(Event::Step("plain generic"))
    }

    fn report_protocol_and_generic(self, node: &ast::Expr, previous: GenericContext<'db>, new: GenericContext<'db>) -> () {
        self.record(Event::ProtocolGeneric(node.range(), previous, new))
    }

    fn report_protocol_and_type_params(self, node: &ast::Expr, _params: &ast::TypeParams) -> () {
        self.record(Event::TypeParams(node.range()))
    }

    fn check_variance(self, _class: StaticClassLiteral<'db>, _alias: GenericAlias<'db>, _node: &ast::Expr) -> () {
        Err("unexpected generic alias")
    }

    fn nearest_disjoint_base(self, base: ClassType<'db>) -> Option<DisjointBase<'db>> {
        self.record(Event::Step("nearest disjoint"))?;
        let ClassType::NonGeneric(class) = base else {
            return Err("unexpected generic alias");
        };
        Ok(Some(DisjointBase { class, kind: DisjointBaseKind::DefinesSlots }))
    }

    fn record_disjoint_base(self, bases: &mut IncompatibleBases<'db>, disjoint: DisjointBase<'db>, index: usize, base: ClassType<'db>) -> () {
        self.record(Event::Step("record disjoint"))?;
        let ClassType::NonGeneric(class) = base else {
            return Err("unexpected generic alias");
        };
        bases.insert(disjoint, index, class);
        Ok(())
    }

    fn check_base_kind(self, _class: StaticClassLiteral<'db>, base: ClassType<'db>, _node: &ast::Expr, _protocol: bool, _kind: Option<CodeGeneratorKind<'db>>, direct: &mut Vec<ClassType<'db>>) -> () {
        self.record(Event::Step("base kind"))?;
        direct.push(base);
        Ok(())
    }

    fn is_final(self, _base: ClassType<'db>) -> bool {
        self.record(Event::Step("final"))?;
        Ok(false)
    }

    fn report_final(self, _class: StaticClassLiteral<'db>, _base: ClassType<'db>, _node: &ast::Expr) -> () {
        Err("unexpected final diagnostic")
    }

    fn static_class_literal(self, _base: ClassType<'db>) -> Option<StaticClassLiteral<'db>> {
        self.record(Event::Step("static class"))?;
        Ok(None)
    }

    fn is_frozen_dataclass(self, _class: StaticClassLiteral<'db>) -> Option<bool> {
        Err("unexpected frozen query")
    }

    fn report_frozen(self, _class: StaticClassLiteral<'db>, _node: &ast::StmtClassDef, _base: StaticClassLiteral<'db>, _source: &ast::Expr, _frozen: bool) -> () {
        Err("unexpected frozen diagnostic")
    }

    fn ordered_dataclass_base(self, _base: ClassType<'db>) -> Option<ClassType<'db>> {
        self.record(Event::Step("ordered"))?;
        Ok(None)
    }

    fn has_own_comparison_methods(self, _class: StaticClassLiteral<'db>) -> bool {
        Err("unexpected comparison query")
    }

    fn report_ordered(self, _class: StaticClassLiteral<'db>, _base: ClassType<'db>, _node: &ast::Expr) -> () {
        Err("unexpected ordered diagnostic")
    }

    fn next_source_base<'node>(self, node: &'node ast::StmtClassDef, cursor: &mut usize) -> Option<&'node ast::Expr> {
        self.record(Event::Source(*cursor))?;
        Ok(next_class_source_base(node, cursor))
    }

    fn expression_type(self, _definition: Definition<'db>, node: &ast::Expr) -> Type<'db> {
        self.record(Event::Expression(node.range()))?;
        Ok(Type::unknown())
    }

    fn is_variable_length_tuple(self, _ty: Type<'db>) -> bool {
        self.record(Event::Step("variable tuple"))?;
        Ok(true)
    }

    fn report_unsupported(self, _class: StaticClassLiteral<'db>, node: &ast::Expr, _ty: Type<'db>) -> () {
        self.record(Event::Unsupported(node.range()))
    }
}

#[test]
fn retained_bases_and_protocol_context_preserve_order_and_refusal_prefixes() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/base_checks.py",
            r#"
class Plain: ...
class Other: ...
class First[T]: ...
class Second[U]: ...
class Empty: ...
class Mixed(first, second, generic, ordinary, plain, *starred): ...
class Pep695[T](first, second, generic, ordinary, plain, *starred): ...
"#,
        )
        .build()?;
    let file = db.program_file(system_path_to_file(&db, "/src/base_checks.py")?);
    let class = |name| {
        global_symbol(&db, file, name)
            .place
            .ignore_possibly_undefined()
            .and_then(Type::as_class_literal)
            .and_then(|class| class.as_static())
            .ok_or_else(|| anyhow::anyhow!("missing {name} class"))
    };
    let subject = class("Plain")?;
    let other = ClassLiteral::Static(class("Other")?);
    let first = class("First")?
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing First context"))?;
    let second = class("Second")?
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing Second context"))?;
    assert_ne!(first, second);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let mixed_types = [
        Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(first)),
        Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(second)),
        Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(second)),
        Type::ClassLiteral(other),
        Type::SpecialForm(SpecialFormType::Generic),
        Type::unknown(),
    ];

    for (name, named_tuple) in [
        ("Empty", false),
        ("Mixed", false),
        ("Mixed", true),
        ("Pep695", false),
    ] {
        let node = module
            .suite()
            .iter()
            .find_map(|statement| match statement {
                ast::Stmt::ClassDef(node) if node.name.as_str() == name => Some(node),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("missing {name} node"))?;
        let types = if name == "Empty" {
            &[][..]
        } else {
            &mixed_types[..]
        };
        let kind = named_tuple.then_some(CodeGeneratorKind::NamedTuple);
        let mut expected = vec![
            Event::Step("empty disjoint bases"),
            Event::Step("empty typed dict bases"),
            Event::Step("definition"),
            Event::Step("expand"),
            Event::Step("variance enabled"),
        ];
        if let [first_node, second_node, generic, ordinary, plain, starred] = node.bases() {
            expected.extend([Event::Entry(0), Event::Missing(first_node.range())]);
            if named_tuple {
                expected.push(Event::NamedTuple(first_node.range()));
            }
            if name == "Pep695" {
                expected.push(Event::TypeParams(first_node.range()));
            }
            expected.extend([Event::Entry(1), Event::Missing(second_node.range())]);
            if named_tuple {
                expected.push(Event::NamedTuple(second_node.range()));
            }
            if name == "Pep695" {
                expected.push(Event::TypeParams(second_node.range()));
            }
            expected.extend([Event::Entry(2), Event::Missing(generic.range())]);
            if name != "Pep695" {
                expected.push(Event::ProtocolGeneric(first_node.range(), first, second));
            }
            expected.extend([Event::Entry(3), Event::Missing(ordinary.range())]);
            if named_tuple {
                expected.push(Event::NamedTuple(ordinary.range()));
            }
            expected.extend([
                Event::Step("nearest disjoint"),
                Event::Step("record disjoint"),
                Event::Step("base kind"),
                Event::Step("final"),
                Event::Step("static class"),
                Event::Step("ordered"),
                Event::Entry(4),
                Event::Missing(plain.range()),
            ]);
            if named_tuple {
                expected.push(Event::NamedTuple(plain.range()));
            }
            expected.extend([
                Event::Step("plain generic"),
                Event::Entry(5),
                Event::Missing(starred.range()),
            ]);
            if named_tuple {
                expected.push(Event::NamedTuple(starred.range()));
            }
            expected.push(Event::Entry(6));
            expected.extend((0..6).map(Event::Source));
            let ast::Expr::Starred(starred_expr) = starred else {
                anyhow::bail!("expected a starred source base");
            };
            expected.extend([
                Event::Expression(starred_expr.value.range()),
                Event::Step("variable tuple"),
                Event::Unsupported(starred.range()),
                Event::Source(6),
            ]);
        } else {
            assert!(node.bases().is_empty());
            expected.extend([Event::Entry(0), Event::Source(0)]);
        }

        for reject_at in (0..expected.len()).map(Some).chain([None]) {
            let recording = || Recording {
                index: &index,
                types,
                events: RefCell::default(),
                reject_at,
            };
            let synchronous = recording();
            let asynchronous = recording();
            let sync_result = check_explicit_bases_sync(
                subject,
                node,
                kind,
                false,
                ExplicitBaseCheckFacts,
                &synchronous,
            );
            let Poll::Ready(async_result) = try_poll_immediate(check_explicit_bases_with(
                subject,
                node,
                kind,
                false,
                ExplicitBaseCheckFacts,
                &asynchronous,
            )) else {
                anyhow::bail!("recording effects unexpectedly suspended");
            };
            let end = reject_at.map_or(expected.len(), |index| index + 1);
            assert_eq!(*synchronous.events.borrow(), expected[..end]);
            assert_eq!(*asynchronous.events.borrow(), expected[..end]);
            for result in [sync_result, async_result] {
                if reject_at.is_some() {
                    assert!(matches!(result, Err("refused")));
                } else {
                    let bases = result.map_err(anyhow::Error::msg)?;
                    let entries: Vec<_> = bases
                        .expanded_entries
                        .iter()
                        .map(|entry| (entry.source_node().range(), entry.ty()))
                        .collect();
                    let expected_entries: Vec<_> = node
                        .bases()
                        .iter()
                        .zip(types)
                        .map(|(node, ty)| (node.range(), *ty))
                        .collect();
                    assert_eq!(entries, expected_entries);
                    assert_eq!(bases.disjoint_bases.len(), usize::from(!types.is_empty()));
                    let expected_direct = if types.is_empty() {
                        vec![]
                    } else {
                        vec![ClassType::NonGeneric(other)]
                    };
                    assert_eq!(bases.direct_typed_dict_bases, expected_direct);
                    for (entry, source) in bases.expanded_entries.iter().zip(node.bases()) {
                        assert!(std::ptr::eq(entry.source_node(), source));
                    }
                }
            }
        }
    }
    Ok(())
}
