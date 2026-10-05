use std::convert::Infallible;

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::Definition;
use ty_python_core::semantic_index;

use crate::Db;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::{StaticClassLiteral, Type, definition_expression_type};

pub(in crate::types) struct DecoratorFacts;

pub(in crate::types) struct DecoratorExpressionCursor<'source> {
    elements: std::slice::Iter<'source, ast::Decorator>,
}

impl<'source> DecoratorExpressionCursor<'source> {
    pub(in crate::types) fn new(elements: &'source [ast::Decorator]) -> Self {
        Self {
            elements: elements.iter(),
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<&'source ast::Expr> {
        self.elements.next().map(|decorator| &decorator.expression)
    }
}

pub(in crate::types) struct DecoratorTypeCursor<'db> {
    elements: std::slice::Iter<'db, Type<'db>>,
}

impl<'db> DecoratorTypeCursor<'db> {
    pub(in crate::types) fn new(elements: &'db [Type<'db>]) -> Self {
        Self {
            elements: elements.iter(),
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<Type<'db>> {
        self.elements.next().copied()
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassDecoratorEffects)]
    pub(in crate::types) trait ClassDecoratorEffects<'db> {
        type Error;
        type Source;
        type Buffer;

        #[operation(source)]
        async fn source(&self, class: StaticClassLiteral<'db>) -> Result<Self::Source, Self::Error>;
        #[operation(local)]
        async fn len(&self, source: &Self::Source) -> Result<usize, Self::Error>;
        #[operation(local)]
        async fn empty(&self) -> Result<Box<[Type<'db>]>, Self::Error>;
        #[operation(source)]
        async fn definition(&self, class: StaticClassLiteral<'db>, source: &Self::Source) -> Result<Definition<'db>, Self::Error>;
        #[operation(local)]
        async fn buffer(&self, capacity: usize) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_expression<'source>(&self, cursor: &mut DecoratorExpressionCursor<'source>) -> Result<Option<&'source ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn expression_cursor<'source>(&self, source: &'source Self::Source) -> Result<DecoratorExpressionCursor<'source>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn append(&self, class: StaticClassLiteral<'db>, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, buffer: Self::Buffer) -> Result<Box<[Type<'db>]>, Self::Error>;
    }

    #[synchronous(SynchronousKnownClassDecoratorEffects)]
    pub(in crate::types) trait KnownClassDecoratorEffects<'db> {
        type Error;

        #[operation(source)]
        async fn has_decorators(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn decorators(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn cursor(&self, elements: &'db [Type<'db>]) -> Result<DecoratorTypeCursor<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, cursor: &mut DecoratorTypeCursor<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn known_function(&self, function: FunctionType<'db>) -> Result<Option<KnownFunction>, Self::Error>;
    }

    #[finite_capability]
    impl DecoratorFacts {
        fn is_empty(&self, len: usize) -> bool { len == 0 }
        fn function<'db>(&self, ty: Type<'db>) -> Option<FunctionType<'db>> { ty.as_function_literal() }
        fn matches(&self, actual: Option<KnownFunction>, expected: KnownFunction) -> bool { actual == Some(expected) }
    }

    #[synchronous(class_decorators_sync)]
    #[capabilities(effects = ClassDecoratorEffects, facts = DecoratorFacts)]
    #[passive_values()]
    pub(in crate::types) async fn class_decorators_with<'db, E: ClassDecoratorEffects<'db>>(
        class: StaticClassLiteral<'db>, facts: DecoratorFacts, effects: &E,
    ) -> Result<Box<[Type<'db>]>, E::Error> {
        let source = effects.source(class).await?;
        let len = effects.len(&source).await?;
        if facts.is_empty(len) { return effects.empty().await; }
        let definition = effects.definition(class, &source).await?;
        let mut cursor = effects.expression_cursor(&source).await?;
        let mut buffer = effects.buffer(len).await?;
        #[cursor_loop]
        while let Some(expression) = effects.next_expression(&mut cursor).await? {
            let ty = effects.expression_type(definition, expression).await?;
            effects.append(class, &mut buffer, ty).await?;
        }
        effects.finish(buffer).await
    }

    #[synchronous(has_known_class_decorator_sync)]
    #[capabilities(effects = KnownClassDecoratorEffects, facts = DecoratorFacts)]
    #[passive_values()]
    pub(in crate::types) async fn has_known_class_decorator_with<'db, E: KnownClassDecoratorEffects<'db>>(
        class: StaticClassLiteral<'db>, expected: KnownFunction, facts: DecoratorFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        if !effects.has_decorators(class).await? { return Ok(false); }
        let elements = effects.decorators(class).await?;
        let mut cursor = effects.cursor(elements).await?;
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut cursor).await? {
            if let Some(function) = facts.function(ty)
                && facts.matches(effects.known_function(function).await?, expected)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub(super) struct InlineClassDecoratorEffects<'db>(pub &'db dyn Db);

pub(super) struct InlineDecoratorSource<'db> {
    module: ParsedModuleRef,
    node: &'db AstNodeRef<ast::StmtClassDef>,
}

impl<'db> SynchronousClassDecoratorEffects<'db> for InlineClassDecoratorEffects<'db> {
    type Error = Infallible;
    type Source = InlineDecoratorSource<'db>;
    type Buffer = Vec<Type<'db>>;

    fn source(&self, class: StaticClassLiteral<'db>) -> Result<Self::Source, Self::Error> {
        let program_file = class.program_file(self.0);
        let python_file = program_file.python_file(self.0);
        let module = parsed_module(self.0, python_file).load(self.0);
        let node = class.body_scope(self.0).node(self.0).expect_class();
        Ok(InlineDecoratorSource { module, node })
    }
    fn len(&self, source: &Self::Source) -> Result<usize, Self::Error> {
        Ok(source.node.node(&source.module).decorator_list.len())
    }
    fn empty(&self) -> Result<Box<[Type<'db>]>, Self::Error> {
        Ok(Box::new([]))
    }
    fn definition(
        &self,
        class: StaticClassLiteral<'db>,
        source: &Self::Source,
    ) -> Result<Definition<'db>, Self::Error> {
        Ok(semantic_index(self.0, class.program_file(self.0))
            .expect_single_definition(source.node.node(&source.module)))
    }
    fn buffer(&self, capacity: usize) -> Result<Self::Buffer, Self::Error> {
        Ok(Vec::with_capacity(capacity))
    }
    fn expression_cursor<'source>(
        &self,
        source: &'source Self::Source,
    ) -> Result<DecoratorExpressionCursor<'source>, Self::Error> {
        Ok(DecoratorExpressionCursor::new(
            &source.node.node(&source.module).decorator_list,
        ))
    }
    fn next_expression<'source>(
        &self,
        cursor: &mut DecoratorExpressionCursor<'source>,
    ) -> Result<Option<&'source ast::Expr>, Self::Error> {
        Ok(cursor.next())
    }
    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(definition_expression_type(self.0, definition, expression))
    }
    fn append(
        &self,
        _class: StaticClassLiteral<'db>,
        buffer: &mut Self::Buffer,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        buffer.push(ty);
        Ok(())
    }
    fn finish(&self, buffer: Self::Buffer) -> Result<Box<[Type<'db>]>, Self::Error> {
        Ok(buffer.into_boxed_slice())
    }
}

impl<'db> SynchronousKnownClassDecoratorEffects<'db> for InlineClassDecoratorEffects<'db> {
    type Error = Infallible;
    fn has_decorators(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_decorators(self.0))
    }
    fn decorators(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(class.decorators_inner(self.0))
    }
    fn cursor(&self, elements: &'db [Type<'db>]) -> Result<DecoratorTypeCursor<'db>, Self::Error> {
        Ok(DecoratorTypeCursor::new(elements))
    }
    fn next_type(
        &self,
        cursor: &mut DecoratorTypeCursor<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(cursor.next())
    }
    fn known_function(
        &self,
        function: FunctionType<'db>,
    ) -> Result<Option<KnownFunction>, Self::Error> {
        Ok(function.known(self.0))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::ClassLiteral;
    use crate::types::signatures::effects::try_poll_immediate;

    fn database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new().with_file("/src/decorators.pyi", "from typing import final, type_check_only\nclass Plain: ...\n@final\n@type_check_only\nclass Decorated: ...\n").build()
    }

    fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
        let file = db.program_file(system_path_to_file(db, "/src/decorators.pyi")?);
        global_symbol(db, file, name)
            .place
            .ignore_possibly_undefined()
            .and_then(Type::as_class_literal)
            .and_then(ClassLiteral::as_static)
            .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
    }

    fn infallible<T>(result: Result<T, Infallible>) -> T {
        match result {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    #[derive(Default)]
    struct Journal {
        events: RefCell<Vec<&'static str>>,
        sources: Cell<usize>,
        retired: RefCell<Vec<(usize, usize)>>,
        refuse_at: Option<usize>,
    }

    impl Journal {
        fn record(&self, event: &'static str) -> Result<(), &'static str> {
            let index = self.events.borrow().len();
            self.events.borrow_mut().push(event);
            if self.refuse_at == Some(index) {
                Err(event)
            } else {
                Ok(())
            }
        }
    }

    struct ObservedSource<'db> {
        source: InlineDecoratorSource<'db>,
        journal: Rc<Journal>,
    }
    impl Drop for ObservedSource<'_> {
        fn drop(&mut self) {
            self.journal.sources.set(self.journal.sources.get() - 1);
        }
    }
    struct ObservedBuffer<'db> {
        values: Vec<Type<'db>>,
        journal: Rc<Journal>,
    }
    impl Drop for ObservedBuffer<'_> {
        fn drop(&mut self) {
            self.journal
                .retired
                .borrow_mut()
                .push((self.values.len(), self.journal.sources.get()));
        }
    }
    struct Observed<'db> {
        inline: InlineClassDecoratorEffects<'db>,
        journal: Rc<Journal>,
    }

    impl<'db> ClassDecoratorEffects<'db> for Observed<'db> {
        type Error = &'static str;
        type Source = ObservedSource<'db>;
        type Buffer = ObservedBuffer<'db>;
        async fn source(
            &self,
            class: StaticClassLiteral<'db>,
        ) -> Result<Self::Source, Self::Error> {
            self.journal.record("source")?;
            let source = infallible(self.inline.source(class));
            self.journal.sources.set(self.journal.sources.get() + 1);
            Ok(ObservedSource {
                source,
                journal: self.journal.clone(),
            })
        }
        async fn len(&self, source: &Self::Source) -> Result<usize, Self::Error> {
            self.journal.record("length")?;
            Ok(infallible(self.inline.len(&source.source)))
        }
        async fn empty(&self) -> Result<Box<[Type<'db>]>, Self::Error> {
            self.journal.record("empty")?;
            Ok(Box::new([]))
        }
        async fn definition(
            &self,
            class: StaticClassLiteral<'db>,
            source: &Self::Source,
        ) -> Result<Definition<'db>, Self::Error> {
            self.journal.record("definition")?;
            Ok(infallible(self.inline.definition(class, &source.source)))
        }
        async fn expression_cursor<'source>(
            &self,
            source: &'source Self::Source,
        ) -> Result<DecoratorExpressionCursor<'source>, Self::Error> {
            self.journal.record("expressions")?;
            Ok(infallible(self.inline.expression_cursor(&source.source)))
        }
        async fn buffer(&self, capacity: usize) -> Result<Self::Buffer, Self::Error> {
            self.journal.record("buffer")?;
            Ok(ObservedBuffer {
                values: Vec::with_capacity(capacity),
                journal: self.journal.clone(),
            })
        }
        async fn next_expression<'source>(
            &self,
            cursor: &mut DecoratorExpressionCursor<'source>,
        ) -> Result<Option<&'source ast::Expr>, Self::Error> {
            self.journal.record("advance")?;
            Ok(cursor.next())
        }
        async fn expression_type(
            &self,
            definition: Definition<'db>,
            expression: &ast::Expr,
        ) -> Result<Type<'db>, Self::Error> {
            self.journal.record("infer")?;
            Ok(infallible(
                self.inline.expression_type(definition, expression),
            ))
        }
        async fn append(
            &self,
            _class: StaticClassLiteral<'db>,
            buffer: &mut Self::Buffer,
            ty: Type<'db>,
        ) -> Result<(), Self::Error> {
            self.journal.record("append")?;
            buffer.values.push(ty);
            Ok(())
        }
        async fn finish(&self, mut buffer: Self::Buffer) -> Result<Box<[Type<'db>]>, Self::Error> {
            self.journal.record("finish")?;
            Ok(std::mem::take(&mut buffer.values).into_boxed_slice())
        }
    }

    #[test]
    fn decorator_production_preserves_empty_guard_order_and_refusal_cleanup() -> anyhow::Result<()>
    {
        let db = database()?;
        let plain = class(&db, "Plain")?;
        let journal = Rc::new(Journal::default());
        let effects = Observed {
            inline: InlineClassDecoratorEffects(&db),
            journal: journal.clone(),
        };
        assert!(
            matches!(try_poll_immediate(class_decorators_with(plain, DecoratorFacts, &effects)), Poll::Ready(Ok(values)) if values.is_empty())
        );
        assert_eq!(*journal.events.borrow(), ["source", "length", "empty"]);
        assert_eq!(journal.sources.get(), 0);
        assert!(journal.retired.borrow().is_empty());

        let decorated = class(&db, "Decorated")?;
        let sequence = [
            "source",
            "length",
            "definition",
            "expressions",
            "buffer",
            "advance",
            "infer",
            "append",
            "advance",
            "infer",
            "append",
            "advance",
            "finish",
        ];
        for refuse_at in std::iter::once(None).chain((0..sequence.len()).map(Some)) {
            let journal = Rc::new(Journal {
                refuse_at,
                ..Journal::default()
            });
            let effects = Observed {
                inline: InlineClassDecoratorEffects(&db),
                journal: journal.clone(),
            };
            let result =
                try_poll_immediate(class_decorators_with(decorated, DecoratorFacts, &effects));
            if let Some(index) = refuse_at {
                assert_eq!(result, Poll::Ready(Err(sequence[index])));
                assert_eq!(*journal.events.borrow(), sequence[..=index]);
                let prefix = if index <= 7 {
                    0
                } else if index <= 10 {
                    1
                } else {
                    2
                };
                let expected = if index <= 4 {
                    vec![]
                } else {
                    vec![(prefix, 1)]
                };
                assert_eq!(*journal.retired.borrow(), expected);
            } else {
                let Poll::Ready(Ok(values)) = result else {
                    anyhow::bail!("decorator production did not complete");
                };
                assert_eq!(
                    values
                        .iter()
                        .map(|ty| ty
                            .as_function_literal()
                            .and_then(|function| function.known(&db)))
                        .collect::<Vec<_>>(),
                    [
                        Some(KnownFunction::Final),
                        Some(KnownFunction::TypeCheckOnly)
                    ]
                );
                assert_eq!(*journal.events.borrow(), sequence);
                assert_eq!(*journal.retired.borrow(), [(0, 1)]);
            }
            assert_eq!(journal.sources.get(), 0);
        }
        Ok(())
    }

    struct Membership<'db> {
        elements: &'db [Type<'db>],
        has_decorators: bool,
        journal: Journal,
        db: &'db dyn Db,
    }
    impl<'db> KnownClassDecoratorEffects<'db> for Membership<'db> {
        type Error = &'static str;
        async fn has_decorators(
            &self,
            _class: StaticClassLiteral<'db>,
        ) -> Result<bool, Self::Error> {
            self.journal.record("has decorators")?;
            Ok(self.has_decorators)
        }
        async fn decorators(
            &self,
            _class: StaticClassLiteral<'db>,
        ) -> Result<&'db [Type<'db>], Self::Error> {
            self.journal.record("canonical decorators")?;
            Ok(self.elements)
        }
        async fn cursor(
            &self,
            elements: &'db [Type<'db>],
        ) -> Result<DecoratorTypeCursor<'db>, Self::Error> {
            self.journal.record("cursor")?;
            Ok(DecoratorTypeCursor::new(elements))
        }
        async fn next_type(
            &self,
            cursor: &mut DecoratorTypeCursor<'db>,
        ) -> Result<Option<Type<'db>>, Self::Error> {
            self.journal.record("advance")?;
            Ok(cursor.next())
        }
        async fn known_function(
            &self,
            function: FunctionType<'db>,
        ) -> Result<Option<KnownFunction>, Self::Error> {
            self.journal.record("known function")?;
            Ok(function.known(self.db))
        }
    }

    #[test]
    fn decorator_membership_skips_nonfunctions_and_stops_at_first_match() -> anyhow::Result<()> {
        let db = database()?;
        let decorated = class(&db, "Decorated")?;
        let decorators = infallible(class_decorators_sync(
            decorated,
            DecoratorFacts,
            &InlineClassDecoratorEffects(&db),
        ));
        let elements = [Type::Never, decorators[0], decorators[1]];
        let sequence = [
            "has decorators",
            "canonical decorators",
            "cursor",
            "advance",
            "advance",
            "known function",
        ];
        for refuse_at in std::iter::once(None).chain((0..sequence.len()).map(Some)) {
            let effects = Membership {
                elements: &elements,
                has_decorators: true,
                journal: Journal {
                    refuse_at,
                    ..Journal::default()
                },
                db: &db,
            };
            let result = try_poll_immediate(has_known_class_decorator_with(
                decorated,
                KnownFunction::Final,
                DecoratorFacts,
                &effects,
            ));
            if let Some(index) = refuse_at {
                assert_eq!(result, Poll::Ready(Err(sequence[index])));
                assert_eq!(*effects.journal.events.borrow(), sequence[..=index]);
            } else {
                assert_eq!(result, Poll::Ready(Ok(true)));
                assert_eq!(*effects.journal.events.borrow(), sequence);
            }
        }
        let effects = Membership {
            elements: &elements,
            has_decorators: false,
            journal: Journal::default(),
            db: &db,
        };
        assert_eq!(
            try_poll_immediate(has_known_class_decorator_with(
                decorated,
                KnownFunction::Final,
                DecoratorFacts,
                &effects
            )),
            Poll::Ready(Ok(false))
        );
        assert_eq!(*effects.journal.events.borrow(), ["has decorators"]);
        Ok(())
    }
}
