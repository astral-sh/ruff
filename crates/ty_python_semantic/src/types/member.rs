use crate::Db;
use crate::place::{
    ConsideredDefinitions, Place, PlaceAndQualifiers, RequiresExplicitReExport, place_by_id,
    place_from_bindings,
};
use crate::semantic_index::{place_table, scope::ScopeId, use_def_map};

pub(super) type Member<'db> = PlaceAndQualifiers<'db>;

/// Infer the public type of a class member/symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(super) fn class_member<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Member<'db> {
    place_table(db, scope)
        .symbol_id(name)
        .map(|symbol_id| {
            let member = place_by_id(
                db,
                scope,
                symbol_id.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            );

            if !member.is_undefined() && !member.is_init_var() {
                // Trust the declared type if we see a class-level declaration
                return member;
            }

            if let Member {
                place: Place::Defined(ty, _, _),
                qualifiers,
            } = member
            {
                // Otherwise, we need to check if the symbol has bindings
                let use_def = use_def_map(db, scope);
                let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                let inferred = place_from_bindings(db, bindings);

                // TODO: we should not need to calculate inferred type second time. This is a temporary
                // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
                match inferred {
                    Place::Undefined => Place::Undefined.with_qualifiers(qualifiers),
                    Place::Defined(_, origin, boundness) => {
                        Place::Defined(ty, origin, boundness).with_qualifiers(qualifiers)
                    }
                }
            } else {
                Member::unbound()
            }
        })
        .unwrap_or_default()
}
