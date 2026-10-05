//! Read and write comparisons for a pair of protocol member accesses.

use super::{
    ProtocolMember, ProtocolMemberAccessMode, ProtocolMemberType, ProtocolMemberWrite,
    ProtocolMemberWriteType, protocol_apply_self_with_receiver,
};
use crate::Db;
use crate::types::constraints::ConstraintSet;
use crate::types::relation::TypeRelationChecker;
pub(super) use crate::types::relation::dependencies::{
    OrdinaryDependencies, RelationDependencies as MemberPairDependencies,
};
use crate::types::{ErrorContext, Type};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod scope_tests;

#[expect(
    clippy::large_enum_variant,
    reason = "keep the bounded pending state inline without allocating for each member comparison"
)]
pub(super) enum ProtocolMemberAccessPairStep<'checker, 'a, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Relate(PendingProtocolMemberRelation<'checker, 'a, 'c, 'db>),
}

impl<'checker, 'a, 'c, 'db> ProtocolMemberAccessPairStep<'checker, 'a, 'c, 'db> {
    pub(super) fn start<D: MemberPairDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        source_type: Type<'db>,
        source_member: &ProtocolMember<'_, 'db>,
        target_member: &ProtocolMember<'_, 'db>,
        access: ProtocolMemberAccessMode,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let source = dependencies.run(db, || source_member.access(db, checker.env, access))?;
        if access == ProtocolMemberAccessMode::Class
            && source_member.is_method()
            && target_member.is_instance_method()
        {
            // The instance-side check is authoritative for an ordinary method's signature. Class
            // access only establishes that the source member is also present on the class.
            return Ok(Self::Complete(ConstraintSet::from_bool(
                checker.constraints,
                source.read.is_some(),
            )));
        }
        let target = dependencies.run(db, || target_member.access(db, checker.env, access))?;
        let writes = WriteComparison {
            source_type,
            source: source.write.map(ProtocolMemberWrite::compatibility_type),
            target: target.write.map(ProtocolMemberWrite::compatibility_type),
        };
        let read_result = match (source.read, target.read) {
            (_, None) => checker.always(),
            (None, Some(_)) => checker.never(),
            (Some(source), Some(target)) => {
                let bind_read = |member_type: ProtocolMemberType<'db>,
                                 member: &ProtocolMember<'_, 'db>| {
                    let Some(member_type) =
                        dependencies.run(db, || member_type.resolve(db, checker.env))?
                    else {
                        return Ok(None);
                    };
                    dependencies.run(db, || {
                        if member.is_method()
                            && let Type::Callable(callable) = member_type.ty()
                        {
                            Some(Type::Callable(protocol_apply_self_with_receiver(
                                db,
                                checker.env.program(db),
                                callable,
                                source_type,
                                source_type,
                            )))
                        } else {
                            member_type.bind_self(db, checker.env, source_type)
                        }
                    })
                };
                let (Some(source), Some(target)) = (
                    bind_read(source, source_member)?,
                    bind_read(target, target_member)?,
                ) else {
                    return Ok(Self::Complete(checker.never()));
                };
                return Ok(Self::Relate(PendingProtocolMemberRelation {
                    checker,
                    source,
                    target,
                    continuation: MemberContinuation::AfterRead {
                        writes,
                        target_is_method: target_member.is_method(),
                    },
                }));
            }
        };
        writes.start(db, checker, read_result, dependencies)
    }
}

struct WriteComparison<'db> {
    source_type: Type<'db>,
    source: Option<ProtocolMemberWriteType<'db>>,
    target: Option<ProtocolMemberWriteType<'db>>,
}

impl<'db> WriteComparison<'db> {
    fn start<'checker, 'a, 'c, D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        read_result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberAccessPairStep<'checker, 'a, 'c, 'db>, D::Error> {
        // Match ConstraintSet::and's short circuit before resolving either writable type.
        if read_result.is_trivially_never_satisfied() {
            return Ok(ProtocolMemberAccessPairStep::Complete(read_result));
        }
        let write_result = match (self.source, self.target) {
            (_, None) => checker.always(),
            (None, Some(_)) => {
                if let Some(context) = checker.report_context() {
                    context.push(ErrorContext::ProtocolMemberNotWritable);
                }
                checker.never()
            }
            (Some(source), Some(target)) => {
                let (Some(target), Some(source)) = (
                    dependencies.run(db, || target.bind(db, checker.env, self.source_type))?,
                    dependencies.run(db, || source.bind(db, checker.env, self.source_type))?,
                ) else {
                    return dependencies
                        .run(db, || {
                            read_result.and(db, checker.constraints, || checker.never())
                        })
                        .map(ProtocolMemberAccessPairStep::Complete);
                };
                return Ok(ProtocolMemberAccessPairStep::Relate(
                    PendingProtocolMemberRelation {
                        checker,
                        source: target,
                        target: source,
                        continuation: MemberContinuation::AfterWrite { read_result },
                    },
                ));
            }
        };
        dependencies
            .run(db, || {
                read_result.and(db, checker.constraints, || write_result)
            })
            .map(ProtocolMemberAccessPairStep::Complete)
    }
}

enum MemberContinuation<'db, 'c> {
    AfterRead {
        writes: WriteComparison<'db>,
        target_is_method: bool,
    },
    AfterWrite {
        read_result: ConstraintSet<'db, 'c>,
    },
}

pub(super) struct PendingProtocolMemberRelation<'checker, 'a, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(super) source: Type<'db>,
    pub(super) target: Type<'db>,
    continuation: MemberContinuation<'db, 'c>,
}

impl<'checker, 'a, 'c, 'db> PendingProtocolMemberRelation<'checker, 'a, 'c, 'db> {
    /// Resumes a completed child comparison. An incomplete child drops this continuation.
    pub(super) fn resume<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberAccessPairStep<'checker, 'a, 'c, 'db>, D::Error> {
        match self.continuation {
            MemberContinuation::AfterRead {
                writes,
                target_is_method,
            } => {
                if let Some(context) = self.checker.report_context()
                    && !target_is_method
                    && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
                {
                    context.push(ErrorContext::ProtocolMemberReadTypeIncompatible {
                        source: self.source,
                        target: self.target,
                    });
                }
                writes.start(db, self.checker, result, dependencies)
            }
            MemberContinuation::AfterWrite { read_result } => {
                if let Some(context) = self.checker.report_context()
                    && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
                {
                    context.push(ErrorContext::ProtocolMemberWriteTypeIncompatible {
                        target: self.source,
                    });
                }
                dependencies
                    .run(db, || {
                        read_result.and(db, self.checker.constraints, || result)
                    })
                    .map(ProtocolMemberAccessPairStep::Complete)
            }
        }
    }
}
