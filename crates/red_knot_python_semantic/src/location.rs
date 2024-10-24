use crate::{
    search::locate_name_on_type,
    semantic_index::{definition::Definition, use_def_map, SemanticIndex},
    Db, HasTy, SemanticModel,
};
use ruff_db::files::{location::Location, File};
use ruff_text_size::TextSize;

// XXX should I just use an alias here? Not getting much value out of this huge import
use ruff_python_ast::{
    Arguments, Comprehension, Decorator, DictItem, ElifElseClause, ExceptHandler, Expr,
    ExprAttribute, ExprAwait, ExprBinOp, ExprBoolOp, ExprCall, ExprCompare, ExprDict, ExprDictComp,
    ExprFString, ExprGenerator, ExprIf, ExprLambda, ExprList, ExprListComp, ExprName, ExprNamed,
    ExprSet, ExprSetComp, ExprSlice, ExprStarred, ExprSubscript, ExprTuple, ExprUnaryOp, ExprYield,
    ExprYieldFrom, ExpressionRef, FString, FStringExpressionElement, FStringPart, FStringValue,
    Identifier, Keyword, MatchCase, ModModule, Parameter, ParameterWithDefault, Parameters, Stmt,
    StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign, StmtClassDef, StmtDelete, StmtExpr,
    StmtFor, StmtFunctionDef, StmtGlobal, StmtIf, StmtImport, StmtImportFrom, StmtMatch,
    StmtNonlocal, StmtRaise, StmtReturn, StmtTry, StmtTypeAlias, StmtWhile, StmtWith, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple, TypeParams, WithItem,
};

pub(crate) fn location_from_definition<'db>(
    definition: Definition<'db>,
    index: &SemanticIndex<'db>,
    db: &dyn Db,
) -> Location {
    let range = index.definition_range(definition);
    return Location {
        file: definition.file(db),
        range,
    };
}

/// This trait is used to locate where something is defined
pub(crate) trait CanLocate<'db> {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location>;
}

impl<'db> CanLocate<'db> for Stmt {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            Stmt::FunctionDef(inner) => inner.locate_def(pos, index, db, file),
            Stmt::ClassDef(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Return(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Delete(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Assign(inner) => inner.locate_def(pos, index, db, file),
            Stmt::AugAssign(inner) => inner.locate_def(pos, index, db, file),
            Stmt::AnnAssign(inner) => inner.locate_def(pos, index, db, file),
            Stmt::TypeAlias(inner) => inner.locate_def(pos, index, db, file),
            Stmt::For(inner) => inner.locate_def(pos, index, db, file),
            Stmt::While(inner) => inner.locate_def(pos, index, db, file),
            Stmt::If(inner) => inner.locate_def(pos, index, db, file),
            Stmt::With(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Match(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Raise(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Try(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Assert(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Import(inner) => inner.locate_def(pos, index, db, file),
            Stmt::ImportFrom(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Global(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Nonlocal(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Expr(inner) => inner.locate_def(pos, index, db, file),
            Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::IpyEscapeCommand(_) => None,
        }
    }
}

impl<'db, T> CanLocate<'db> for Vec<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        for item in self {
            let lookup = item.locate_def(pos, index, db, file);
            if lookup.is_some() {
                return lookup;
            }
        }
        None
    }
}
// XXX can merge Vec and [T] into something else?
impl<'db, T> CanLocate<'db> for [T]
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        for item in self {
            let lookup = item.locate_def(pos, index, db, file);
            if lookup.is_some() {
                return lookup;
            }
        }
        None
    }
}

impl<'db, T> CanLocate<'db> for Box<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        self.as_ref().locate_def(pos, index, db, file)
    }
}
impl<'db, T> CanLocate<'db> for Option<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            None => None,
            Some(elt) => elt.locate_def(pos, index, db, file),
        }
    }
}

impl<'db> CanLocate<'db> for Expr {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            Expr::BoolOp(inner) => inner.locate_def(pos, index, db, file),
            Expr::Named(inner) => inner.locate_def(pos, index, db, file),
            Expr::BinOp(inner) => inner.locate_def(pos, index, db, file),
            Expr::UnaryOp(inner) => inner.locate_def(pos, index, db, file),
            Expr::Lambda(inner) => inner.locate_def(pos, index, db, file),
            Expr::If(inner) => inner.locate_def(pos, index, db, file),
            Expr::Dict(inner) => inner.locate_def(pos, index, db, file),
            Expr::Set(inner) => inner.locate_def(pos, index, db, file),
            Expr::ListComp(inner) => inner.locate_def(pos, index, db, file),
            Expr::SetComp(inner) => inner.locate_def(pos, index, db, file),
            Expr::DictComp(inner) => inner.locate_def(pos, index, db, file),
            Expr::Generator(inner) => inner.locate_def(pos, index, db, file),
            Expr::Await(inner) => inner.locate_def(pos, index, db, file),
            Expr::Yield(inner) => inner.locate_def(pos, index, db, file),
            Expr::YieldFrom(inner) => inner.locate_def(pos, index, db, file),
            Expr::Compare(inner) => inner.locate_def(pos, index, db, file),
            Expr::Call(inner) => inner.locate_def(pos, index, db, file),
            Expr::FString(inner) => inner.locate_def(pos, index, db, file),
            Expr::StringLiteral(_) => None,
            Expr::BytesLiteral(_) => None,
            Expr::NumberLiteral(_) => None,
            Expr::BooleanLiteral(_) => None,
            Expr::NoneLiteral(_) => None,
            Expr::EllipsisLiteral(_) => None,
            Expr::Attribute(inner) => inner.locate_def(pos, index, db, file),
            Expr::Subscript(inner) => inner.locate_def(pos, index, db, file),
            Expr::Starred(inner) => inner.locate_def(pos, index, db, file),
            Expr::Name(inner) => inner.locate_def(pos, index, db, file),
            Expr::List(inner) => inner.locate_def(pos, index, db, file),
            Expr::Tuple(inner) => inner.locate_def(pos, index, db, file),
            Expr::Slice(inner) => inner.locate_def(pos, index, db, file),
            Expr::IpyEscapeCommand(_) => None,
        }
    }
}
macro_rules! impl_can_locate {
    ($type:ty, ranged, $($field:ident),+) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(&self, pos: TextSize, index: &SemanticIndex<'db>, db: &'db dyn Db, file: File) -> Option<Location> {
                if !pos.in_range(&self.range) {
                    return None;
                }
                None
                    $(.or_else(|| self.$field.locate_def(pos, index, db, file)))+
            }
        }
    };
    // Case where `locate_def` directly forwards to a field.
    ($type:ty, $($field:ident),+) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(&self, pos: TextSize, index: &SemanticIndex<'db>, db: &'db dyn Db, file: File) -> Option<Location> {
                None
                    $(.or_else(|| self.$field.locate_def(pos, index, db, file)))+
            }
        }
    };


}
macro_rules! locate_todo {
    ($type: ty) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(
                &self,
                _pos: TextSize,
                _index: &SemanticIndex<'db>,
                _db: &'db dyn Db,
                _file: File,
            ) -> Option<Location> {
                None
            }
        }
    };
}
impl_can_locate!(StmtFor, ranged, target, iter, body, orelse);
impl_can_locate!(StmtDelete, ranged, targets);
impl_can_locate!(DictItem, value);
impl_can_locate!(ModModule, ranged, body);
impl_can_locate!(StmtFunctionDef, ranged, decorator_list, returns, body);
impl_can_locate!(StmtClassDef, ranged, decorator_list, arguments, body);
impl_can_locate!(StmtReturn, ranged, value);
impl_can_locate!(StmtGlobal, ranged, names);
impl_can_locate!(StmtNonlocal, ranged, names);
impl_can_locate!(Arguments, ranged, args, keywords);
impl_can_locate!(Keyword, value);
impl_can_locate!(Decorator, ranged, expression);
impl_can_locate!(ExprBoolOp, values);
impl_can_locate!(ExprNamed, value);
impl_can_locate!(ExprBinOp, left, right);
impl_can_locate!(ExprUnaryOp, ranged, operand);
impl_can_locate!(ExprLambda, ranged, parameters, body);
impl_can_locate!(ExprIf, ranged, test, body, orelse);
impl_can_locate!(ExprDict, ranged, items);
impl_can_locate!(ExprSet, ranged, elts);
impl_can_locate!(ExprListComp, ranged, elt, generators);
impl_can_locate!(ExprSetComp, ranged, elt, generators);
impl_can_locate!(ExprDictComp, ranged, key, value, generators);
impl_can_locate!(ExprGenerator, ranged, elt, generators);
impl_can_locate!(ExprAwait, ranged, value);
impl_can_locate!(ExprYield, ranged, value);
impl_can_locate!(ExprYieldFrom, ranged, value);
impl_can_locate!(ExprCompare, ranged, left, comparators);
impl_can_locate!(ExprCall, ranged, func, arguments);
impl_can_locate!(ExprFString, ranged, value);
impl_can_locate!(FStringExpressionElement, ranged, expression);
impl_can_locate!(Comprehension, ranged, target, iter, ifs);
impl_can_locate!(StmtWhile, ranged, test, body, orelse);
impl_can_locate!(StmtIf, ranged, test, body, elif_else_clauses);
impl_can_locate!(ElifElseClause, ranged, test, body);
impl_can_locate!(StmtWith, ranged, items, body);
impl_can_locate!(WithItem, ranged, context_expr, optional_vars);
impl_can_locate!(StmtMatch, ranged, subject, cases);
impl_can_locate!(StmtAssign, ranged, targets, value);
impl_can_locate!(StmtAugAssign, ranged, target, value);
impl_can_locate!(StmtAnnAssign, ranged, target, annotation, value);
impl_can_locate!(StmtTypeAlias, ranged, name, type_params, value);
impl_can_locate!(TypeParams, ranged, type_params);
impl_can_locate!(MatchCase, ranged, guard, body);
impl_can_locate!(StmtRaise, ranged, exc, cause);
impl_can_locate!(StmtTry, ranged, body, handlers, orelse, finalbody);
impl_can_locate!(StmtAssert, ranged, test, msg);
impl_can_locate!(
    Parameters,
    ranged,
    posonlyargs,
    args,
    vararg,
    kwonlyargs,
    kwarg
);
impl_can_locate!(ParameterWithDefault, ranged, parameter, default);
impl_can_locate!(Parameter, ranged, annotation);
locate_todo!(StmtImport);
locate_todo!(StmtImportFrom);
impl_can_locate!(StmtExpr, ranged, value);
locate_todo!(ExceptHandler);
impl<'db> CanLocate<'db> for TypeParam {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            TypeParam::TypeVar(inner) => inner.locate_def(pos, index, db, file),
            TypeParam::ParamSpec(inner) => inner.locate_def(pos, index, db, file),
            TypeParam::TypeVarTuple(inner) => inner.locate_def(pos, index, db, file),
        }
    }
}

impl_can_locate!(TypeParamTypeVar, ranged, bound, default);
impl_can_locate!(TypeParamParamSpec, ranged, default);
impl_can_locate!(TypeParamTypeVarTuple, ranged, default);
impl<'db> CanLocate<'db> for FStringValue {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        for part in self {
            let result = part.locate_def(pos, index, db, file);
            if result.is_some() {
                return result;
            }
        }
        None
    }
}

impl<'db> CanLocate<'db> for FStringPart {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            FStringPart::Literal(_) => None,
            FStringPart::FString(FString { elements, .. }) => {
                for expression in elements.expressions() {
                    let result = expression.locate_def(pos, index, db, file);
                    if result.is_some() {
                        return result;
                    }
                }
                None
            }
        }
    }
}

impl<'db> CanLocate<'db> for ExprAttribute {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        if !pos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        if pos.in_range(&self.attr.range) {
            // we're on the attribute itself
            // so we'll look up the type of the expr
            // (to determine where this attribute is)
            // XXX should I pass around a model instead of an index/file pair?
            let model = SemanticModel::new(db, file);
            let inner_ty = self.value.as_ref().ty(&model);

            // now that I have the inner type, let's try to find the name
            locate_name_on_type(db, index, &inner_ty, &self.attr)
        } else {
            // let's check out the inner expression
            self.value.locate_def(pos, index, db, file)
        }
    }
}

impl_can_locate!(ExprSubscript, ranged, value, slice);
impl_can_locate!(ExprStarred, ranged, value);

impl<'db> CanLocate<'db> for ExprName {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        if !pos.in_range(&self.range) {
            return None;
        }

        let file_scope_id = index.expression_scope_id(ExpressionRef::from(self));
        let scope = file_scope_id.to_scope_id(db, file);
        let scoped_use_id = index
            .ast_ids(file_scope_id)
            .use_id(ExpressionRef::from(self));
        let udm = use_def_map(db, scope);
        if let Some(binding) = udm.bindings_at_use(scoped_use_id).next() {
            // take first binding I find as the canonical one
            let definition: Definition<'db> = binding.binding;
            Some(location_from_definition(definition, index, db))
        } else {
            // couldn't find a definition (perhaps this is unbound?)
            None
        }
    }
}

impl_can_locate!(ExprList, ranged, elts);
impl_can_locate!(ExprTuple, ranged, elts);
impl_can_locate!(ExprSlice, ranged, lower, upper, step);

impl<'db> CanLocate<'db> for Identifier {
    fn locate_def(
        &self,
        _pos: TextSize,
        _index: &SemanticIndex<'db>,
        _db: &'db dyn Db,
        _file: File,
    ) -> Option<Location> {
        // TODO figure this one out
        None
    }
}
