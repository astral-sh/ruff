#![allow(non_snake_case)]

use libcst_native::{
    AnnAssign, Annotation, Arg, AsName, Assert, Assign, AssignEqual, AssignTarget,
    AssignTargetExpression, Asynchronous, Attribute, AugAssign, Await, BaseSlice, BinaryOp,
    BinaryOperation, BooleanOp, BooleanOperation, Break, Call, ClassDef, CompFor, CompIf, CompOp,
    Comparison, ComparisonTarget, CompoundStatement, ConcatenatedString, Continue, Decorator, Del,
    DelTargetExpression, Dict, DictComp, DictElement, Element, Ellipsis, Else, ExceptHandler,
    ExceptStarHandler, Expr, Expression, Finally, Float, For, FormattedString,
    FormattedStringContent, FormattedStringExpression, FormattedStringText, From, FunctionDef,
    GeneratorExp, Global, If, IfExp, Imaginary, Import, ImportAlias, ImportFrom, ImportNames,
    ImportStar, IndentedBlock, Index, Integer, Lambda, List, ListComp, Match, Module, Name,
    NameItem, NameOrAttribute, NamedExpr, Nonlocal, OrElse, Param, ParamStar, Parameters, Pass,
    Raise, Return, Set, SetComp, SimpleStatementLine, SimpleStatementSuite, SimpleString, Slice,
    SmallStatement, StarArg, StarredDictElement, StarredElement, Statement, Subscript,
    SubscriptElement, Suite, Try, TryStar, Tuple, UnaryOp, UnaryOperation, While, With, WithItem,
    Yield, YieldValue,
};

pub trait CSTVisitor {
    fn visit_Module<'a>(&mut self, node: &'a Module<'a>) -> Module<'a> {
        walk_Module(self, node)
    }
    fn visit_Statement<'a>(&mut self, node: &'a Statement<'a>) -> Option<Statement<'a>> {
        walk_Statement(self, node)
    }
    fn visit_SimpleStatementLine<'a>(
        &mut self,
        node: &'a SimpleStatementLine<'a>,
    ) -> Option<SimpleStatementLine<'a>> {
        walk_SimpleStatementLine(self, node)
    }
    fn visit_CompoundStatement<'a>(
        &mut self,
        node: &'a CompoundStatement<'a>,
    ) -> Option<CompoundStatement<'a>> {
        walk_CompoundStatement(self, node)
    }
    fn visit_SmallStatement(&mut self, node: &SmallStatement) {
        walk_SmallStatement(self, node);
    }
    fn visit_Expression(&mut self, node: &Expression) {
        walk_Expression(self, node);
    }
    fn visit_AnnAssign(&mut self, node: &AnnAssign) {
        walk_AnnAssign(self, node);
    }
    fn visit_Annotation(&mut self, node: &Annotation) {
        walk_Annotation(self, node);
    }
    fn visit_Arg(&mut self, node: &Arg) {
        walk_Arg(self, node);
    }
    fn visit_AsName(&mut self, node: &AsName) {
        walk_AsName(self, node);
    }
    fn visit_Assert(&mut self, node: &Assert) {
        walk_Assert(self, node);
    }
    fn visit_Assign(&mut self, node: &Assign) {
        walk_Assign(self, node);
    }
    fn visit_AssignEqual(&mut self, node: &AssignEqual) {
        walk_AssignEqual(self, node);
    }
    fn visit_AssignTarget(&mut self, node: &AssignTarget) {
        walk_AssignTarget(self, node);
    }
    fn visit_AssignTargetExpression(&mut self, node: &AssignTargetExpression) {
        walk_AssignTargetExpression(self, node);
    }
    fn visit_Asynchronous(&mut self, node: &Asynchronous) {
        walk_Asynchronous(self, node);
    }
    fn visit_Attribute(&mut self, node: &Attribute) {
        walk_Attribute(self, node);
    }
    fn visit_AugAssign(&mut self, node: &AugAssign) {
        walk_AugAssign(self, node);
    }
    fn visit_Await(&mut self, node: &Await) {
        walk_Await(self, node);
    }
    fn visit_BinaryOperation(&mut self, node: &BinaryOperation) {
        walk_BinaryOperation(self, node);
    }
    fn visit_BinaryOp(&mut self, node: &BinaryOp) {
        walk_BinaryOp(self, node);
    }
    fn visit_BooleanOperation(&mut self, node: &BooleanOperation) {
        walk_BooleanOperation(self, node);
    }
    fn visit_BooleanOp(&mut self, node: &BooleanOp) {
        walk_BooleanOp(self, node);
    }
    fn visit_Break(&mut self, node: &Break) {
        walk_Break(self, node);
    }
    fn visit_Call(&mut self, node: &Call) {
        walk_Call(self, node);
    }
    fn visit_ClassDef<'a>(&mut self, node: &'a ClassDef<'a>) -> ClassDef<'a> {
        walk_ClassDef(self, node)
    }
    fn visit_CompFor(&mut self, node: &CompFor) {
        walk_CompFor(self, node);
    }
    fn visit_CompIf(&mut self, node: &CompIf) {
        walk_CompIf(self, node);
    }
    fn visit_Comparison(&mut self, node: &Comparison) {
        walk_Comparison(self, node);
    }
    fn visit_ComparisonTarget(&mut self, node: &ComparisonTarget) {
        walk_ComparisonTarget(self, node);
    }
    fn visit_CompOp(&mut self, node: &CompOp) {
        walk_CompOp(self, node);
    }
    fn visit_ConcatenatedString(&mut self, node: &ConcatenatedString) {
        walk_ConcatenatedString(self, node);
    }
    fn visit_Continue(&mut self, node: &Continue) {
        walk_Continue(self, node);
    }
    fn visit_Decorator(&mut self, node: &Decorator) {
        walk_Decorator(self, node);
    }
    fn visit_Del(&mut self, node: &Del) {
        walk_Del(self, node);
    }
    fn visit_DelTargetExpression(&mut self, node: &DelTargetExpression) {
        walk_DelTargetExpression(self, node);
    }
    fn visit_Dict(&mut self, node: &Dict) {
        walk_Dict(self, node);
    }
    fn visit_DictComp(&mut self, node: &DictComp) {
        walk_DictComp(self, node);
    }
    fn visit_DictElement(&mut self, node: &DictElement) {
        walk_DictElement(self, node);
    }
    fn visit_Element(&mut self, node: &Element) {
        walk_Element(self, node);
    }
    fn visit_Ellipsis(&mut self, node: &Ellipsis) {
        walk_Ellipsis(self, node);
    }
    fn visit_Else(&mut self, node: &Else) {
        walk_Else(self, node);
    }
    fn visit_ExceptHandler(&mut self, node: &ExceptHandler) {
        walk_ExceptHandler(self, node);
    }
    fn visit_ExceptStarHandler(&mut self, node: &ExceptStarHandler) {
        walk_ExceptStarHandler(self, node);
    }
    fn visit_Expr(&mut self, node: &Expr) {
        walk_Expr(self, node);
    }
    fn visit_Finally(&mut self, node: &Finally) {
        walk_Finally(self, node);
    }
    fn visit_Float(&mut self, node: &Float) {
        walk_Float(self, node);
    }
    fn visit_For(&mut self, node: &For) {
        walk_For(self, node);
    }
    fn visit_FormattedString(&mut self, node: &FormattedString) {
        walk_FormattedString(self, node);
    }
    fn visit_FormattedStringExpression(&mut self, node: &FormattedStringExpression) {
        walk_FormattedStringExpression(self, node);
    }
    fn visit_FormattedStringText(&mut self, node: &FormattedStringText) {
        walk_FormattedStringText(self, node);
    }
    fn visit_From(&mut self, node: &From) {
        walk_From(self, node);
    }
    fn visit_FunctionDef(&mut self, node: &FunctionDef) {
        walk_FunctionDef(self, node);
    }
    fn visit_GeneratorExp(&mut self, node: &GeneratorExp) {
        walk_GeneratorExp(self, node);
    }
    fn visit_Global(&mut self, node: &Global) {
        walk_Global(self, node);
    }
    fn visit_If(&mut self, node: &If) {
        walk_If(self, node);
    }
    fn visit_IfExp(&mut self, node: &IfExp) {
        walk_IfExp(self, node);
    }
    fn visit_Imaginary(&mut self, node: &Imaginary) {
        walk_Imaginary(self, node);
    }
    fn visit_Import(&mut self, node: &Import) {
        walk_Import(self, node);
    }
    fn visit_ImportAlias(&mut self, node: &ImportAlias) {
        walk_ImportAlias(self, node);
    }
    fn visit_ImportFrom(&mut self, node: &ImportFrom) {
        walk_ImportFrom(self, node);
    }
    fn visit_ImportStar(&mut self, node: &ImportStar) {
        walk_ImportStar(self, node);
    }
    fn visit_IndentedBlock(&mut self, node: &IndentedBlock) {
        walk_IndentedBlock(self, node);
    }
    fn visit_Index(&mut self, node: &Index) {
        walk_Index(self, node);
    }
    fn visit_Integer(&mut self, node: &Integer) {
        walk_Integer(self, node);
    }
    fn visit_Lambda(&mut self, node: &Lambda) {
        walk_Lambda(self, node);
    }
    fn visit_List(&mut self, node: &List) {
        walk_List(self, node);
    }
    fn visit_ListComp(&mut self, node: &ListComp) {
        walk_ListComp(self, node);
    }
    fn visit_Match(&mut self, node: &Match) {
        walk_Match(self, node);
    }

    // fn visit_MatchAs(&mut self, node: &MatchAs) { walk_MatchAs(self, node); }
    // fn visit_MatchCase(&mut self, node: &MatchCase) { walk_MatchCase(self, node); }
    // fn visit_MatchClass(&mut self, node: &MatchClass) { walk_MatchClass(self, node); }
    // fn visit_MatchKeywordElement(&mut self, node: &MatchKeywordElement) { walk_MatchKeywordElement(self, node); }
    // fn visit_MatchList(&mut self, node: &MatchList) { walk_MatchList(self, node); }
    // fn visit_MatchMapping(&mut self, node: &MatchMapping) { walk_MatchMapping(self, node); }
    // fn visit_MatchMappingElement(&mut self, node: &MatchMappingElement) { walk_MatchMappingElement(self, node); }
    // fn visit_MatchOr(&mut self, node: &MatchOr) { walk_MatchOr(self, node); }
    // fn visit_MatchOrElement(&mut self, node: &MatchOrElement) { walk_MatchOrElement(self, node); }
    // fn visit_MatchPattern(&mut self, node: &MatchPattern) { walk_MatchPattern(self, node); }
    // fn visit_MatchSequence(&mut self, node: &MatchSequence) { walk_MatchSequence(self, node); }
    // fn visit_MatchSequenceElement(&mut self, node: &MatchSequenceElement) { walk_MatchSequenceElement(self, node); }
    // fn visit_MatchSingleton(&mut self, node: &MatchSingleton) { walk_MatchSingleton(self, node); }
    // fn visit_MatchStar(&mut self, node: &MatchStar) { walk_MatchStar(self, node); }
    // fn visit_MatchTuple(&mut self, node: &MatchTuple) { walk_MatchTuple(self, node); }
    // fn visit_MatchValue(&mut self, node: &MatchValue) { walk_MatchValue(self, node); }

    fn visit_Name(&mut self, node: &Name) {
        walk_Name(self, node);
    }
    fn visit_NameItem(&mut self, node: &NameItem) {
        walk_NameItem(self, node);
    }
    fn visit_NamedExpr(&mut self, node: &NamedExpr) {
        walk_NamedExpr(self, node);
    }
    fn visit_Nonlocal(&mut self, node: &Nonlocal) {
        walk_Nonlocal(self, node);
    }
    fn visit_OrElse(&mut self, node: &OrElse) {
        walk_OrElse(self, node);
    }
    fn visit_Param(&mut self, node: &Param) {
        walk_Param(self, node);
    }
    fn visit_ParamStar(&mut self, node: &ParamStar) {
        walk_ParamStar(self, node);
    }
    fn visit_Parameters(&mut self, node: &Parameters) {
        walk_Parameters(self, node);
    }
    fn visit_Pass(&mut self, node: &Pass) {
        walk_Pass(self, node);
    }
    fn visit_Raise(&mut self, node: &Raise) {
        walk_Raise(self, node);
    }
    fn visit_Return(&mut self, node: &Return) {
        walk_Return(self, node);
    }
    fn visit_Set(&mut self, node: &Set) {
        walk_Set(self, node);
    }
    fn visit_SetComp(&mut self, node: &SetComp) {
        walk_SetComp(self, node);
    }
    fn visit_SimpleStatementSuite(&mut self, node: &SimpleStatementSuite) {
        walk_SimpleStatementSuite(self, node);
    }
    fn visit_SimpleString(&mut self, node: &SimpleString) {
        walk_SimpleString(self, node);
    }
    fn visit_Slice(&mut self, node: &Slice) {
        walk_Slice(self, node);
    }
    fn visit_StarredDictElement(&mut self, node: &StarredDictElement) {
        walk_StarredDictElement(self, node);
    }
    fn visit_StarredElement(&mut self, node: &StarredElement) {
        walk_StarredElement(self, node);
    }
    fn visit_Subscript(&mut self, node: &Subscript) {
        walk_Subscript(self, node);
    }
    fn visit_SubscriptElement(&mut self, node: &SubscriptElement) {
        walk_SubscriptElement(self, node);
    }
    fn visit_Try(&mut self, node: &Try) {
        walk_Try(self, node);
    }
    fn visit_TryStar(&mut self, node: &TryStar) {
        walk_TryStar(self, node);
    }
    fn visit_Tuple(&mut self, node: &Tuple) {
        walk_Tuple(self, node);
    }
    fn visit_UnaryOp(&mut self, node: &UnaryOp) {
        walk_UnaryOp(self, node);
    }
    fn visit_UnaryOperation(&mut self, node: &UnaryOperation) {
        walk_UnaryOperation(self, node);
    }
    fn visit_While(&mut self, node: &While) {
        walk_While(self, node);
    }
    fn visit_With(&mut self, node: &With) {
        walk_With(self, node);
    }
    fn visit_WithItem(&mut self, node: &WithItem) {
        walk_WithItem(self, node);
    }
    fn visit_Yield(&mut self, node: &Yield) {
        walk_Yield(self, node);
    }
    fn visit_YieldValue(&mut self, node: &YieldValue) {
        walk_YieldValue(self, node);
    }
}

pub fn walk_Module<'a, V: CSTVisitor + ?Sized>(visitor: &mut V, node: &'a Module<'a>) -> Module<'a>
where
    'a: 'a,
{
    let mut body: Vec<Statement> = vec![];
    for node in &node.body {
        if let Some(node) = visitor.visit_Statement(node) {
            body.push(node)
        }
    }

    let mut transformed: Module<'a> = node.clone();
    transformed.body = body;
    transformed
}

pub fn walk_Statement<'a, V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &'a Statement<'a>,
) -> Option<Statement<'a>> {
    match node {
        Statement::Simple(node) => visitor
            .visit_SimpleStatementLine(node)
            .map(Statement::Simple),
        Statement::Compound(node) => visitor
            .visit_CompoundStatement(node)
            .map(Statement::Compound),
    }
}

pub fn walk_SimpleStatementLine<'a, V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &'a SimpleStatementLine<'a>,
) -> Option<SimpleStatementLine<'a>> {
    for node in &node.body {
        visitor.visit_SmallStatement(node);
    }
    Some(node.clone())
}

pub fn walk_CompoundStatement<'a, V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &'a CompoundStatement<'a>,
) -> Option<CompoundStatement<'a>> {
    match node {
        CompoundStatement::If(node) => {
            visitor.visit_If(node);
            return None;
        }
        CompoundStatement::FunctionDef(node) => visitor.visit_FunctionDef(node),
        CompoundStatement::For(node) => visitor.visit_For(node),
        CompoundStatement::While(node) => visitor.visit_While(node),
        CompoundStatement::ClassDef(node) => {
            return Some(CompoundStatement::ClassDef(visitor.visit_ClassDef(node)))
        }
        CompoundStatement::Try(node) => visitor.visit_Try(node),
        CompoundStatement::TryStar(node) => visitor.visit_TryStar(node),
        CompoundStatement::With(node) => visitor.visit_With(node),
        CompoundStatement::Match(node) => visitor.visit_Match(node),
    }

    Some(node.clone())
}

pub fn walk_SmallStatement<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &SmallStatement) {
    match node {
        SmallStatement::Pass(node) => visitor.visit_Pass(node),
        SmallStatement::Break(node) => visitor.visit_Break(node),
        SmallStatement::Continue(node) => visitor.visit_Continue(node),
        SmallStatement::Return(node) => visitor.visit_Return(node),
        SmallStatement::Expr(node) => visitor.visit_Expr(node),
        SmallStatement::Assert(node) => visitor.visit_Assert(node),
        SmallStatement::Import(node) => visitor.visit_Import(node),
        SmallStatement::ImportFrom(node) => visitor.visit_ImportFrom(node),
        SmallStatement::Assign(node) => visitor.visit_Assign(node),
        SmallStatement::AnnAssign(node) => visitor.visit_AnnAssign(node),
        SmallStatement::Raise(node) => visitor.visit_Raise(node),
        SmallStatement::Global(node) => visitor.visit_Global(node),
        SmallStatement::Nonlocal(node) => visitor.visit_Nonlocal(node),
        SmallStatement::AugAssign(node) => visitor.visit_AugAssign(node),
        SmallStatement::Del(node) => visitor.visit_Del(node),
    }
}

pub fn walk_Expression<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Expression) {
    match node {
        Expression::Name(node) => visitor.visit_Name(node),
        Expression::Ellipsis(node) => visitor.visit_Ellipsis(node),
        Expression::Integer(node) => visitor.visit_Integer(node),
        Expression::Float(node) => visitor.visit_Float(node),
        Expression::Imaginary(node) => visitor.visit_Imaginary(node),
        Expression::Comparison(node) => visitor.visit_Comparison(node),
        Expression::UnaryOperation(node) => visitor.visit_UnaryOperation(node),
        Expression::BinaryOperation(node) => visitor.visit_BinaryOperation(node),
        Expression::BooleanOperation(node) => visitor.visit_BooleanOperation(node),
        Expression::Attribute(node) => visitor.visit_Attribute(node),
        Expression::Tuple(node) => visitor.visit_Tuple(node),
        Expression::Call(node) => visitor.visit_Call(node),
        Expression::GeneratorExp(node) => visitor.visit_GeneratorExp(node),
        Expression::ListComp(node) => visitor.visit_ListComp(node),
        Expression::SetComp(node) => visitor.visit_SetComp(node),
        Expression::DictComp(node) => visitor.visit_DictComp(node),
        Expression::List(node) => visitor.visit_List(node),
        Expression::Set(node) => visitor.visit_Set(node),
        Expression::Dict(node) => visitor.visit_Dict(node),
        Expression::Subscript(node) => visitor.visit_Subscript(node),
        Expression::StarredElement(node) => visitor.visit_StarredElement(node),
        Expression::IfExp(node) => visitor.visit_IfExp(node),
        Expression::Lambda(node) => visitor.visit_Lambda(node),
        Expression::Yield(node) => visitor.visit_Yield(node),
        Expression::Await(node) => visitor.visit_Await(node),
        Expression::SimpleString(node) => visitor.visit_SimpleString(node),
        Expression::ConcatenatedString(node) => visitor.visit_ConcatenatedString(node),
        Expression::FormattedString(node) => visitor.visit_FormattedString(node),
        Expression::NamedExpr(node) => visitor.visit_NamedExpr(node),
    }
}
pub fn walk_AssignEqual<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &AssignEqual) {
    // Nothing to do.
}
pub fn walk_AssignTarget<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &AssignTarget) {
    visitor.visit_AssignTargetExpression(&node.target);
}
pub fn walk_AssignTargetExpression<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &AssignTargetExpression,
) {
    match node {
        AssignTargetExpression::Name(node) => visitor.visit_Name(node),
        AssignTargetExpression::Attribute(node) => visitor.visit_Attribute(node),
        AssignTargetExpression::StarredElement(node) => visitor.visit_StarredElement(node),
        AssignTargetExpression::Tuple(node) => visitor.visit_Tuple(node),
        AssignTargetExpression::List(node) => visitor.visit_List(node),
        AssignTargetExpression::Subscript(node) => visitor.visit_Subscript(node),
    }
}
pub fn walk_AnnAssign<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &AnnAssign) {
    visitor.visit_AssignTargetExpression(&node.target);
    if let Some(node) = &node.value {
        visitor.visit_Expression(node)
    }
}
pub fn walk_Annotation<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Annotation) {
    visitor.visit_Expression(&node.annotation);
}
pub fn walk_Arg<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Arg) {
    visitor.visit_Expression(&node.value);
    if let Some(node) = &node.keyword {
        visitor.visit_Name(node)
    }
    if let Some(node) = &node.equal {
        visitor.visit_AssignEqual(node)
    }
}
pub fn walk_AsName<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &AsName) {
    visitor.visit_AssignTargetExpression(&node.name)
}
pub fn walk_Assert<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Assert) {
    visitor.visit_Expression(&node.test);
    if let Some(expression) = &node.msg {
        visitor.visit_Expression(expression);
    }
}
pub fn walk_Assign<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Assign) {
    for target in &node.targets {
        visitor.visit_AssignTarget(target)
    }
    visitor.visit_Expression(&node.value)
}
pub fn walk_Asynchronous<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Asynchronous) {
    // Nothing to do.
}
pub fn walk_Attribute<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Attribute) {
    visitor.visit_Expression(&node.value)
}
pub fn walk_AugAssign<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &AugAssign) {
    visitor.visit_AssignTargetExpression(&node.target);
    visitor.visit_Expression(&node.value);
}
pub fn walk_Await<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Await) {
    visitor.visit_Expression(&node.expression);
}
pub fn walk_BinaryOperation<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &BinaryOperation) {
    visitor.visit_Expression(&node.left);
    visitor.visit_BinaryOp(&node.operator);
    visitor.visit_Expression(&node.right);
}
pub fn walk_BinaryOp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &BinaryOp) {
    // Nothing to do.
}
pub fn walk_BooleanOperation<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &BooleanOperation) {
    visitor.visit_Expression(&node.left);
    visitor.visit_BooleanOp(&node.operator);
    visitor.visit_Expression(&node.right);
}
pub fn walk_BooleanOp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &BooleanOp) {
    // Nothing to do.
}
pub fn walk_Break<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Break) {
    // Nothing to do.
}
pub fn walk_Call<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Call) {
    for node in &node.args {
        visitor.visit_Arg(node)
    }
    visitor.visit_Expression(&node.func)
}
pub fn walk_ClassDef<'a, V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &'a ClassDef<'a>,
) -> ClassDef<'a> {
    visitor.visit_Name(&node.name);
    for node in &node.bases {
        visitor.visit_Arg(node);
    }
    for node in &node.keywords {
        visitor.visit_Arg(node);
    }
    for node in &node.decorators {
        visitor.visit_Decorator(node);
    }
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }

    node.clone()
}
pub fn walk_CompFor<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &CompFor) {
    if let Some(node) = &node.asynchronous {
        visitor.visit_Asynchronous(node);
    }
    visitor.visit_AssignTargetExpression(&node.target);
    visitor.visit_Expression(&node.iter);
    for node in &node.ifs {
        visitor.visit_CompIf(node);
    }
    if let Some(node) = &node.inner_for_in {
        visitor.visit_CompFor(node);
    }
}
pub fn walk_CompIf<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &CompIf) {
    visitor.visit_Expression(&node.test)
}
pub fn walk_Comparison<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Comparison) {
    visitor.visit_Expression(&node.left);
    for node in &node.comparisons {
        visitor.visit_ComparisonTarget(node);
    }
}
pub fn walk_ComparisonTarget<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ComparisonTarget) {
    visitor.visit_CompOp(&node.operator);
    visitor.visit_Expression(&node.comparator);
}
pub fn walk_CompOp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &CompOp) {
    // Nothing to do.
}
pub fn walk_ConcatenatedString<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ConcatenatedString) {
    // Nothing to do.
}
pub fn walk_Continue<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Continue) {
    // Nothing to do.
}
pub fn walk_Decorator<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Decorator) {
    visitor.visit_Expression(&node.decorator)
}
pub fn walk_Del<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Del) {
    visitor.visit_DelTargetExpression(&node.target)
}
pub fn walk_DelTargetExpression<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &DelTargetExpression,
) {
    match node {
        DelTargetExpression::Name(node) => visitor.visit_Name(node),
        DelTargetExpression::Attribute(node) => visitor.visit_Attribute(node),
        DelTargetExpression::Tuple(node) => visitor.visit_Tuple(node),
        DelTargetExpression::List(node) => visitor.visit_List(node),
        DelTargetExpression::Subscript(node) => visitor.visit_Subscript(node),
    }
}
pub fn walk_Dict<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Dict) {
    for node in &node.elements {
        visitor.visit_DictElement(node)
    }
}
pub fn walk_DictComp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &DictComp) {
    visitor.visit_Expression(&node.key);
    visitor.visit_Expression(&node.value);
    visitor.visit_CompFor(&node.for_in);
}
pub fn walk_DictElement<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &DictElement) {
    match node {
        DictElement::Simple { key, value, .. } => {
            visitor.visit_Expression(key);
            visitor.visit_Expression(value);
        }
        DictElement::Starred(node) => visitor.visit_StarredDictElement(node),
    }
}
pub fn walk_Element<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Element) {
    match node {
        Element::Simple { value: node, .. } => visitor.visit_Expression(node),
        Element::Starred(node) => visitor.visit_StarredElement(node),
    }
}
pub fn walk_Ellipsis<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Ellipsis) {
    // Nothing to do.
}
pub fn walk_Else<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Else) {
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
}
pub fn walk_ExceptHandler<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ExceptHandler) {
    if let Some(node) = &node.r#type {
        visitor.visit_Expression(node);
    }
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    if let Some(node) = &node.name {
        visitor.visit_AsName(node)
    }
}
pub fn walk_ExceptStarHandler<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ExceptStarHandler) {
    visitor.visit_Expression(&node.r#type);
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    if let Some(node) = &node.name {
        visitor.visit_AsName(node)
    }
}
pub fn walk_Expr<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Expr) {
    visitor.visit_Expression(&node.value)
}
pub fn walk_Finally<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Finally) {
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
}
pub fn walk_Float<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Float) {
    // Nothing to do.
}
pub fn walk_For<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &For) {
    visitor.visit_AssignTargetExpression(&node.target);
    visitor.visit_Expression(&node.iter);
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    if let Some(node) = &node.orelse {
        visitor.visit_Else(node);
    }
    if let Some(node) = &node.asynchronous {
        visitor.visit_Asynchronous(node);
    }
}
pub fn walk_FormattedString<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &FormattedString) {
    for node in &node.parts {
        match node {
            FormattedStringContent::Text(node) => visitor.visit_FormattedStringText(node),
            FormattedStringContent::Expression(node) => {
                visitor.visit_FormattedStringExpression(node)
            }
        }
    }
}
pub fn walk_FormattedStringExpression<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &FormattedStringExpression,
) {
    visitor.visit_Expression(&node.expression);
}
pub fn walk_FormattedStringText<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &FormattedStringText,
) {
    // Nothing to do.
}
pub fn walk_From<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &From) {
    visitor.visit_Expression(&node.item)
}
pub fn walk_FunctionDef<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &FunctionDef) {
    visitor.visit_Name(&node.name);
    visitor.visit_Parameters(&node.params);
    for node in &node.decorators {
        visitor.visit_Decorator(node);
    }
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    if let Some(node) = &node.returns {
        visitor.visit_Annotation(node);
    }
    if let Some(node) = &node.asynchronous {
        visitor.visit_Asynchronous(node);
    }
}
pub fn walk_GeneratorExp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &GeneratorExp) {
    visitor.visit_Expression(&node.elt);
    visitor.visit_CompFor(&node.for_in);
}
pub fn walk_Global<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Global) {
    for node in &node.names {
        visitor.visit_NameItem(node)
    }
}
pub fn walk_If<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &If) {
    visitor.visit_Expression(&node.test);
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    if let Some(node) = &node.orelse {
        visitor.visit_OrElse(node);
    }
}
pub fn walk_IfExp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &IfExp) {
    visitor.visit_Expression(&node.test);
    visitor.visit_Expression(&node.body);
    visitor.visit_Expression(&node.orelse);
}
pub fn walk_Imaginary<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Imaginary) {
    // Nothing to do.
}
pub fn walk_Import<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Import) {
    for node in &node.names {
        visitor.visit_ImportAlias(node)
    }
}
pub fn walk_ImportAlias<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ImportAlias) {
    match &node.name {
        NameOrAttribute::N(node) => visitor.visit_Name(node),
        NameOrAttribute::A(node) => visitor.visit_Attribute(node),
    }
    if let Some(node) = &node.asname {
        visitor.visit_AsName(node)
    }
}
pub fn walk_ImportFrom<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ImportFrom) {
    match &node.names {
        ImportNames::Star(node) => visitor.visit_ImportStar(node),
        ImportNames::Aliases(node) => {
            for node in node {
                visitor.visit_ImportAlias(node)
            }
        }
    }
}
pub fn walk_ImportStar<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ImportStar) {
    // Nothing to do.
}
pub fn walk_IndentedBlock<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &IndentedBlock) {
    for node in &node.body {
        visitor.visit_Statement(node);
    }
}
pub fn walk_Index<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Index) {
    visitor.visit_Expression(&node.value);
}
pub fn walk_Integer<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Integer) {
    // Nothing to do.
}
pub fn walk_Lambda<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Lambda) {
    visitor.visit_Parameters(&node.params);
    visitor.visit_Expression(&node.body);
}
pub fn walk_List<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &List) {
    for node in &node.elements {
        visitor.visit_Element(node)
    }
}
pub fn walk_ListComp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ListComp) {
    visitor.visit_Expression(&node.elt);
    visitor.visit_CompFor(&node.for_in);
}
pub fn walk_Match<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Match) {
    visitor.visit_Expression(&node.subject);
    // TODO
    // for node in &node.cases {
    //     visitor.visit_MatchCase(node);
    // }
}
pub fn walk_Name<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Name) {
    // Nothing to do.
}
pub fn walk_NameItem<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &NameItem) {
    visitor.visit_Name(&node.name);
}
pub fn walk_NamedExpr<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &NamedExpr) {}
pub fn walk_Nonlocal<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Nonlocal) {}
pub fn walk_OrElse<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &OrElse) {
    match node {
        OrElse::Elif(node) => {
            visitor.visit_If(node);
        }
        OrElse::Else(node) => visitor.visit_Else(node),
    }
}
pub fn walk_Param<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Param) {
    visitor.visit_Name(&node.name);
    if let Some(node) = &node.annotation {
        visitor.visit_Annotation(node);
    }
    if let Some(node) = &node.equal {
        visitor.visit_AssignEqual(node);
    }
    if let Some(node) = &node.default {
        visitor.visit_Expression(node);
    }
}
pub fn walk_ParamStar<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &ParamStar) {
    // Nothing to do.
}
pub fn walk_Parameters<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Parameters) {
    for node in &node.posonly_params {
        visitor.visit_Param(node);
    }
    for node in &node.params {
        visitor.visit_Param(node);
    }
    if let Some(node) = &node.star_kwarg {
        visitor.visit_Param(node);
    }
    for node in &node.kwonly_params {
        visitor.visit_Param(node);
    }
    if let Some(node) = &node.star_arg {
        match node {
            StarArg::Star(node) => visitor.visit_ParamStar(node),
            StarArg::Param(node) => visitor.visit_Param(node),
        }
    }
}
pub fn walk_Pass<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Pass) {
    // Nothing to do.
}
pub fn walk_Raise<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Raise) {}
pub fn walk_Return<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Return) {
    if let Some(expression) = &node.value {
        visitor.visit_Expression(expression);
    }
}
pub fn walk_Set<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Set) {
    for node in &node.elements {
        visitor.visit_Element(node)
    }
}
pub fn walk_SetComp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &SetComp) {
    visitor.visit_Expression(&node.elt);
    visitor.visit_CompFor(&node.for_in);
}
pub fn walk_SimpleString<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &SimpleString) {
    // Nothing to do.
}
pub fn walk_SimpleStatementSuite<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    node: &SimpleStatementSuite,
) {
    for node in &node.body {
        visitor.visit_SmallStatement(node)
    }
}
pub fn walk_Slice<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Slice) {
    if let Some(node) = &node.lower {
        visitor.visit_Expression(node)
    }
    if let Some(node) = &node.upper {
        visitor.visit_Expression(node)
    }
    if let Some(node) = &node.step {
        visitor.visit_Expression(node)
    }
}
pub fn walk_StarredDictElement<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &StarredDictElement) {
    visitor.visit_Expression(&node.value)
}
pub fn walk_StarredElement<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &StarredElement) {
    visitor.visit_Expression(&node.value)
}
pub fn walk_Subscript<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Subscript) {
    visitor.visit_Expression(&node.value);
    for node in &node.slice {
        visitor.visit_SubscriptElement(node)
    }
}
pub fn walk_SubscriptElement<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &SubscriptElement) {
    match &node.slice {
        BaseSlice::Index(node) => visitor.visit_Index(node),
        BaseSlice::Slice(node) => visitor.visit_Slice(node),
    }
}
pub fn walk_Try<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Try) {
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    for node in &node.handlers {
        visitor.visit_ExceptHandler(node)
    }
    if let Some(node) = &node.orelse {
        visitor.visit_Else(node)
    }
    if let Some(node) = &node.finalbody {
        visitor.visit_Finally(node)
    }
}
pub fn walk_TryStar<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &TryStar) {
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    for node in &node.handlers {
        visitor.visit_ExceptStarHandler(node)
    }
    if let Some(node) = &node.orelse {
        visitor.visit_Else(node)
    }
    if let Some(node) = &node.finalbody {
        visitor.visit_Finally(node)
    }
}
pub fn walk_Tuple<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Tuple) {
    for node in &node.elements {
        visitor.visit_Element(node)
    }
}
pub fn walk_UnaryOp<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &UnaryOp) {
    // Nothing to do.
}
pub fn walk_UnaryOperation<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &UnaryOperation) {
    visitor.visit_UnaryOp(&node.operator);
    visitor.visit_Expression(&node.expression);
}
pub fn walk_While<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &While) {
    visitor.visit_Expression(&node.test);
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
    if let Some(node) = &node.orelse {
        visitor.visit_Else(node)
    }
}
pub fn walk_With<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &With) {
    if let Some(node) = &node.asynchronous {
        visitor.visit_Asynchronous(node);
    }
    for node in &node.items {
        visitor.visit_WithItem(node)
    }
    match &node.body {
        Suite::IndentedBlock(node) => visitor.visit_IndentedBlock(node),
        Suite::SimpleStatementSuite(node) => visitor.visit_SimpleStatementSuite(node),
    }
}
pub fn walk_WithItem<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &WithItem) {
    visitor.visit_Expression(&node.item);
    if let Some(node) = &node.asname {
        visitor.visit_AsName(node)
    }
}
pub fn walk_Yield<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &Yield) {
    if let Some(node) = &node.value {
        visitor.visit_YieldValue(node);
    }
}
pub fn walk_YieldValue<V: CSTVisitor + ?Sized>(visitor: &mut V, node: &YieldValue) {
    match node {
        YieldValue::Expression(node) => visitor.visit_Expression(node),
        YieldValue::From(node) => visitor.visit_From(node),
    }
}
