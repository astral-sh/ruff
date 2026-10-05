use std::cell::{Cell, RefCell};
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;

use super::{ApplyClassSpecializationEffects, apply_class_specialization_with};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, GenericContext, Specialization, StaticClassLiteral, Type,
};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/apply.py", "class Plain: ...\nclass Generic[T]: ...\n")
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = system_path_to_file(db, "/src/apply.py")?;
    global_symbol(db, db.program_file(file), name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation<'db> {
    Context,
    Specialize(GenericContext<'db>),
    Alias,
}

struct Observed<'db> {
    db: &'db dyn Db,
    context: Option<GenericContext<'db>>,
    operations: RefCell<Vec<Operation<'db>>>,
    refuse: Option<Operation<'db>>,
}

impl<'db> Observed<'db> {
    fn record(&self, operation: Operation<'db>) -> Result<(), Operation<'db>> {
        self.operations.borrow_mut().push(operation);
        if self.refuse == Some(operation) {
            Err(operation)
        } else {
            Ok(())
        }
    }
}

impl<'db> ApplyClassSpecializationEffects<'db, Specialization<'db>> for Observed<'db> {
    type Error = Operation<'db>;

    async fn generic_context(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        self.record(Operation::Context)?;
        Ok(self.context)
    }

    async fn specialize(
        &self,
        context: GenericContext<'db>,
        input: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.record(Operation::Specialize(context))?;
        Ok(input)
    }

    async fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.record(Operation::Alias)?;
        Ok(ClassType::Generic(GenericAlias::new(
            self.db,
            class,
            specialization,
        )))
    }
}

#[test]
fn context_lookup_precedes_callback_and_alias() -> anyhow::Result<()> {
    let db = database()?;
    let class = class(&db, "Generic")?;
    let context = class
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
    let specialization = context.specialize(&db, &[Type::bool_literal(true)]);
    let mut effects = Observed {
        db: &db,
        context: None,
        operations: RefCell::default(),
        refuse: None,
    };
    assert_eq!(
        try_poll_immediate(apply_class_specialization_with(
            class,
            specialization,
            &effects
        )),
        Poll::Ready(Ok(ClassType::NonGeneric(ClassLiteral::Static(class)))),
    );
    assert_eq!(*effects.operations.borrow(), [Operation::Context]);

    effects.context = Some(context);
    effects.operations.borrow_mut().clear();
    let expected = ClassType::Generic(GenericAlias::new(&db, class, specialization));
    assert_eq!(
        try_poll_immediate(apply_class_specialization_with(
            class,
            specialization,
            &effects
        )),
        Poll::Ready(Ok(expected)),
    );
    let operations = [
        Operation::Context,
        Operation::Specialize(context),
        Operation::Alias,
    ];
    assert_eq!(*effects.operations.borrow(), operations);
    for (index, operation) in operations.into_iter().enumerate() {
        effects.refuse = Some(operation);
        effects.operations.borrow_mut().clear();
        assert_eq!(
            try_poll_immediate(apply_class_specialization_with(
                class,
                specialization,
                &effects
            )),
            Poll::Ready(Err(operation)),
        );
        assert_eq!(*effects.operations.borrow(), operations[..=index]);
    }
    Ok(())
}

#[test]
fn ordinary_callback_is_only_called_for_generic_classes() -> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db, "Plain")?;
    let generic = class(&db, "Generic")?;
    let context = generic
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("missing generic context"))?;
    let specialization = context.specialize(&db, &[Type::bool_literal(true)]);
    let calls = Cell::new(0);
    let actual = plain.apply_specialization(&db, |_| {
        calls.set(calls.get() + 1);
        specialization
    });
    assert_eq!(actual, ClassType::NonGeneric(ClassLiteral::Static(plain)));
    assert_eq!(calls.get(), 0);
    let owned = Box::new(specialization);
    let actual = generic.apply_specialization(&db, |given| {
        calls.set(calls.get() + 1);
        assert_eq!(given, context);
        let specialization = *owned;
        drop(owned);
        specialization
    });
    assert_eq!(
        actual,
        ClassType::Generic(GenericAlias::new(&db, generic, specialization))
    );
    assert_eq!(calls.get(), 1);
    Ok(())
}
