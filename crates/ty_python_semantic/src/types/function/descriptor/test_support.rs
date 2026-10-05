//! Fixture construction and an independent normalization oracle for descriptor-update controls.

use super::super::{FunctionType, UpdatedFunctionSignatures};
use crate::Db;
use crate::types::callable::CallableTypeKind;
use crate::types::{CallableSignature, CallableType};

impl<'db> FunctionType<'db> {
    /// Installs the supplied stored signatures and implementation callables for runtime controls.
    /// The function's literal and descriptor override remain unchanged.
    pub(in crate::types) fn with_probe_updated_signatures(
        self,
        db: &'db dyn Db,
        signature: CallableSignature<'db>,
        implementation_callables: Box<[CallableType<'db>]>,
    ) -> Self {
        Self::new_internal(
            db,
            self.literal(db),
            UpdatedFunctionSignatures::new(Some(signature), Some(implementation_callables)),
            self.descriptor_kind(db),
        )
    }

    /// Normalizes a descriptor override independently of the shared effects implementation.
    /// When the requested kind matches the declaration, the result has no stored override;
    /// otherwise it stores the requested kind. Both representations preserve the stored payload.
    /// Keeping this implementation independent lets runtime controls detect changes in canonical
    /// identity or stored payloads even when the ordinary and controlled paths share an algorithm.
    pub(in crate::types) fn probe_descriptor_kind_oracle(
        self,
        db: &'db dyn Db,
        kind: CallableTypeKind,
    ) -> Self {
        let declared = Self::new_internal(db, self.literal(db), self.updated_signatures(db), None);
        if declared.callable_type_kind(db) == kind {
            return declared;
        }
        Self::new_internal(
            db,
            self.literal(db),
            self.updated_signatures(db),
            Some(kind),
        )
    }
}
