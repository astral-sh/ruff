//! Member-presence preflight before protocol compatibility checks.

use std::convert::Infallible;

use super::outer_step::InterfaceMembers;
use super::{
    ProtocolInterface, ProtocolInterfaceView, ProtocolMember, non_object_protocol_member_count,
};
use crate::place::PlaceAndQualifiers;
use crate::types::relation::RelationFieldReads;
use crate::types::{MemberLookupPolicy, ProtocolInstanceType, Type};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ProtocolMembersDefinedWork {
    Entry,
    Complete,
}

pub(in crate::types) trait ProtocolMembersDefinedEffects<'db> {
    type Error;

    async fn checkpoint(&self, work: ProtocolMembersDefinedWork) -> Result<(), Self::Error>;
    async fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;
    async fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> Result<Option<ProtocolMember<'db, 'db>>, Self::Error>;
    async fn non_object_member_count(
        &self,
        interface: ProtocolInterface<'db>,
    ) -> Result<usize, Self::Error>;
    async fn includes_member_or_object_fallback(
        &self,
        source: ProtocolInterfaceView<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<bool, Self::Error>;
    async fn restricted_member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
}

pub(in crate::types) trait SyncProtocolMembersDefinedEffects<'db> {
    type Error;

    fn checkpoint(&self, work: ProtocolMembersDefinedWork) -> Result<(), Self::Error>;
    fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error>;
    fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> Result<Option<ProtocolMember<'db, 'db>>, Self::Error>;
    fn non_object_member_count(
        &self,
        interface: ProtocolInterface<'db>,
    ) -> Result<usize, Self::Error>;
    fn includes_member_or_object_fallback(
        &self,
        source: ProtocolInterfaceView<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<bool, Self::Error>;
    fn restricted_member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_protocol_members_defined]
pub(in crate::types) async fn protocol_members_defined_with<
    'db,
    E: ProtocolMembersDefinedEffects<'db>,
>(
    fields: RelationFieldReads<'db>,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects
        .checkpoint(ProtocolMembersDefinedWork::Entry)
        .await?;
    let target_interface = effects.protocol_interface(protocol).await?;
    let result = match ty {
        Type::ProtocolInstance(source_protocol) => {
            let source_interface = effects.protocol_interface(source_protocol).await?;
            if fields.protocol_interface_member_count(source_interface)
                >= fields.protocol_interface_member_count(target_interface)
                || fields.protocol_interface_member_count(source_interface)
                    >= effects
                        .non_object_member_count(target_interface.base())
                        .await?
            {
                let mut members = InterfaceMembers::with_fields(fields, target_interface);
                loop {
                    let Some(member) = effects.next_interface_member(&mut members).await? else {
                        break true;
                    };
                    if !effects
                        .includes_member_or_object_fallback(source_interface, env, member.name())
                        .await?
                    {
                        break false;
                    }
                }
            } else {
                false
            }
        }
        _ => {
            let mut members = InterfaceMembers::with_fields(fields, target_interface);
            loop {
                let Some(member) = effects.next_interface_member(&mut members).await? else {
                    break true;
                };
                if !effects
                    .restricted_member(ty, env, member.name())
                    .await?
                    .place
                    .is_definitely_bound()
                    && !effects
                        .member(ty, env, member.name())
                        .await?
                        .place
                        .is_definitely_bound()
                {
                    break false;
                }
            }
        }
    };
    effects
        .checkpoint(ProtocolMembersDefinedWork::Complete)
        .await?;
    Ok(result)
}

pub(super) struct InlineProtocolMembersDefinedEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineProtocolMembersDefinedEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SyncProtocolMembersDefinedEffects<'db> for InlineProtocolMembersDefinedEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: ProtocolMembersDefinedWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn protocol_interface(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Infallible> {
        Ok(protocol.interface(self.db))
    }

    fn next_interface_member(
        &self,
        members: &mut InterfaceMembers<'db>,
    ) -> Result<Option<ProtocolMember<'db, 'db>>, Infallible> {
        Ok(members.next())
    }

    fn non_object_member_count(
        &self,
        interface: ProtocolInterface<'db>,
    ) -> Result<usize, Infallible> {
        Ok(non_object_protocol_member_count(self.db, interface))
    }

    fn includes_member_or_object_fallback(
        &self,
        source: ProtocolInterfaceView<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<bool, Infallible> {
        Ok(source.includes_member_or_object_fallback(self.db, env, name))
    }

    fn restricted_member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.member_lookup_with_policy(
            self.db,
            env,
            name,
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        ))
    }

    fn member(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.member(self.db, env, name))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::convert::Infallible;

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;

    use super::{
        ProtocolMembersDefinedEffects, ProtocolMembersDefinedWork,
        SyncProtocolMembersDefinedEffects, protocol_members_defined_sync,
        protocol_members_defined_with,
    };
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::{Place, PlaceAndQualifiers, global_symbol};
    use crate::types::protocol_class::outer_step::InterfaceMembers;
    use crate::types::protocol_class::{ProtocolInterface, ProtocolInterfaceView, ProtocolMember};
    use crate::types::relation::RelationFieldReads;
    use crate::types::signatures::effects::legacy_inline;
    use crate::types::{ClassLiteral, ProtocolInstanceType, Type};
    use crate::{Db, ProgramEnvironment};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Operation<'db> {
        Work(ProtocolMembersDefinedWork),
        Interface(ProtocolInstanceType<'db>),
        Next,
        Count,
        Includes(&'db str),
        Restricted(&'db str),
        Member(&'db str),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Stopped;

    struct Observed<'db> {
        db: &'db TestDb,
        non_object_count: usize,
        included: &'static [&'static str],
        restricted: &'static [&'static str],
        fallback: &'static [&'static str],
        stop: Option<Operation<'db>>,
        operations: RefCell<Vec<Operation<'db>>>,
    }

    impl<'db> Observed<'db> {
        fn new(db: &'db TestDb) -> Self {
            Self {
                db,
                non_object_count: 0,
                included: &[],
                restricted: &[],
                fallback: &[],
                stop: None,
                operations: RefCell::default(),
            }
        }

        fn record(&self, operation: Operation<'db>) -> Result<(), Stopped> {
            self.operations.borrow_mut().push(operation);
            if self.stop == Some(operation) {
                Err(Stopped)
            } else {
                Ok(())
            }
        }

        fn assert_both(
            &self,
            ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            expected: Result<bool, Stopped>,
            operations: &[Operation<'db>],
        ) {
            let env = self.db.program_environment();
            for asynchronous in [false, true] {
                self.operations.borrow_mut().clear();
                let fields = RelationFieldReads::new(self.db);
                let actual = if asynchronous {
                    legacy_inline(async {
                        Ok::<_, Infallible>(
                            protocol_members_defined_with(fields, &env, ty, protocol, self).await,
                        )
                    })
                } else {
                    protocol_members_defined_sync(fields, &env, ty, protocol, self)
                };
                assert_eq!(actual, expected, "asynchronous={asynchronous}");
                assert_eq!(
                    &*self.operations.borrow(),
                    operations,
                    "asynchronous={asynchronous}"
                );
            }
        }
    }

    impl<'db> SyncProtocolMembersDefinedEffects<'db> for Observed<'db> {
        type Error = Stopped;

        fn checkpoint(&self, work: ProtocolMembersDefinedWork) -> Result<(), Stopped> {
            self.record(Operation::Work(work))
        }

        fn protocol_interface(
            &self,
            protocol: ProtocolInstanceType<'db>,
        ) -> Result<ProtocolInterfaceView<'db>, Stopped> {
            self.record(Operation::Interface(protocol))?;
            Ok(protocol.interface(self.db))
        }

        fn next_interface_member(
            &self,
            members: &mut InterfaceMembers<'db>,
        ) -> Result<Option<ProtocolMember<'db, 'db>>, Stopped> {
            self.record(Operation::Next)?;
            Ok(members.next())
        }

        fn non_object_member_count(
            &self,
            _interface: ProtocolInterface<'db>,
        ) -> Result<usize, Stopped> {
            self.record(Operation::Count)?;
            Ok(self.non_object_count)
        }

        fn includes_member_or_object_fallback(
            &self,
            _source: ProtocolInterfaceView<'db>,
            _env: &ProgramEnvironment<'db>,
            name: &'db str,
        ) -> Result<bool, Stopped> {
            self.record(Operation::Includes(name))?;
            Ok(self.included.contains(&name))
        }

        fn restricted_member(
            &self,
            _ty: Type<'db>,
            _env: &ProgramEnvironment<'db>,
            name: &'db str,
        ) -> Result<PlaceAndQualifiers<'db>, Stopped> {
            self.record(Operation::Restricted(name))?;
            Ok(if self.restricted.contains(&name) {
                Place::bound(Type::object())
            } else {
                Place::Undefined
            }
            .into())
        }

        fn member(
            &self,
            _ty: Type<'db>,
            _env: &ProgramEnvironment<'db>,
            name: &'db str,
        ) -> Result<PlaceAndQualifiers<'db>, Stopped> {
            self.record(Operation::Member(name))?;
            Ok(if self.fallback.contains(&name) {
                Place::bound(Type::object())
            } else {
                Place::Undefined
            }
            .into())
        }
    }

    impl<'db> ProtocolMembersDefinedEffects<'db> for Observed<'db> {
        type Error = Stopped;

        async fn checkpoint(&self, work: ProtocolMembersDefinedWork) -> Result<(), Stopped> {
            SyncProtocolMembersDefinedEffects::checkpoint(self, work)
        }
        async fn protocol_interface(
            &self,
            protocol: ProtocolInstanceType<'db>,
        ) -> Result<ProtocolInterfaceView<'db>, Stopped> {
            SyncProtocolMembersDefinedEffects::protocol_interface(self, protocol)
        }
        async fn next_interface_member(
            &self,
            members: &mut InterfaceMembers<'db>,
        ) -> Result<Option<ProtocolMember<'db, 'db>>, Stopped> {
            SyncProtocolMembersDefinedEffects::next_interface_member(self, members)
        }
        async fn non_object_member_count(
            &self,
            interface: ProtocolInterface<'db>,
        ) -> Result<usize, Stopped> {
            SyncProtocolMembersDefinedEffects::non_object_member_count(self, interface)
        }
        async fn includes_member_or_object_fallback(
            &self,
            source: ProtocolInterfaceView<'db>,
            env: &ProgramEnvironment<'db>,
            name: &'db str,
        ) -> Result<bool, Stopped> {
            SyncProtocolMembersDefinedEffects::includes_member_or_object_fallback(
                self, source, env, name,
            )
        }
        async fn restricted_member(
            &self,
            ty: Type<'db>,
            env: &ProgramEnvironment<'db>,
            name: &'db str,
        ) -> Result<PlaceAndQualifiers<'db>, Stopped> {
            SyncProtocolMembersDefinedEffects::restricted_member(self, ty, env, name)
        }
        async fn member(
            &self,
            ty: Type<'db>,
            env: &ProgramEnvironment<'db>,
            name: &'db str,
        ) -> Result<PlaceAndQualifiers<'db>, Stopped> {
            SyncProtocolMembersDefinedEffects::member(self, ty, env, name)
        }
    }

    fn fixture() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file("/src/preflight.py", "from typing import Protocol\nclass Empty(Protocol): ...\nclass One(Protocol):\n    z: object\nclass Two(Protocol):\n    z: object\n    a: object\n")
            .build()
    }

    fn protocol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ProtocolInstanceType<'db>> {
        let file = db.program_file(system_path_to_file(db, "/src/preflight.py")?);
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
    fn protocol_count_request_is_lazy_and_precedes_member_iteration() -> anyhow::Result<()> {
        let db = fixture()?;
        let source = protocol(&db, "One")?;
        let target = protocol(&db, "Two")?;
        let mut effects = Observed::new(&db);
        effects.included = &["a", "z"];
        effects.stop = Some(Operation::Count);
        effects.assert_both(
            Type::ProtocolInstance(target),
            target,
            Ok(true),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Interface(target),
                Operation::Next,
                Operation::Includes("a"),
                Operation::Next,
                Operation::Includes("z"),
                Operation::Next,
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        effects.stop = None;
        effects.non_object_count = 1;
        effects.assert_both(
            Type::ProtocolInstance(source),
            target,
            Ok(true),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Interface(source),
                Operation::Count,
                Operation::Next,
                Operation::Includes("a"),
                Operation::Next,
                Operation::Includes("z"),
                Operation::Next,
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        effects.non_object_count = 2;
        effects.assert_both(
            Type::ProtocolInstance(source),
            target,
            Ok(false),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Interface(source),
                Operation::Count,
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        Ok(())
    }

    #[test]
    fn protocol_missing_member_stops_the_shared_iteration() -> anyhow::Result<()> {
        let db = fixture()?;
        let target = protocol(&db, "Two")?;
        let effects = Observed::new(&db);
        effects.assert_both(
            Type::ProtocolInstance(target),
            target,
            Ok(false),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Interface(target),
                Operation::Next,
                Operation::Includes("a"),
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        Ok(())
    }

    #[test]
    fn fallback_lookup_is_lazy_and_first_missing_member_stops_iteration() -> anyhow::Result<()> {
        let db = fixture()?;
        let target = protocol(&db, "Two")?;
        let mut effects = Observed::new(&db);
        effects.restricted = &["a"];
        effects.fallback = &["z"];
        effects.assert_both(
            Type::object(),
            target,
            Ok(true),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Next,
                Operation::Restricted("a"),
                Operation::Next,
                Operation::Restricted("z"),
                Operation::Member("z"),
                Operation::Next,
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        effects.restricted = &[];
        effects.fallback = &[];
        effects.assert_both(
            Type::object(),
            target,
            Ok(false),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Next,
                Operation::Restricted("a"),
                Operation::Member("a"),
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        let empty = protocol(&db, "Empty")?;
        effects.assert_both(
            Type::object(),
            empty,
            Ok(true),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(empty),
                Operation::Next,
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        Ok(())
    }

    #[test]
    fn refused_dependency_or_completion_does_not_return_a_boolean() -> anyhow::Result<()> {
        let db = fixture()?;
        let target = protocol(&db, "Two")?;
        let mut effects = Observed::new(&db);
        effects.stop = Some(Operation::Restricted("a"));
        effects.assert_both(
            Type::object(),
            target,
            Err(Stopped),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(target),
                Operation::Next,
                Operation::Restricted("a"),
            ],
        );
        let empty = protocol(&db, "Empty")?;
        effects.stop = Some(Operation::Work(ProtocolMembersDefinedWork::Complete));
        effects.assert_both(
            Type::object(),
            empty,
            Err(Stopped),
            &[
                Operation::Work(ProtocolMembersDefinedWork::Entry),
                Operation::Interface(empty),
                Operation::Next,
                Operation::Work(ProtocolMembersDefinedWork::Complete),
            ],
        );
        Ok(())
    }
}
