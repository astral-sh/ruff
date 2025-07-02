use crate::FxOrderSet;
use crate::types::Type;

#[derive(Debug, Default)]
pub(crate) struct TypeVisitor<'db> {
    seen: FxOrderSet<Type<'db>>,
}

impl<'db> TypeVisitor<'db> {
    pub(crate) fn visit(
        &mut self,
        ty: Type<'db>,
        func: impl FnOnce(&mut Self) -> Type<'db>,
    ) -> Type<'db> {
        if !self.seen.insert(ty) {
            // TODO: proper recursive type handling

            // This must be Any, not e.g. a todo type, because Any is the normalized form of the
            // dynamic type (that is, todo types are normalized to Any).
            return Type::any();
        }
        let ret = func(self);
        self.seen.pop();
        ret
    }
}
