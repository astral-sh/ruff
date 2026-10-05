use std::cell::Cell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ty_python_core::semantic_index;

use super::*;
use crate::db::tests::TestDbBuilder;
use crate::types::infer::InferenceRegion;
use crate::types::signatures::effects::try_poll_immediate;
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefuseAt {
    Base,
    ExtraItems,
}

struct RefusingEffects<'db> {
    definition: Definition<'db>,
    deferred: bool,
    expressions: Cell<usize>,
    refuse_at: RefuseAt,
}

impl<'db> DeferredEffects<'db> for RefusingEffects<'db> {
    type Error = RefuseAt;

    async fn checkpoint(&self) -> Result<(), RefuseAt> {
        Ok(())
    }

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, RefuseAt> {
        Ok(definition.kind(db))
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, RefuseAt> {
        Ok(context.in_stub())
    }

    async fn class_checkpoint(&self, _work: DeferredClassWork) -> Result<(), RefuseAt> {
        Ok(())
    }

    async fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        _expression: &ast::Expr,
    ) -> Result<Type<'db>, RefuseAt> {
        assert_eq!(builder.typevar_binding_context, Some(self.definition));
        assert_eq!(builder.is_deferred(), self.deferred);
        self.expressions.set(self.expressions.get() + 1);
        if self.refuse_at == RefuseAt::Base {
            Err(RefuseAt::Base)
        } else {
            Ok(Type::unknown())
        }
    }

    async fn is_typed_dict(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _definition: Definition<'db>,
    ) -> Result<bool, RefuseAt> {
        assert_eq!(self.expressions.get(), 2);
        Err(RefuseAt::ExtraItems)
    }

    async fn extra_items(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _expression: &ast::Expr,
    ) -> Result<(), RefuseAt> {
        Err(RefuseAt::ExtraItems)
    }

    async fn function_annotations<'ast>(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _definition: Definition<'db>,
        _function: &ast::StmtFunctionDef,
    ) -> Result<(), RefuseAt> {
        Err(self.refuse_at)
    }

    async fn type_parameter<'ast>(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _node: TypeParameterDefinitionNode<'_>,
    ) -> Result<(), RefuseAt> {
        Err(self.refuse_at)
    }

    async fn assignment<'ast>(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _value: &'ast ast::Expr,
    ) -> Result<(), RefuseAt> {
        Err(self.refuse_at)
    }
}

#[test]
fn class_refusal_restores_lookup_and_typevar_context() -> anyhow::Result<()> {
    for path in ["/src/header.py", "/src/header.pyi"] {
        let db = TestDbBuilder::new()
            .with_file(path, "class C(Base, Other, extra_items=value): ...\n")
            .build()?;
        let file = db.program_file(system_path_to_file(&db, path)?);
        let module = parsed_module(&db, file.python_file(&db)).load(&db);
        let [ast::Stmt::ClassDef(class)] = module.suite().as_slice() else {
            anyhow::bail!("expected a class definition");
        };
        let index = semantic_index(&db, file);
        let definition = index.expect_single_definition(class);
        let env = ProgramEnvironment::from_file(file);
        for refuse_at in [RefuseAt::Base, RefuseAt::ExtraItems] {
            let mut builder = TypeInferenceBuilder::new(
                &db,
                &env,
                InferenceRegion::Deferred(definition),
                file.file(&db),
                file,
                index,
                &module,
            );
            builder.context.defuse();
            let effects = RefusingEffects {
                definition,
                deferred: path.ends_with(".pyi"),
                expressions: Cell::new(0),
                refuse_at,
            };
            assert_eq!(
                try_poll_immediate(builder.infer_region_deferred_with(&effects, definition)),
                Poll::Ready(Err(refuse_at)),
            );
            assert!(builder.typevar_binding_context.is_none());
            assert!(!builder.is_deferred());
            assert_eq!(
                effects.expressions.get(),
                if refuse_at == RefuseAt::Base { 1 } else { 2 },
            );
        }
    }
    Ok(())
}
