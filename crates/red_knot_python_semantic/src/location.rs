use crate::{
    search::locate_name_on_type,
    semantic_index::{definition::Definition, use_def_map, SemanticIndex},
    Db, HasTy, SemanticModel,
};
use ruff_db::files::{location::Location, File};
use ruff_text_size::TextSize;

use ruff_python_ast as ast;

///
/// Given a definition, find the location of the definition.
///
pub(crate) fn location_from_definition<'db>(
    definition: Definition<'db>,
    index: &SemanticIndex<'db>,
    db: &dyn Db,
) -> Location {
    let range = index.definition_range(definition);
    Location {
        file: definition.file(db),
        range,
    }
}

pub(crate) trait CanLocate<'db> {
    ///
    /// Given a position in a file, try and find the definition for whatever
    /// is underneath that position, then return the location of that definition.
    ///
    /// In the case where the position is outside of the range controlled, or
    /// when nothing is found, return None.
    ///
    /// TODO currently this doesn't differentiate between "this position is outside
    /// of my range" and "this position is within my range, but I could not find a
    /// definition". This means that certain forms of short circuiting in the "there's
    /// no definition to be found" case are not happenign
    ///
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location>;
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
        self.iter()
            .find_map(|item| item.locate_def(pos, index, db, file))
    }
}
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
        self.iter()
            .find_map(|item| item.locate_def(pos, index, db, file))
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
        self.as_ref()
            .and_then(|elt| elt.locate_def(pos, index, db, file))
    }
}

macro_rules! impl_can_locate {
    // If an item has self.range, we can use it to quickly rule out problematic branches
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

// for the most part, location is just traversing the AST looking for
// the smallest AST node that has our position, without going over.
//
// This in practice turns into just checking attributes across various attributes
// on our AST nodes. Unlike with walking, where we are walking across all the branches,
// here we are just looking for one target and will return early once we find that
//
// For Enums in particular the macro isn't smart enough to handle that, so we generally
// have broken those out below
impl_can_locate!(ast::StmtFor, ranged, target, iter, body, orelse);
impl_can_locate!(ast::StmtDelete, ranged, targets);
impl_can_locate!(ast::DictItem, value);
impl_can_locate!(ast::ModModule, ranged, body);
impl_can_locate!(ast::StmtFunctionDef, ranged, decorator_list, returns, body);
impl_can_locate!(ast::StmtClassDef, ranged, decorator_list, arguments, body);
impl_can_locate!(ast::StmtReturn, ranged, value);
impl_can_locate!(ast::StmtGlobal, ranged, names);
impl_can_locate!(ast::StmtNonlocal, ranged, names);
impl_can_locate!(ast::Arguments, ranged, args, keywords);
impl_can_locate!(ast::Keyword, value);
impl_can_locate!(ast::Decorator, ranged, expression);
impl_can_locate!(ast::ExprBoolOp, values);
impl_can_locate!(ast::ExprNamed, value);
impl_can_locate!(ast::ExprBinOp, left, right);
impl_can_locate!(ast::ExprUnaryOp, ranged, operand);
impl_can_locate!(ast::ExprLambda, ranged, parameters, body);
impl_can_locate!(ast::ExprIf, ranged, test, body, orelse);
impl_can_locate!(ast::ExprDict, ranged, items);
impl_can_locate!(ast::ExprSet, ranged, elts);
impl_can_locate!(ast::ExprListComp, ranged, elt, generators);
impl_can_locate!(ast::ExprSetComp, ranged, elt, generators);
impl_can_locate!(ast::ExprDictComp, ranged, key, value, generators);
impl_can_locate!(ast::ExprGenerator, ranged, elt, generators);
impl_can_locate!(ast::ExprAwait, ranged, value);
impl_can_locate!(ast::ExprYield, ranged, value);
impl_can_locate!(ast::ExprYieldFrom, ranged, value);
impl_can_locate!(ast::ExprCompare, ranged, left, comparators);
impl_can_locate!(ast::ExprCall, ranged, func, arguments);
impl_can_locate!(ast::ExprFString, ranged, value);
impl_can_locate!(ast::FStringExpressionElement, ranged, expression);
impl_can_locate!(ast::Comprehension, ranged, target, iter, ifs);
impl_can_locate!(ast::StmtWhile, ranged, test, body, orelse);
impl_can_locate!(ast::StmtIf, ranged, test, body, elif_else_clauses);
impl_can_locate!(ast::ElifElseClause, ranged, test, body);
impl_can_locate!(ast::StmtWith, ranged, items, body);
impl_can_locate!(ast::WithItem, ranged, context_expr, optional_vars);
impl_can_locate!(ast::StmtMatch, ranged, subject, cases);
impl_can_locate!(ast::StmtAssign, ranged, targets, value);
impl_can_locate!(ast::StmtAugAssign, ranged, target, value);
impl_can_locate!(ast::StmtAnnAssign, ranged, target, annotation, value);
impl_can_locate!(ast::StmtTypeAlias, ranged, name, type_params, value);
impl_can_locate!(ast::TypeParams, ranged, type_params);
impl_can_locate!(ast::MatchCase, ranged, guard, body);
impl_can_locate!(ast::StmtRaise, ranged, exc, cause);
impl_can_locate!(ast::StmtTry, ranged, body, handlers, orelse, finalbody);
impl_can_locate!(ast::StmtAssert, ranged, test, msg);
impl_can_locate!(
    ast::Parameters,
    ranged,
    posonlyargs,
    args,
    vararg,
    kwonlyargs,
    kwarg
);
impl_can_locate!(ast::ParameterWithDefault, ranged, parameter, default);
impl_can_locate!(ast::Parameter, ranged, annotation);
impl_can_locate!(ast::StmtExpr, ranged, value);
impl_can_locate!(ast::TypeParamTypeVar, ranged, bound, default);
impl_can_locate!(ast::TypeParamParamSpec, ranged, default);
impl_can_locate!(ast::TypeParamTypeVarTuple, ranged, default);
impl_can_locate!(ast::ExprSubscript, ranged, value, slice);
impl_can_locate!(ast::ExprStarred, ranged, value);
impl_can_locate!(ast::ExprList, ranged, elts);
impl_can_locate!(ast::ExprTuple, ranged, elts);
impl_can_locate!(ast::ExprSlice, ranged, lower, upper, step);

// these ones just bail instantly, but really should be expanded
locate_todo!(ast::StmtImport);
locate_todo!(ast::StmtImportFrom);
locate_todo!(ast::ExceptHandler);
locate_todo!(ast::Identifier);

impl<'db> CanLocate<'db> for ast::Expr {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            ast::Expr::BoolOp(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Named(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::BinOp(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::UnaryOp(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Lambda(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::If(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Dict(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Set(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::ListComp(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::SetComp(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::DictComp(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Generator(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Await(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Yield(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::YieldFrom(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Compare(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Call(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::FString(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::StringLiteral(_) => None,
            ast::Expr::BytesLiteral(_) => None,
            ast::Expr::NumberLiteral(_) => None,
            ast::Expr::BooleanLiteral(_) => None,
            ast::Expr::NoneLiteral(_) => None,
            ast::Expr::EllipsisLiteral(_) => None,
            ast::Expr::Attribute(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Subscript(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Starred(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Name(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::List(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Tuple(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::Slice(inner) => inner.locate_def(pos, index, db, file),
            ast::Expr::IpyEscapeCommand(_) => None,
        }
    }
}
impl<'db> CanLocate<'db> for ast::Stmt {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            ast::Stmt::FunctionDef(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::ClassDef(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Return(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Delete(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Assign(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::AugAssign(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::AnnAssign(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::TypeAlias(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::For(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::While(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::If(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::With(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Match(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Raise(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Try(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Assert(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Import(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::ImportFrom(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Global(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Nonlocal(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Expr(inner) => inner.locate_def(pos, index, db, file),
            ast::Stmt::Pass(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_) => None,
        }
    }
}

impl<'db> CanLocate<'db> for ast::TypeParam {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            ast::TypeParam::TypeVar(inner) => inner.locate_def(pos, index, db, file),
            ast::TypeParam::ParamSpec(inner) => inner.locate_def(pos, index, db, file),
            ast::TypeParam::TypeVarTuple(inner) => inner.locate_def(pos, index, db, file),
        }
    }
}

impl<'db> CanLocate<'db> for ast::FStringValue {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        self.iter()
            .find_map(|item| item.locate_def(pos, index, db, file))
    }
}

impl<'db> CanLocate<'db> for ast::FStringPart {
    fn locate_def(
        &self,
        pos: TextSize,
        index: &SemanticIndex<'db>,
        db: &'db dyn Db,
        file: File,
    ) -> Option<Location> {
        match self {
            ast::FStringPart::Literal(_) => None,
            ast::FStringPart::FString(ast::FString { elements, .. }) => elements
                .expressions()
                .find_map(|expression| expression.locate_def(pos, index, db, file)),
        }
    }
}

impl<'db> CanLocate<'db> for ast::ExprAttribute {
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

impl<'db> CanLocate<'db> for ast::ExprName {
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

        let file_scope_id = index.expression_scope_id(ast::ExpressionRef::from(self));
        let scope = file_scope_id.to_scope_id(db, file);
        let scoped_use_id = index
            .ast_ids(file_scope_id)
            .use_id(ast::ExpressionRef::from(self));
        let udm = use_def_map(db, scope);
        let binding = udm.bindings_at_use(scoped_use_id).next()?;
        // take first binding I find as the canonical one
        let definition: Definition<'db> = binding.binding;
        Some(location_from_definition(definition, index, db))
    }
}
