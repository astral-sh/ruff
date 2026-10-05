//! Admitted suffix renaming preserves raw metadata, binding, ParamSpec attributes, and freshness.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    FieldReadProfile, FieldRequest, FieldRequestContext, RunError, RunResult,
};
use ty_python_core::definition::Definition;

use super::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use super::{SourceAccess, SourceEffects};
use crate::types::TypeVarVariance;
use crate::types::typevar::name_suffix::{
    TypeVarNameSuffixEffects, TypeVarNameSuffixFacts, with_name_suffix_with,
};
use crate::types::typevar::{
    BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarBoundOrConstraintsEvaluation,
    TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarKind,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Creates the renamed variable used to give a returned callable its own type parameters.
    pub(super) async fn rename_return_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.type_parameter_future(|| {
            with_name_suffix_with(variable, "return", TypeVarNameSuffixFacts, self)
        })
        .await?
        .await
    }

    /// Admits construction and reading of the interned-field requests used by suffix renaming.
    async fn rename_interned_field<R, P, F>(&self, make: F, profile: &P) -> RunResult<R::Output>
    where
        R: FieldRequest<'db>,
        P: FieldReadProfile<R::Stored>,
        F: FnOnce(FieldRequestContext<'db>) -> R,
    {
        // Generated interned selectors contain only the request context and interned handle.
        // The returned request also contains its accessor and conversion pointers. Four request
        // carriers cover the selector and intermediate copies before the read future takes it.
        let bytes = Self::checked(size_of::<R>().checked_mul(4).and_then(|bytes| {
            bytes.checked_add(size_of::<FieldRequestContext<'db>>().checked_mul(4)?)
        }))?;
        let request = self
            .local_with_fixed_transfers(32, bytes, || {
                make(self.access.endpoint().field_request_context())
            })
            .await?;
        self.type_parameter_future(|| self.field_with_profile(request, profile))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVarNameSuffixEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // The shared body performs a fixed sequence of field transfers and one identity update.
        // Name copying and interning have their own charges; these widths measure carriers only.
        type Carriers<'a, 'db> = (
            BoundTypeVarInstance<'db>,
            TypeVarInstance<'db>,
            TypeVarIdentity<'db>,
            BoundTypeVarIdentity<'db>,
            Option<Definition<'db>>,
            TypeVarKind,
            Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
            Option<TypeVarVariance>,
            Option<TypeVarDefaultEvaluation<'db>>,
            &'a Name,
            &'a str,
            [&'a str; 3],
            Name,
        );
        let bytes = Self::checked(size_of::<Carriers<'_, 'db>>().checked_mul(4))?;
        self.local_with_fixed_transfers(64, bytes, || ()).await
    }

    async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> RunResult<TypeVarInstance<'db>> {
        self.rename_interned_field(
            |fields| bound.field_requests(fields).typevar(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn identity(&self, typevar: TypeVarInstance<'db>) -> RunResult<TypeVarIdentity<'db>> {
        self.rename_interned_field(
            |fields| typevar.field_requests(fields).identity(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn source_name(&self, identity: TypeVarIdentity<'db>) -> RunResult<&'db Name> {
        self.rename_interned_field(
            |fields| identity.field_requests(fields).name(),
            &FixedFieldBorrow,
        )
        .await
    }

    async fn concatenate_name(&self, parts: [&str; 3]) -> RunResult<Name> {
        let length = Self::checked(
            parts[0]
                .len()
                .checked_add(parts[1].len())
                .and_then(|length| length.checked_add(parts[2].len())),
        )?;
        let work = Self::checked(length.checked_mul(2).and_then(|work| work.checked_add(32)))?;
        // Name::concat uses inline storage or one exact CharStr allocation. The extra words
        // cover its reference count and optional heap length. Final release needs constant work.
        let bytes = Self::checked(
            length
                .checked_add(size_of::<usize>() * 2)
                .and_then(|bytes| bytes.checked_add(size_of::<[&str; 3]>().checked_mul(4)?))
                .and_then(|bytes| bytes.checked_add(size_of::<Name>().checked_mul(4)?)),
        )?;
        self.local_with_fixed_transfers(work, bytes, || Name::concat(&parts))
            .await
    }

    async fn definition(
        &self,
        identity: TypeVarIdentity<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        self.rename_interned_field(
            |fields| identity.field_requests(fields).definition(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn kind(&self, identity: TypeVarIdentity<'db>) -> RunResult<TypeVarKind> {
        self.rename_interned_field(
            |fields| identity.field_requests(fields).kind(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn intern_identity(
        &self,
        name: &Name,
        definition: Option<Definition<'db>>,
        kind: TypeVarKind,
    ) -> RunResult<TypeVarIdentity<'db>> {
        self.type_parameter_future(|| self.access.intern_typevar_identity(name, definition, kind))
            .await?
            .await
    }

    async fn bounds(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraintsEvaluation<'db>>> {
        self.rename_interned_field(
            |fields| typevar.bound_or_constraints_request(fields),
            &FixedFieldCopy,
        )
        .await
    }

    async fn variance(&self, typevar: TypeVarInstance<'db>) -> RunResult<Option<TypeVarVariance>> {
        self.rename_interned_field(
            |fields| typevar.field_requests(fields).explicit_variance(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn default(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarDefaultEvaluation<'db>>> {
        self.rename_interned_field(|fields| typevar.default_request(fields), &FixedFieldCopy)
            .await
    }

    async fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        self.type_parameter_future(|| {
            self.access
                .intern_typevar_instance(identity, bounds, variance, default)
        })
        .await?
        .await
    }

    async fn bound_identity(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.rename_interned_field(|fields| bound.identity_request(fields), &FixedFieldCopy)
            .await
    }

    async fn intern_bound(
        &self,
        typevar: TypeVarInstance<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.type_parameter_future(|| self.access.intern_bound_typevar(typevar, identity))
            .await?
            .await
    }
}
