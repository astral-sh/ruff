use libcst_native::{
    AnnAssign, Annotation, Arg, AsName, Assert, Assign, AssignEqual, AssignTarget, Asynchronous,
    Attribute, AugAssign, AugOp, Await, BinaryOperation, BitAnd, BitAndAssign, BitInvert,
    BitOrAssign, BitXor, BitXorAssign, BooleanOperation, Break, Call, ClassDef, Colon, Comma,
    Comment, CompFor, CompIf, Comparison, ComparisonTarget, CompoundStatement, ConcatenatedString,
    Continue, Decorator, Del, Dict, DictComp, DictElement, Divide, DivideAssign, Dot, Element,
    Ellipsis, Else, EmptyLine, Equal, ExceptHandler, ExceptStarHandler, Expr, Expression, Finally,
    Float, FloorDivide, FloorDivideAssign, For, FormattedString, FormattedStringExpression,
    FormattedStringText, From, FunctionDef, GeneratorExp, Global, GreaterThan, GreaterThanEqual,
    If, IfExp, Imaginary, Import, ImportAlias, ImportFrom, ImportStar, In, IndentedBlock, Index,
    Integer, Is, IsNot, Lambda, LeftCurlyBrace, LeftParen, LeftShift, LeftShiftAssign,
    LeftSquareBracket, LessThan, LessThanEqual, List, ListComp, Match, MatchAs, MatchCase,
    MatchClass, MatchKeywordElement, MatchList, MatchMapping, MatchMappingElement, MatchOr,
    MatchOrElement, MatchPattern, MatchSequence, MatchSequenceElement, MatchSingleton, MatchStar,
    MatchTuple, MatchValue, MatrixMultiply, MatrixMultiplyAssign, Minus, Module, Modulo,
    ModuloAssign, Multiply, MultiplyAssign, Name, NameItem, NamedExpr, Newline, Nonlocal, Not,
    NotEqual, NotIn, Or, Param, ParamSlash, ParamStar, Parameters, ParenthesizedWhitespace, Pass,
    Plus, Power, PowerAssign, Raise, Return, RightCurlyBrace, RightParen, RightShift,
    RightShiftAssign, RightSquareBracket, Semicolon, Set, SetComp, SimpleStatementLine,
    SimpleStatementSuite, SimpleString, SimpleWhitespace, Slice, SmallStatement,
    StarredDictElement, StarredElement, Statement, Subscript, SubscriptElement, Subtract,
    SubtractAssign, TrailingWhitespace, Try, TryStar, Tuple, UnaryOperation, While, With, WithItem,
    Yield,
};

pub trait CSTVisitor {
    // fn visit_statement(&mut self, statement: &Statement) {
    //     walk_statement(self, statement);
    // }
    // fn visit_simple_statement_line(&mut self, simple_statement_line: &SimpleStatementLine) {
    //     walk_simple_statement_line(self, simple_statement_line);
    // }
    // fn visit_compound_statement(&mut self, compound_statement: &CompoundStatement) {
    //     walk_compound_statement(self, compound_statement);
    // }
    // fn visit_small_statement(&mut self, small_statement: &SmallStatement) {
    //     walk_small_statement(self, small_statement);
    // }
    // fn visit_expression(&mut self, expression: &Expression) {
    //     walk_expression(self, expression);
    // }
    // fn visit_Add(&mut self, node: &Add) {}
    // fn visit_AddAssign(&mut self, node: &AddAssign) {}
    // fn visit_And(&mut self, node: &And) {}
    fn visit_AnnAssign(&mut self, node: &AnnAssign) {}
    fn visit_Annotation(&mut self, node: &Annotation) {}
    fn visit_Arg(&mut self, node: &Arg) {}
    fn visit_AsName(&mut self, node: &AsName) {}
    fn visit_Assert(&mut self, node: &Assert) {}
    fn visit_Assign(&mut self, node: &Assign) {}
    fn visit_AssignEqual(&mut self, node: &AssignEqual) {}
    fn visit_AssignTarget(&mut self, node: &AssignTarget) {}
    fn visit_Asynchronous(&mut self, node: &Asynchronous) {}
    fn visit_Attribute(&mut self, node: &Attribute) {}
    fn visit_AugAssign(&mut self, node: &AugAssign) {}
    fn visit_AugOp(&mut self, node: &AugOp) {}
    fn visit_Await(&mut self, node: &Await) {}
    fn visit_BinaryOperation(&mut self, node: &BinaryOperation) {}
    // fn visit_BitAnd(&mut self, node: &BitAnd) {}
    // fn visit_BitAndAssign(&mut self, node: &BitAndAssign) {}
    // fn visit_BitInvert(&mut self, node: &BitInvert) {}
    // fn visit_BitOr(&mut self, node: &BitOr) {}
    // fn visit_BitOrAssign(&mut self, node: &BitOrAssign) {}
    // fn visit_BitXor(&mut self, node: &BitXor) {}
    // fn visit_BitXorAssign(&mut self, node: &BitXorAssign) {}
    fn visit_BooleanOperation(&mut self, node: &BooleanOperation) {}
    fn visit_Break(&mut self, node: &Break) {}
    fn visit_Call(&mut self, node: &Call) {}
    fn visit_ClassDef(&mut self, node: &ClassDef) {}
    fn visit_Colon(&mut self, node: &Colon) {}
    fn visit_Comma(&mut self, node: &Comma) {}
    fn visit_Comment(&mut self, node: &Comment) {}
    fn visit_CompFor(&mut self, node: &CompFor) {}
    fn visit_CompIf(&mut self, node: &CompIf) {}
    fn visit_Comparison(&mut self, node: &Comparison) {}
    fn visit_ComparisonTarget(&mut self, node: &ComparisonTarget) {}
    fn visit_ConcatenatedString(&mut self, node: &ConcatenatedString) {}
    fn visit_Continue(&mut self, node: &Continue) {}
    fn visit_Decorator(&mut self, node: &Decorator) {}
    fn visit_Del(&mut self, node: &Del) {}
    fn visit_Dict(&mut self, node: &Dict) {}
    fn visit_DictComp(&mut self, node: &DictComp) {}
    fn visit_DictElement(&mut self, node: &DictElement) {}
    fn visit_Divide(&mut self, node: &Divide) {}
    fn visit_DivideAssign(&mut self, node: &DivideAssign) {}
    fn visit_Dot(&mut self, node: &Dot) {}
    fn visit_Element(&mut self, node: &Element) {}
    fn visit_Ellipsis(&mut self, node: &Ellipsis) {}
    fn visit_Else(&mut self, node: &Else) {}
    fn visit_EmptyLine(&mut self, node: &EmptyLine) {}
    fn visit_Equal(&mut self, node: &Equal) {}
    fn visit_ExceptHandler(&mut self, node: &ExceptHandler) {}
    fn visit_ExceptStarHandler(&mut self, node: &ExceptStarHandler) {}
    fn visit_Expr(&mut self, node: &Expr) {}
    fn visit_Finally(&mut self, node: &Finally) {}
    fn visit_Float(&mut self, node: &Float) {}
    fn visit_FloorDivide(&mut self, node: &FloorDivide) {}
    fn visit_FloorDivideAssign(&mut self, node: &FloorDivideAssign) {}
    fn visit_For(&mut self, node: &For) {}
    fn visit_FormattedString(&mut self, node: &FormattedString) {}

    fn visit_FormattedStringExpression(&mut self, node: &FormattedStringExpression) {}
    fn visit_FormattedStringText(&mut self, node: &FormattedStringText) {}
    fn visit_From(&mut self, node: &From) {}
    fn visit_FunctionDef(&mut self, node: &FunctionDef) {}
    fn visit_GeneratorExp(&mut self, node: &GeneratorExp) {}
    fn visit_Global(&mut self, node: &Global) {}
    fn visit_GreaterThan(&mut self, node: &GreaterThan) {}
    fn visit_GreaterThanEqual(&mut self, node: &GreaterThanEqual) {}
    fn visit_If(&mut self, node: &If) {}
    fn visit_IfExp(&mut self, node: &IfExp) {}
    fn visit_Imaginary(&mut self, node: &Imaginary) {}
    fn visit_Import(&mut self, node: &Import) {}
    fn visit_ImportAlias(&mut self, node: &ImportAlias) {}
    fn visit_ImportFrom(&mut self, node: &ImportFrom) {}
    fn visit_ImportStar(&mut self, node: &ImportStar) {}
    fn visit_In(&mut self, node: &In) {}
    fn visit_IndentedBlock(&mut self, node: &IndentedBlock) {}
    fn visit_Index(&mut self, node: &Index) {}
    fn visit_Integer(&mut self, node: &Integer) {}
    fn visit_Is(&mut self, node: &Is) {}
    fn visit_IsNot(&mut self, node: &IsNot) {}
    fn visit_Lambda(&mut self, node: &Lambda) {}
    fn visit_LeftCurlyBrace(&mut self, node: &LeftCurlyBrace) {}
    fn visit_LeftParen(&mut self, node: &LeftParen) {}
    fn visit_LeftShift(&mut self, node: &LeftShift) {}
    fn visit_LeftShiftAssign(&mut self, node: &LeftShiftAssign) {}
    fn visit_LeftSquareBracket(&mut self, node: &LeftSquareBracket) {}
    fn visit_LessThan(&mut self, node: &LessThan) {}
    fn visit_LessThanEqual(&mut self, node: &LessThanEqual) {}
    fn visit_List(&mut self, node: &List) {}
    fn visit_ListComp(&mut self, node: &ListComp) {}
    fn visit_Match(&mut self, node: &Match) {}
    fn visit_MatchAs(&mut self, node: &MatchAs) {}
    fn visit_MatchCase(&mut self, node: &MatchCase) {}
    fn visit_MatchClass(&mut self, node: &MatchClass) {}
    fn visit_MatchKeywordElement(&mut self, node: &MatchKeywordElement) {}
    fn visit_MatchList(&mut self, node: &MatchList) {}
    fn visit_MatchMapping(&mut self, node: &MatchMapping) {}
    fn visit_MatchMappingElement(&mut self, node: &MatchMappingElement) {}
    fn visit_MatchOr(&mut self, node: &MatchOr) {}
    fn visit_MatchOrElement(&mut self, node: &MatchOrElement) {}
    fn visit_MatchPattern(&mut self, node: &MatchPattern) {}
    fn visit_MatchSequence(&mut self, node: &MatchSequence) {}

    fn visit_MatchSequenceElement(&mut self, node: &MatchSequenceElement) {}
    fn visit_MatchSingleton(&mut self, node: &MatchSingleton) {}
    fn visit_MatchStar(&mut self, node: &MatchStar) {}
    fn visit_MatchTuple(&mut self, node: &MatchTuple) {}
    fn visit_MatchValue(&mut self, node: &MatchValue) {}
    fn visit_MatrixMultiply(&mut self, node: &MatrixMultiply) {}

    fn visit_MatrixMultiplyAssign(&mut self, node: &MatrixMultiplyAssign) {}
    fn visit_Minus(&mut self, node: &Minus) {}
    fn visit_Module(&mut self, node: &Module) {}
    fn visit_Modulo(&mut self, node: &Modulo) {}
    fn visit_ModuloAssign(&mut self, node: &ModuloAssign) {}
    fn visit_Multiply(&mut self, node: &Multiply) {}
    fn visit_MultiplyAssign(&mut self, node: &MultiplyAssign) {}
    fn visit_Name(&mut self, node: &Name) {}
    fn visit_NameItem(&mut self, node: &NameItem) {}
    fn visit_NamedExpr(&mut self, node: &NamedExpr) {}
    fn visit_Newline(&mut self, node: &Newline) {}
    fn visit_Nonlocal(&mut self, node: &Nonlocal) {}
    fn visit_Not(&mut self, node: &Not) {}
    fn visit_NotEqual(&mut self, node: &NotEqual) {}
    fn visit_NotIn(&mut self, node: &NotIn) {}
    fn visit_Or(&mut self, node: &Or) {}
    fn visit_Param(&mut self, node: &Param) {}
    fn visit_ParamSlash(&mut self, node: &ParamSlash) {}
    fn visit_ParamStar(&mut self, node: &ParamStar) {}
    fn visit_Parameters(&mut self, node: &Parameters) {}

    fn visit_ParenthesizedWhitespace(&mut self, node: &ParenthesizedWhitespace) {}
    fn visit_Pass(&mut self, node: &Pass) {}
    fn visit_Plus(&mut self, node: &Plus) {}
    fn visit_Power(&mut self, node: &Power) {}
    fn visit_PowerAssign(&mut self, node: &PowerAssign) {}
    fn visit_Raise(&mut self, node: &Raise) {}
    fn visit_Return(&mut self, node: &Return) {}
    fn visit_RightCurlyBrace(&mut self, node: &RightCurlyBrace) {}
    fn visit_RightParen(&mut self, node: &RightParen) {}
    fn visit_RightShift(&mut self, node: &RightShift) {}
    fn visit_RightShiftAssign(&mut self, node: &RightShiftAssign) {}
    fn visit_RightSquareBracket(&mut self, node: &RightSquareBracket) {}
    fn visit_Semicolon(&mut self, node: &Semicolon) {}
    fn visit_Set(&mut self, node: &Set) {}
    fn visit_SetComp(&mut self, node: &SetComp) {}
    fn visit_SimpleStatementLine(&mut self, node: &SimpleStatementLine) {}

    fn visit_SimpleStatementSuite(&mut self, node: &SimpleStatementSuite) {}
    fn visit_SimpleString(&mut self, node: &SimpleString) {}
    fn visit_SimpleWhitespace(&mut self, node: &SimpleWhitespace) {}
    fn visit_Slice(&mut self, node: &Slice) {}
    fn visit_StarredDictElement(&mut self, node: &StarredDictElement) {}
    fn visit_StarredElement(&mut self, node: &StarredElement) {}
    fn visit_Subscript(&mut self, node: &Subscript) {}
    fn visit_SubscriptElement(&mut self, node: &SubscriptElement) {}
    fn visit_Subtract(&mut self, node: &Subtract) {}
    fn visit_SubtractAssign(&mut self, node: &SubtractAssign) {}
    fn visit_TrailingWhitespace(&mut self, node: &TrailingWhitespace) {}
    fn visit_Try(&mut self, node: &Try) {}
    fn visit_TryStar(&mut self, node: &TryStar) {}
    fn visit_Tuple(&mut self, node: &Tuple) {}
    fn visit_UnaryOperation(&mut self, node: &UnaryOperation) {}
    fn visit_While(&mut self, node: &While) {}
    fn visit_With(&mut self, node: &With) {}
    fn visit_WithItem(&mut self, node: &WithItem) {}
    fn visit_Yield(&mut self, node: &Yield) {}
}

pub fn walk_statement<V: CSTVisitor + ?Sized>(visitor: &mut V, statement: &Statement) {
    match statement {
        Statement::Simple(simple_statement_line) => {
            visitor.visit_simple_statement_line(simple_statement_line)
        }
        Statement::Compound(compound_statement) => {
            visitor.visit_compound_statement(compound_statement)
        }
    }
}

pub fn walk_simple_statement_line<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    simple_statement_line: &SimpleStatementLine,
) {
    for small_statement in &simple_statement_line.body {
        visitor.visit_small_statement(small_statement);
    }
}

pub fn walk_compound_statement<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    compound_statement: &CompoundStatement,
) {
    match compound_statement {
        CompoundStatement::FunctionDef(_) => {}
        CompoundStatement::If(_) => {}
        CompoundStatement::For(_) => {}
        CompoundStatement::While(_) => {}
        CompoundStatement::ClassDef(_) => {}
        CompoundStatement::Try(_) => {}
        CompoundStatement::TryStar(_) => {}
        CompoundStatement::With(_) => {}
        CompoundStatement::Match(_) => {}
    }
}

pub fn walk_small_statement<V: CSTVisitor + ?Sized>(
    visitor: &mut V,
    small_statement: &SmallStatement,
) {
    match small_statement {
        SmallStatement::Pass(_) => {}
        SmallStatement::Break(_) => {}
        SmallStatement::Continue(_) => {}
        SmallStatement::Return(inner) => {
            if let Some(expression) = &inner.value {
                visitor.visit_expression(expression);
            }
        }
        SmallStatement::Expr(inner) => {
            visitor.visit_expression(&inner.value);
        }
        SmallStatement::Assert(inner) => {
            visitor.visit_expression(&inner.test);
            if let Some(expression) = &inner.msg {
                visitor.visit_expression(expression);
            }
        }
        SmallStatement::Import(inner) => {
            // Do I really need to recurse here?
            for name in &inner.names {
                visitor.visit_ImportAlias(name);
            }
        }
        SmallStatement::ImportFrom(inner) => {
            // Do I really need to recurse here?
            for name in &inner.names {
                visitor.visit_Name(name);
            }
        }
        SmallStatement::Assign(_) => {}
        SmallStatement::AnnAssign(_) => {}
        SmallStatement::Raise(_) => {}
        SmallStatement::Global(_) => {}
        SmallStatement::Nonlocal(_) => {}
        SmallStatement::AugAssign(_) => {}
        SmallStatement::Del(_) => {}
    }
}

pub fn walk_expression<V: CSTVisitor + ?Sized>(visitor: &mut V, expression: &Expression) {
    match expression {
        Expression::Name(_) => {}
        Expression::Ellipsis(_) => {}
        Expression::Integer(_) => {}
        Expression::Float(_) => {}
        Expression::Imaginary(_) => {}
        Expression::Comparison(_) => {}
        Expression::UnaryOperation(_) => {}
        Expression::BinaryOperation(_) => {}
        Expression::BooleanOperation(_) => {}
        Expression::Attribute(_) => {}
        Expression::Tuple(_) => {}
        Expression::Call(_) => {}
        Expression::GeneratorExp(_) => {}
        Expression::ListComp(_) => {}
        Expression::SetComp(_) => {}
        Expression::DictComp(_) => {}
        Expression::List(_) => {}
        Expression::Set(_) => {}
        Expression::Dict(_) => {}
        Expression::Subscript(_) => {}
        Expression::StarredElement(_) => {}
        Expression::IfExp(_) => {}
        Expression::Lambda(_) => {}
        Expression::Yield(_) => {}
        Expression::Await(_) => {}
        Expression::SimpleString(_) => {}
        Expression::ConcatenatedString(_) => {}
        Expression::FormattedString(_) => {}
        Expression::NamedExpr(_) => {}
    }
}
