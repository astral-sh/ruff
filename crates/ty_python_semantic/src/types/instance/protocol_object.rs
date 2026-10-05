//! Shared entry and exclusions for protocol equivalence to object.

use std::convert::Infallible;

use super::ProtocolInstanceType;
use super::protocol_relation::{InlineProtocolRelationEffects, ProtocolRelationEffects};
use crate::types::Type;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::protocol_class::ProtocolInterfaceView;
use crate::types::relation::dependencies::OrdinaryDependencies;
use crate::types::relation::{RelationFieldReads, RelationOwners, TypeRelationChecker};
use crate::types::typevar::TypeVarSet;
use crate::{Db, Program, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ProtocolObjectWork {
    Entry,
    MemberName { bytes: usize },
    Complete,
}

pub(in crate::types) trait ProtocolObjectEffects<'db> {
    type Error;

    async fn checkpoint(&self, work: ProtocolObjectWork) -> Result<(), Self::Error>;
    async fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;
    async fn compare_object(
        &self,
        program: Program<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error>;
}

pub(in crate::types) trait SyncProtocolObjectEffects<'db> {
    type Error;

    fn checkpoint(&self, work: ProtocolObjectWork) -> Result<(), Self::Error>;
    fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;
    fn compare_object(
        &self,
        program: Program<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error>;
}

#[ty_mapping_probe_macros::dual_protocol_object]
pub(in crate::types) async fn protocol_object_equivalence_with<
    'db,
    E: ProtocolObjectEffects<'db>,
>(
    fields: RelationFieldReads<'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint(ProtocolObjectWork::Entry).await?;
    let interface = effects.protocol_interface(protocol).await?;

    // Neither hashability nor instance-dictionary storage is guaranteed for every object.
    // Subclasses can replace `object.__hash__` with `None`, while slotted instances can
    // omit the `__dict__` that typeshed broadly declares on `object`.
    effects
        .checkpoint(ProtocolObjectWork::MemberName {
            bytes: "__hash__".len(),
        })
        .await?;
    if fields.protocol_interface_includes_member(interface, "__hash__") {
        effects.checkpoint(ProtocolObjectWork::Complete).await?;
        return Ok(false);
    }
    effects
        .checkpoint(ProtocolObjectWork::MemberName {
            bytes: "__dict__".len(),
        })
        .await?;
    if fields.protocol_interface_includes_member(interface, "__dict__") {
        effects.checkpoint(ProtocolObjectWork::Complete).await?;
        return Ok(false);
    }

    let program = fields.protocol_interface_program(interface);
    let result = effects.compare_object(program, protocol).await?;
    effects.checkpoint(ProtocolObjectWork::Complete).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_protocol_relation]
pub(in crate::types) async fn protocol_object_compare_with<
    'checker,
    'a,
    'c,
    'db,
    E: ProtocolRelationEffects<'a, 'c, 'db>,
>(
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    let constraints = effects
        .check_type_satisfies_protocol(checker, Type::object(), protocol)
        .await?;
    effects.is_always_satisfied(checker, constraints).await
}

pub(super) struct InlineProtocolObjectEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineProtocolObjectEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SyncProtocolObjectEffects<'db> for InlineProtocolObjectEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: ProtocolObjectWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Infallible> {
        Ok(protocol.interface(self.db))
    }

    fn compare_object(
        &self,
        program: Program<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Infallible> {
        let env = ProgramEnvironment::from_program(program);
        let constraints = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &constraints);
        let checker = owners.subtyping(TypeVarSet::None);
        protocol_object_compare_sync(
            &checker,
            protocol,
            &InlineProtocolRelationEffects::new(self.db, &OrdinaryDependencies),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::convert::Infallible;

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;

    use super::{
        InlineProtocolObjectEffects, ProtocolObjectEffects, ProtocolObjectWork,
        SyncProtocolObjectEffects, protocol_object_equivalence_sync,
        protocol_object_equivalence_with,
    };
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::instance::protocol_object_equivalence_ingredient;
    use crate::types::protocol_class::ProtocolInterfaceView;
    use crate::types::relation::RelationFieldReads;
    use crate::types::signatures::effects::legacy_inline;
    use crate::types::{ClassLiteral, ProtocolInstanceType, Type};
    use crate::{Db, Program};

    #[derive(Debug, Eq, PartialEq)]
    enum Operation {
        Work(ProtocolObjectWork),
        Interface,
        Compare,
    }

    struct Observed<'db> {
        inline: InlineProtocolObjectEffects<'db>,
        operations: RefCell<Vec<Operation>>,
    }

    impl<'db> Observed<'db> {
        fn new(db: &'db dyn Db) -> Self {
            Self {
                inline: InlineProtocolObjectEffects::new(db),
                operations: RefCell::default(),
            }
        }
    }

    impl<'db> SyncProtocolObjectEffects<'db> for Observed<'db> {
        type Error = Infallible;

        fn checkpoint(&self, work: ProtocolObjectWork) -> Result<(), Infallible> {
            self.operations.borrow_mut().push(Operation::Work(work));
            self.inline.checkpoint(work)
        }

        fn protocol_interface(
            &self,
            protocol: ProtocolInstanceType<'db>,
        ) -> Result<ProtocolInterfaceView<'db>, Infallible> {
            self.operations.borrow_mut().push(Operation::Interface);
            self.inline.protocol_interface(protocol)
        }

        fn compare_object(
            &self,
            program: Program<'db>,
            protocol: ProtocolInstanceType<'db>,
        ) -> Result<bool, Infallible> {
            self.operations.borrow_mut().push(Operation::Compare);
            self.inline.compare_object(program, protocol)
        }
    }

    impl<'db> ProtocolObjectEffects<'db> for Observed<'db> {
        type Error = Infallible;

        async fn checkpoint(&self, work: ProtocolObjectWork) -> Result<(), Infallible> {
            SyncProtocolObjectEffects::checkpoint(self, work)
        }

        async fn protocol_interface(
            &self,
            protocol: ProtocolInstanceType<'db>,
        ) -> Result<ProtocolInterfaceView<'db>, Infallible> {
            SyncProtocolObjectEffects::protocol_interface(self, protocol)
        }

        async fn compare_object(
            &self,
            program: Program<'db>,
            protocol: ProtocolInstanceType<'db>,
        ) -> Result<bool, Infallible> {
            SyncProtocolObjectEffects::compare_object(self, program, protocol)
        }
    }

    fn protocol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ProtocolInstanceType<'db>> {
        let file = db.program_file(system_path_to_file(db, "/src/object_protocol.py")?);
        let class = global_symbol(db, file, name)
            .place
            .ignore_possibly_undefined()
            .and_then(Type::as_class_literal)
            .and_then(ClassLiteral::as_static)
            .ok_or_else(|| anyhow::anyhow!("missing fixture class {name}"))?;
        Type::instance(
            db,
            &db.program_environment(),
            class.identity_specialization(db),
        )
        .as_protocol_instance()
        .ok_or_else(|| anyhow::anyhow!("fixture {name} is not a protocol"))
    }

    #[test]
    fn shared_object_entry_preserves_class_member_filtering_and_hash_exclusion()
    -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/object_protocol.py",
                "from typing import Protocol\n\nclass Empty(Protocol): ...\n\nclass Hash(Protocol):\n    __hash__: object\n\nclass Dictionary(Protocol):\n    __dict__: object\n\nclass Both(Protocol):\n    __hash__: object\n    __dict__: object\n",
            )
            .build()?;
        let _ingredient = protocol_object_equivalence_ingredient(&db);

        // Protocol interfaces omit `__dict__` declarations, so Dictionary is empty and Both
        // only requires `__hash__`.
        for (name, expected_members, expected_result, names, compare) in [
            ("Empty", [].as_slice(), true, 2, true),
            ("Hash", ["__hash__"].as_slice(), false, 1, false),
            ("Dictionary", [].as_slice(), true, 2, true),
            ("Both", ["__hash__"].as_slice(), false, 1, false),
        ] {
            let protocol = protocol(&db, name)?;
            let members = protocol
                .interface(&db)
                .members(&db)
                .map(|member| member.name())
                .collect::<Vec<_>>();
            assert_eq!(members, expected_members, "{name}");
            let mut expected = vec![
                Operation::Work(ProtocolObjectWork::Entry),
                Operation::Interface,
            ];
            expected.extend(
                (0..names).map(|_| Operation::Work(ProtocolObjectWork::MemberName { bytes: 8 })),
            );
            if compare {
                expected.push(Operation::Compare);
            }
            expected.push(Operation::Work(ProtocolObjectWork::Complete));

            let synchronous = Observed::new(&db);
            let result = protocol_object_equivalence_sync(
                RelationFieldReads::new(&db),
                protocol,
                &synchronous,
            )?;
            assert_eq!(result, expected_result, "{name}");
            assert_eq!(*synchronous.operations.borrow(), expected, "{name}");

            let asynchronous = Observed::new(&db);
            let result = legacy_inline(protocol_object_equivalence_with(
                RelationFieldReads::new(&db),
                protocol,
                &asynchronous,
            ));
            assert_eq!(result, expected_result, "{name}");
            assert_eq!(*asynchronous.operations.borrow(), expected, "{name}");
            assert_eq!(
                protocol.is_equivalent_to_object(&db),
                expected_result,
                "{name}"
            );
        }
        Ok(())
    }
}
