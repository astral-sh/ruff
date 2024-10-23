use crate::{
    search::locate_name_on_type,
    semantic_index::{definition::Definition, use_def_map, SemanticIndex},
    Db, HasTy, SemanticModel,
};
use lsp_types::{Position, Range};
use ruff_db::{
    files::File,
    source::{line_index, source_text},
};
use ruff_text_size::TextRange;
use url::Url;

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

#[derive(Debug)]
pub enum DefLocation {
    // XXX not including lsp_types in here would be good
    // same for Url
    Location { url: Url, range: Range },
    Todo { s: String },
}

impl DefLocation {
    pub(crate) fn from_definition<'db>(
        definition: Definition<'db>,
        index: &SemanticIndex<'db>,
        db: &dyn Db,
        file: File,
    ) -> DefLocation {
        let range = index.definition_range(&definition);
        let final_range = ruff_range_to_lsp_range(db, file, range);
        return DefLocation::Location {
            url: definition.file(db).try_url(db.upcast()),
            range: final_range,
        };
    }
}

fn ruff_range_to_lsp_range(db: &dyn Db, file: File, range: TextRange) -> lsp_types::Range {
    let li = line_index(db.upcast(), file);
    let contents = source_text(db.upcast(), file);
    let loc_start = li.source_location(range.start(), &contents);
    let loc_end = li.source_location(range.end(), &contents);
    lsp_types::Range {
        start: lsp_types::Position::new(
            // XXX very wrong probably
            loc_start.row.to_zero_indexed() as u32,
            loc_start.column.to_zero_indexed() as u32,
        ),
        end: lsp_types::Position::new(
            // XXX very wrong probably
            loc_end.row.to_zero_indexed() as u32,
            loc_end.column.to_zero_indexed() as u32,
        ),
    }
}

// this is a position as number of characters from the start
// XXX find something to use instead of this
pub struct CPosition(pub u64);

// LSP position... lsp-types
impl From<Position> for CPosition {
    fn from(_value: Position) -> Self {
        todo!()
    }
}

impl CPosition {
    fn in_range(&self, range: &TextRange) -> bool {
        (u64::from(range.start().to_u32()) <= self.0) && (u64::from(range.end().to_u32()) >= self.0)
    }
}

/// This trait is used to locate where something is defined
pub(crate) trait CanLocate<'db> {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation>;
}

impl<'db> CanLocate<'db> for Stmt {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            Stmt::FunctionDef(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::ClassDef(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Return(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Delete(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Assign(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::AugAssign(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::AnnAssign(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::TypeAlias(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::For(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::While(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::If(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::With(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Match(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Raise(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Try(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Assert(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Import(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::ImportFrom(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Global(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Nonlocal(inner) => inner.locate_def(cpos, index, db, file),
            Stmt::Expr(inner) => inner.locate_def(cpos, index, db, file),
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
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        for item in self {
            let lookup = item.locate_def(cpos, index, db, file);
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
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        for item in self {
            let lookup = item.locate_def(cpos, index, db, file);
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
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        self.as_ref().locate_def(cpos, index, db, file)
    }
}
impl<'db, T> CanLocate<'db> for Option<T>
where
    T: CanLocate<'db>,
{
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            None => None,
            Some(elt) => elt.locate_def(cpos, index, db, file),
        }
    }
}

impl<'db> CanLocate<'db> for Expr {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            Expr::BoolOp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Named(inner) => inner.locate_def(cpos, index, db, file),
            Expr::BinOp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::UnaryOp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Lambda(inner) => inner.locate_def(cpos, index, db, file),
            Expr::If(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Dict(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Set(inner) => inner.locate_def(cpos, index, db, file),
            Expr::ListComp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::SetComp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::DictComp(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Generator(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Await(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Yield(inner) => inner.locate_def(cpos, index, db, file),
            Expr::YieldFrom(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Compare(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Call(inner) => inner.locate_def(cpos, index, db, file),
            Expr::FString(inner) => inner.locate_def(cpos, index, db, file),
            Expr::StringLiteral(_) => None,
            Expr::BytesLiteral(_) => None,
            Expr::NumberLiteral(_) => None,
            Expr::BooleanLiteral(_) => None,
            Expr::NoneLiteral(_) => None,
            Expr::EllipsisLiteral(_) => None,
            Expr::Attribute(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Subscript(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Starred(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Name(inner) => inner.locate_def(cpos, index, db, file),
            Expr::List(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Tuple(inner) => inner.locate_def(cpos, index, db, file),
            Expr::Slice(inner) => inner.locate_def(cpos, index, db, file),
            Expr::IpyEscapeCommand(_) => None,
        }
    }
}
macro_rules! impl_can_locate {
    ($type:ty, ranged, $($field:ident),+) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(&self, cpos: &CPosition, index: &SemanticIndex<'db>, db: &'db dyn Db, file: File) -> Option<DefLocation> {
                if !cpos.in_range(&self.range) {
                    return None;
                }
                None
                    $(.or_else(|| self.$field.locate_def(cpos, index, db, file)))+
            }
        }
    };
    // Case where `locate_def` directly forwards to a field.
    ($type:ty, $($field:ident),+) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(&self, cpos: &CPosition, index: &SemanticIndex<'db>, db: &'db dyn Db, file: File) -> Option<DefLocation> {
                None
                    $(.or_else(|| self.$field.locate_def(cpos, index, db, file)))+
            }
        }
    };


}
macro_rules! locate_todo {
    ($type: ty) => {
        impl<'db> CanLocate<'db> for $type {
            fn locate_def(
                &self,
                _cpos: &CPosition,
                _index: &SemanticIndex<'db>,
                _db: &'db dyn Db,
                _file: File,
            ) -> Option<DefLocation> {
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
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            TypeParam::TypeVar(inner) => inner.locate_def(cpos, index, db, file),
            TypeParam::ParamSpec(inner) => inner.locate_def(cpos, index, db, file),
            TypeParam::TypeVarTuple(inner) => inner.locate_def(cpos, index, db, file),
        }
    }
}

impl_can_locate!(TypeParamTypeVar, ranged, bound, default);
impl_can_locate!(TypeParamParamSpec, ranged, default);
impl_can_locate!(TypeParamTypeVarTuple, ranged, default);
impl<'db> CanLocate<'db> for FStringValue {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        for part in self {
            let result = part.locate_def(cpos, index, db, file);
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
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        match self {
            FStringPart::Literal(_) => None,
            FStringPart::FString(FString { elements, .. }) => {
                for expression in elements.expressions() {
                    let result = expression.locate_def(cpos, index, db, file);
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
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        if cpos.in_range(&self.attr.range) {
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
            self.value.locate_def(cpos, index, db, file)
        }
    }
}

impl<'db> CanLocate<'db> for ExprSubscript {
    fn locate_def(
        &self,
        cpos: &CPosition,
        _index: &SemanticIndex<'db>,
        _db: &'db dyn Db,
        _file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }
        // we're definitely in here!
        Some(DefLocation::Todo {
            s: "Subscript Access!".to_string(),
        })
    }
}

impl_can_locate!(ExprStarred, ranged, value);

impl<'db> CanLocate<'db> for ExprName {
    fn locate_def(
        &self,
        cpos: &CPosition,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<DefLocation> {
        if !cpos.in_range(&self.range) {
            return None;
        }

        let file_scope_id = index.expression_scope_id(ExpressionRef::from(self));
        let scope = file_scope_id.to_scope_id(db, file);
        let scoped_use_id = index
            .ast_ids(file_scope_id)
            .use_id(ExpressionRef::from(self));
        let udm = use_def_map(db, scope);
        for binding in udm.bindings_at_use(scoped_use_id) {
            // take first binding I find
            let definition: Definition<'db> = binding.binding;
            return Some(DefLocation::from_definition(definition, index, db, file));
        }
        Some(DefLocation::Todo {
            s: "Name Access!".to_string(),
        })
    }
}

impl_can_locate!(ExprList, ranged, elts);
impl_can_locate!(ExprTuple, ranged, elts);
impl_can_locate!(ExprSlice, ranged, lower, upper, step);

impl<'db> CanLocate<'db> for Identifier {
    fn locate_def(
        &self,
        _cpos: &CPosition,
        _index: &SemanticIndex<'db>,
        _db: &'db dyn Db,
        _file: File,
    ) -> Option<DefLocation> {
        // TODO figure this one out
        None
    }
}
