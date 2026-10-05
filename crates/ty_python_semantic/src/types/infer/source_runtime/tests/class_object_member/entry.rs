//! Checks the complete canonical class-object entry beyond the namespace-only provider.

use super::*;
use crate::types::{DescriptorOrigin, member_lookup_ingredient, member_lookup_with_policy_impl};

const METHOD: &str = "class Product:\n    def method(self) -> bool:\n        return True\n";

/// Retains the real class definition until cold inference supplies the lookup receiver.
#[derive(Clone, Copy, Debug)]
struct EntryRequest<'name, 'db> {
    definition: Definition<'db>,
    name: &'name Name,
}

impl<'db> MemberOperation<'db> for EntryRequest<'_, 'db> {
    type Output = (Type<'db>, MemberLookupResult<'db>);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let inference = access.definition(self.definition).await?;
        let effects = SourceEffects::new(access, program);
        let receiver = effects
            .local_with_fixed_transfers(3, 0, || {
                inference
                    .original_class_type(self.definition)
                    .map(Type::ClassLiteral)
                    .ok_or(RunError::Contract("fixture definition is not a class"))
            })
            .await??;
        let result = access
            .member_lookup(receiver, self.name, MemberLookupPolicy::default())
            .await?;
        Ok((receiver, result))
    }
}

/// Cold class access preserves the unbound method and publishes the canonical full-entry memo.
/// A separate database supplies the ordinary semantic result. Repeated controlled lookup must
/// reuse the same memo; same-database ordinary dispatch also compares every member metadata field.
/// Rust is needed to assert the cold dependency capture and canonical memo address, which mdtests
/// cannot observe.
#[test]
fn cold_class_method_entry_preserves_function_and_reuses_canonical_memo() {
    let db = database(METHOD);
    let prepared = prepare(&db);
    let name = Name::new_static("method");
    let definition = request(&prepared, &name, Route::Provider).definition;
    let request = EntryRequest {
        definition,
        name: &name,
    };
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    observations::reset(None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete((receiver, result))) = cold.value else {
        panic!("cold class-object entry: {:?}", cold.value);
    };
    let resolved = result.expect("class method lookup must succeed");
    let member = resolved.member(&db);
    let Type::FunctionLiteral(function) = member.place.expect_type() else {
        panic!("class access must retain the unbound function: {member:?}");
    };
    let class_node = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(Stmt::as_class_def_stmt)
        .unwrap();
    let method_node = class_node
        .body
        .iter()
        .find_map(Stmt::as_function_def_stmt)
        .unwrap();
    let method_definition = prepared
        .semantic_index()
        .expect_single_definition(method_node);
    assert_eq!(function.last_definition(&db), method_definition);

    assert_eq!(resolved.descriptor_origin(&db), DescriptorOrigin::default());
    let key = MemberLookupKey::new(
        &db,
        prepared.program_file().program(&db),
        receiver,
        name.as_str(),
        MemberLookupPolicy::default(),
    );
    let ingredient = member_lookup_ingredient(&db);
    let database_key = ingredient.database_key_index(key.as_id());
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key.as_id()).is_ok());
    let first = cold
        .reads
        .iter()
        .find(|read| read.key == database_key)
        .unwrap();
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    warm.check_root_reads().unwrap();
    assert_eq!(warm.value, cold.value);
    assert!(
        warm.reads
            .iter()
            .any(|read| read.key == database_key && read.memo_address == first.memo_address)
    );
    assert_eq!(member_lookup_with_policy_impl(&db, key, None, None), result);

    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary_db = database(METHOD);
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ordinary_definition = super::request(&ordinary_prepared, &name, Route::Provider).definition;
    let ordinary_class = infer_definition_types(&ordinary_db, ordinary_definition)
        .original_class_type(ordinary_definition)
        .unwrap();
    let ordinary_key = MemberLookupKey::new(
        &ordinary_db,
        ordinary_prepared.program_file().program(&ordinary_db),
        Type::ClassLiteral(ordinary_class),
        name.as_str(),
        MemberLookupPolicy::default(),
    );
    let expected = member_lookup_with_policy_impl(&ordinary_db, ordinary_key, None, None).unwrap();
    let expected_member = expected.member(&ordinary_db);
    assert!(matches!(
        expected_member.place.expect_type(),
        Type::FunctionLiteral(_)
    ));
    assert_eq!(
        member.place.expect_type().display(&db, &env).to_string(),
        expected_member
            .place
            .expect_type()
            .display(&ordinary_db, &ordinary_env)
            .to_string()
    );
    assert_eq!(
        expected.descriptor_origin(&ordinary_db),
        DescriptorOrigin::default()
    );
    assert_cleanup();
}
