use std::cell::RefCell;

use ruff_db::files::system_path_to_file;

use super::{ClassBaseConversion, ConversionEffects, sealed};
use crate::db::tests::TestDbBuilder;
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{
    ClassBase, ClassLiteral, DynamicType, IntersectionType, RecursivelyDefined, SpecialFormType,
    Type, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Clone, Copy)]
enum Stop {
    Never,
    BeforeChild(usize),
    AfterChild(usize),
    AfterDefault,
}

struct Recording<'db> {
    db: &'db dyn Db,
    children: RefCell<Vec<Type<'db>>>,
    stop: Stop,
}

impl<'db> Recording<'db> {
    fn new(db: &'db dyn Db, stop: Stop) -> Self {
        Self {
            db,
            children: RefCell::new(Vec::new()),
            stop,
        }
    }
}

impl sealed::Sealed for Recording<'_> {}

impl SourceReadControl for Recording<'_> {
    type Error = Incomplete;

    fn check(&self) -> Result<(), Incomplete> {
        expansion_probe::continue_work(self.db)
    }
}

impl<'db> ConversionEffects<'db> for Recording<'db> {
    fn default_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassBase<'db>, Incomplete> {
        let value = read_source(self, || ClassBase::Class(class.default_specialization(db)))?;
        if matches!(self.stop, Stop::AfterDefault) {
            expansion_probe::refuse(db, Incomplete::Interrupted);
        }
        Ok(value)
    }

    fn convert_child(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Result<Option<ClassBase<'db>>, Incomplete> {
        self.check()?;
        if matches!(self.stop, Stop::BeforeChild(index) if index == self.children.borrow().len()) {
            return Err(expansion_probe::refuse(db, Incomplete::Interrupted));
        }
        let index = self.children.borrow().len();
        self.children.borrow_mut().push(ty);
        let value = ClassBaseConversion::from_type(ty).resolve_with(db, env, subclass, self)?;
        if matches!(self.stop, Stop::AfterChild(stop) if stop == index) {
            expansion_probe::refuse(db, Incomplete::Interrupted);
        }
        Ok(value)
    }
}

#[test]
fn compound_conversion_preserves_provenance_order_and_refusal() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let any = Type::SpecialForm(SpecialFormType::Any);
    let protocol = Type::SpecialForm(SpecialFormType::Protocol);
    let cases = [
        (
            vec![Type::unknown(), any, protocol],
            Some(ClassBase::unknown()),
            3,
        ),
        (
            vec![Type::unknown(), Type::int_literal(1), protocol],
            None,
            2,
        ),
        (vec![any, protocol], None, 0),
    ];
    for (elements, expected, visited) in cases {
        let ty = Type::Union(UnionType::new(
            &db,
            elements.as_slice(),
            RecursivelyDefined::No,
        ));
        for stop in (0..visited)
            .flat_map(|index| [Stop::BeforeChild(index), Stop::AfterChild(index)])
            .chain([Stop::Never, Stop::Never])
        {
            let effects = Recording::new(&db, stop);
            let (outcome, _) = expansion_probe::run_mro(&db, 100_000, || {
                ClassBaseConversion::from_explicit_type(ty).resolve_with(&db, &env, None, &effects)
            });
            let count = match stop {
                Stop::BeforeChild(count) => {
                    assert_eq!(outcome, Err(Incomplete::Interrupted));
                    count
                }
                Stop::AfterChild(index) => {
                    assert_eq!(outcome, Err(Incomplete::Interrupted));
                    index + 1
                }
                Stop::Never => {
                    assert_eq!(outcome, Ok(Ok(expected)));
                    visited
                }
                Stop::AfterDefault => anyhow::bail!("unexpected default refusal"),
            };
            assert_eq!(*effects.children.borrow(), elements[..count]);
        }
    }
    let effects = Recording::new(&db, Stop::Never);
    let (outcome, _) = expansion_probe::run_mro(&db, 100_000, || {
        let explicit =
            ClassBaseConversion::from_explicit_type(any).resolve_with(&db, &env, None, &effects)?;
        let child = effects.convert_child(&db, &env, any, None)?;
        Ok::<_, Incomplete>((explicit, child))
    });
    assert_eq!(
        outcome,
        Ok(Ok((
            Some(ClassBase::Any),
            Some(ClassBase::Dynamic(DynamicType::Any))
        )))
    );
    Ok(())
}

#[test]
fn intersection_stops_conversion_at_the_first_valid_child() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let elements = [Type::int_literal(1), Type::unknown(), Type::Never];
    let ty = Type::Intersection(IntersectionType::new(
        &db,
        elements.into_iter().collect::<FxOrderSet<_>>(),
        NegativeIntersectionElements::Empty,
    ));
    let expected = ClassBaseConversion::from_type(ty).resolve(&db, &env, None);
    for stop in [
        Stop::BeforeChild(0),
        Stop::AfterChild(0),
        Stop::BeforeChild(1),
        Stop::AfterChild(1),
        Stop::Never,
    ] {
        let effects = Recording::new(&db, stop);
        let (outcome, _) = expansion_probe::run_mro(&db, 100_000, || {
            ClassBaseConversion::from_type(ty).resolve_with(&db, &env, None, &effects)
        });
        let visited = match stop {
            Stop::BeforeChild(count) => {
                assert_eq!(outcome, Err(Incomplete::Interrupted));
                count
            }
            Stop::AfterChild(index) => {
                assert_eq!(outcome, Err(Incomplete::Interrupted));
                index + 1
            }
            Stop::Never => {
                assert_eq!(outcome, Ok(Ok(expected)));
                2
            }
            Stop::AfterDefault => anyhow::bail!("unexpected default refusal"),
        };
        assert_eq!(*effects.children.borrow(), elements[..visited]);
    }
    Ok(())
}

#[test]
fn interrupted_default_result_cannot_be_published() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/base.pyi", "class Base: ...\n")
        .build()?;
    let env = db.program_environment();
    let file = db.program_file(system_path_to_file(&db, "/src/base.pyi")?);
    let ty = global_symbol(&db, file, "Base")
        .place
        .ignore_possibly_undefined()
        .ok_or_else(|| anyhow::anyhow!("missing Base"))?;
    assert!(matches!(ty, Type::ClassLiteral(_)));
    let expected = ClassBaseConversion::from_type(ty).resolve(&db, &env, None);
    for stop in [Stop::AfterDefault, Stop::Never, Stop::Never] {
        let effects = Recording::new(&db, stop);
        let (outcome, _) = expansion_probe::run_mro(&db, 100_000, || {
            let value = ClassBaseConversion::from_type(ty).resolve_with(&db, &env, None, &effects);
            if matches!(stop, Stop::AfterDefault) {
                assert_eq!(value, Err(Incomplete::Interrupted));
                assert_eq!(
                    ClassBaseConversion::from_type(Type::unknown())
                        .resolve_with(&db, &env, None, &effects),
                    Err(Incomplete::Interrupted)
                );
            }
            value
        });
        if matches!(stop, Stop::AfterDefault) {
            assert_eq!(outcome, Err(Incomplete::Interrupted));
        } else {
            assert_eq!(outcome, Ok(Ok(expected)));
        }
    }
    Ok(())
}
