use super::{
    ComparableAlias, ComparableArguments, ComparableBytesLiteral, ComparableComprehension,
    ComparableDecorator, ComparableDictItem, ComparableElifElseClause, ComparableExceptHandler,
    ComparableExpr, ComparableFString, ComparableInterpolatedStringElement, ComparableKeyword,
    ComparableMatchCase, ComparableMod, ComparableModExpression, ComparableModModule,
    ComparableParameter, ComparableParameterWithDefault, ComparableParameters, ComparablePattern,
    ComparablePatternArguments, ComparablePatternKeyword, ComparableStmt, ComparableStringLiteral,
    ComparableTString, ComparableTypeParam, ComparableTypeParams, ComparableWithItem, Cow,
    ExceptHandlerExceptHandler, Expr, ExprAttribute, ExprAwait, ExprBinOp, ExprBoolLiteral,
    ExprBoolOp, ExprBytesLiteral, ExprCall, ExprCompare, ExprDict, ExprDictComp, ExprFString,
    ExprGenerator, ExprIf, ExprIpyEscapeCommand, ExprLambda, ExprList, ExprListComp, ExprName,
    ExprNamed, ExprNumberLiteral, ExprSet, ExprSetComp, ExprSlice, ExprStarred, ExprStringLiteral,
    ExprSubscript, ExprTString, ExprTuple, ExprUnaryOp, ExprYield, ExprYieldFrom, HashableExpr,
    HashableExprKind, HashableNumber, HashableReal, InterpolatedElement, Number, PatternMatchAs,
    PatternMatchClass, PatternMatchMapping, PatternMatchOr, PatternMatchSequence,
    PatternMatchSingleton, PatternMatchStar, PatternMatchValue, StmtAnnAssign, StmtAssert,
    StmtAssign, StmtAugAssign, StmtClassDef, StmtDelete, StmtExpr, StmtFor, StmtFunctionDef,
    StmtGlobal, StmtIf, StmtImport, StmtImportFrom, StmtIpyEscapeCommand, StmtMatch, StmtNonlocal,
    StmtRaise, StmtReturn, StmtTry, StmtTypeAlias, StmtWhile, StmtWith, TypeParamParamSpec,
    TypeParamTypeVar, TypeParamTypeVarTuple, ast,
};
use crate::flat::{self, fields};

impl<'a> From<flat::View<'a, ast::Alias>> for ComparableAlias<'a> {
    fn from(alias: flat::View<'a, ast::Alias>) -> Self {
        Self {
            name: alias.name().as_str(),
            asname: alias
                .asname()
                .map(super::super::flat::View::<crate::Identifier>::as_str),
        }
    }
}

impl<'a> From<flat::View<'a, ast::WithItem>> for ComparableWithItem<'a> {
    fn from(with_item: flat::View<'a, ast::WithItem>) -> Self {
        Self {
            context_expr: (with_item.context_expr()).into(),
            optional_vars: with_item.optional_vars().map(Into::into),
        }
    }
}

impl<'a> From<flat::View<'a, ast::PatternArguments>> for ComparablePatternArguments<'a> {
    fn from(parameters: flat::View<'a, ast::PatternArguments>) -> Self {
        Self {
            patterns: parameters.patterns().iter().map(Into::into).collect(),
            keywords: parameters.keywords().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::PatternKeyword>> for ComparablePatternKeyword<'a> {
    fn from(keyword: flat::View<'a, ast::PatternKeyword>) -> Self {
        Self {
            attr: keyword.attr().as_str(),
            pattern: (keyword.pattern()).into(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Pattern>> for ComparablePattern<'a> {
    fn from(pattern: flat::View<'a, ast::Pattern>) -> Self {
        match pattern.fields() {
            fields::Pattern::MatchValue(fields::PatternMatchValue { value, .. }) => {
                Self::MatchValue(PatternMatchValue {
                    value: value.into(),
                })
            }
            fields::Pattern::MatchSingleton(fields::PatternMatchSingleton { value, .. }) => {
                Self::MatchSingleton(PatternMatchSingleton {
                    value: (&value).into(),
                })
            }
            fields::Pattern::MatchSequence(fields::PatternMatchSequence { patterns, .. }) => {
                Self::MatchSequence(PatternMatchSequence {
                    patterns: patterns.iter().map(Into::into).collect(),
                })
            }
            fields::Pattern::MatchMapping(fields::PatternMatchMapping {
                keys,
                patterns,
                rest,
                ..
            }) => Self::MatchMapping(PatternMatchMapping {
                keys: keys.iter().map(Into::into).collect(),
                patterns: patterns.iter().map(Into::into).collect(),
                rest: rest.map(super::super::flat::View::<crate::Identifier>::as_str),
            }),
            fields::Pattern::MatchClass(fields::PatternMatchClass { cls, arguments, .. }) => {
                Self::MatchClass(PatternMatchClass {
                    cls: cls.into(),
                    arguments: arguments.into(),
                })
            }
            fields::Pattern::MatchStar(fields::PatternMatchStar { name, .. }) => {
                Self::MatchStar(PatternMatchStar {
                    name: name.map(super::super::flat::View::<crate::Identifier>::as_str),
                })
            }
            fields::Pattern::MatchAs(fields::PatternMatchAs { pattern, name, .. }) => {
                Self::MatchAs(PatternMatchAs {
                    pattern: pattern.map(Into::into),
                    name: name.map(super::super::flat::View::<crate::Identifier>::as_str),
                })
            }
            fields::Pattern::MatchOr(fields::PatternMatchOr { patterns, .. }) => {
                Self::MatchOr(PatternMatchOr {
                    patterns: patterns.iter().map(Into::into).collect(),
                })
            }
        }
    }
}

impl<'a> From<flat::View<'a, ast::MatchCase>> for ComparableMatchCase<'a> {
    fn from(match_case: flat::View<'a, ast::MatchCase>) -> Self {
        Self {
            pattern: (match_case.pattern()).into(),
            guard: match_case.guard().map(Into::into),
            body: match_case.body().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Decorator>> for ComparableDecorator<'a> {
    fn from(decorator: flat::View<'a, ast::Decorator>) -> Self {
        Self {
            expression: (decorator.expression()).into(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Arguments>> for ComparableArguments<'a> {
    fn from(arguments: flat::View<'a, ast::Arguments>) -> Self {
        Self {
            args: arguments.args().iter().map(Into::into).collect(),
            keywords: arguments.keywords().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Parameters>> for ComparableParameters<'a> {
    fn from(parameters: flat::View<'a, ast::Parameters>) -> Self {
        Self {
            posonlyargs: parameters.posonlyargs().iter().map(Into::into).collect(),
            args: parameters.args().iter().map(Into::into).collect(),
            vararg: parameters.vararg().map(Into::into),
            kwonlyargs: parameters.kwonlyargs().iter().map(Into::into).collect(),
            kwarg: parameters.kwarg().map(Into::into),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Parameter>> for ComparableParameter<'a> {
    fn from(arg: flat::View<'a, ast::Parameter>) -> Self {
        Self {
            arg: arg.name().as_str(),
            annotation: arg.annotation().map(Into::into),
        }
    }
}

impl<'a> From<flat::View<'a, ast::ParameterWithDefault>> for ComparableParameterWithDefault<'a> {
    fn from(arg: flat::View<'a, ast::ParameterWithDefault>) -> Self {
        Self {
            def: (arg.parameter()).into(),
            default: arg.default().map(Into::into),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Keyword>> for ComparableKeyword<'a> {
    fn from(keyword: flat::View<'a, ast::Keyword>) -> Self {
        Self {
            arg: keyword
                .arg()
                .map(super::super::flat::View::<crate::Identifier>::as_str),
            value: (keyword.value()).into(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Comprehension>> for ComparableComprehension<'a> {
    fn from(comprehension: flat::View<'a, ast::Comprehension>) -> Self {
        Self {
            target: (comprehension.target()).into(),
            iter: (comprehension.iter()).into(),
            ifs: comprehension.ifs().iter().map(Into::into).collect(),
            is_async: comprehension.is_async(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::ExceptHandler>> for ComparableExceptHandler<'a> {
    fn from(except_handler: flat::View<'a, ast::ExceptHandler>) -> Self {
        let fields::ExceptHandler::ExceptHandler(fields::ExceptHandlerExceptHandler {
            type_,
            name,
            body,
            ..
        }) = except_handler.fields();
        Self::ExceptHandler(ExceptHandlerExceptHandler {
            type_: type_.map(Into::into),
            name: name.map(super::super::flat::View::<crate::Identifier>::as_str),
            body: body.iter().map(Into::into).collect(),
        })
    }
}

impl<'a> From<flat::View<'a, ast::InterpolatedStringElement>>
    for ComparableInterpolatedStringElement<'a>
{
    fn from(interpolated_string_element: flat::View<'a, ast::InterpolatedStringElement>) -> Self {
        match interpolated_string_element.fields() {
            fields::InterpolatedStringElement::Literal(
                fields::InterpolatedStringLiteralElement { value, .. },
            ) => Self::Literal(value.into()),
            fields::InterpolatedStringElement::Interpolation(fields::InterpolatedElement {
                node: formatted_value,
                ..
            }) => formatted_value.into(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::InterpolatedElement>> for InterpolatedElement<'a> {
    fn from(interpolated_element: flat::View<'a, ast::InterpolatedElement>) -> Self {
        let fields::InterpolatedElement {
            expression,
            debug_text,
            conversion,
            format_spec,
            range: _,
            node: _,
        } = interpolated_element.fields();

        Self {
            expression: (expression).into(),
            debug_text: debug_text.map(Into::into),
            conversion,
            format_spec: format_spec
                .as_ref()
                .map(|spec| spec.elements().iter().map(Into::into).collect()),
        }
    }
}

impl<'a> From<flat::View<'a, ast::InterpolatedElement>>
    for ComparableInterpolatedStringElement<'a>
{
    fn from(interpolated_element: flat::View<'a, ast::InterpolatedElement>) -> Self {
        Self::InterpolatedElement(interpolated_element.into())
    }
}

impl<'a> From<flat::View<'a, ast::ElifElseClause>> for ComparableElifElseClause<'a> {
    fn from(elif_else_clause: flat::View<'a, ast::ElifElseClause>) -> Self {
        let fields::ElifElseClause {
            range: _,
            node: _,
            test,
            body,
        } = elif_else_clause.fields();
        Self {
            test: test.map(Into::into),
            body: body.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::StringLiteral>> for ComparableStringLiteral<'a> {
    fn from(string_literal: flat::View<'a, ast::StringLiteral>) -> Self {
        Self {
            value: string_literal.value(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::BytesLiteral>> for ComparableBytesLiteral<'a> {
    fn from(bytes_literal: flat::View<'a, ast::BytesLiteral>) -> Self {
        Self {
            value: Cow::Borrowed(bytes_literal.value()),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Expr>> for ComparableExpr<'a> {
    fn from(expr: flat::View<'a, ast::Expr>) -> Self {
        match expr.fields() {
            fields::Expr::BoolOp(fields::ExprBoolOp {
                op,
                values,
                range: _,
                node: _,
            }) => Self::BoolOp(ExprBoolOp {
                op: op.into(),
                values: values.iter().map(Into::into).collect(),
            }),
            fields::Expr::Named(fields::ExprNamed {
                target,
                value,
                range: _,
                node: _,
            }) => Self::NamedExpr(ExprNamed {
                target: target.into(),
                value: value.into(),
            }),
            fields::Expr::BinOp(fields::ExprBinOp {
                left,
                op,
                right,
                range: _,
                node: _,
            }) => Self::BinOp(ExprBinOp {
                left: left.into(),
                op: op.into(),
                right: right.into(),
            }),
            fields::Expr::UnaryOp(fields::ExprUnaryOp {
                op,
                operand,
                range: _,
                node: _,
            }) => Self::UnaryOp(ExprUnaryOp {
                op: op.into(),
                operand: operand.into(),
            }),
            fields::Expr::Lambda(fields::ExprLambda {
                parameters,
                body,
                range: _,
                node: _,
            }) => Self::Lambda(ExprLambda {
                parameters: parameters.map(Into::into),
                body: body.into(),
            }),
            fields::Expr::If(fields::ExprIf {
                test,
                body,
                orelse,
                range: _,
                node: _,
            }) => Self::IfExp(ExprIf {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            }),
            fields::Expr::Dict(fields::ExprDict {
                items,
                range: _,
                node: _,
            }) => Self::Dict(ExprDict {
                items: items.iter().map(ComparableDictItem::from).collect(),
            }),
            fields::Expr::Set(fields::ExprSet {
                elts,
                range: _,
                node: _,
            }) => Self::Set(ExprSet {
                elts: elts.iter().map(Into::into).collect(),
            }),
            fields::Expr::ListComp(fields::ExprListComp {
                elt,
                generators,
                range: _,
                node: _,
            }) => Self::ListComp(ExprListComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            fields::Expr::SetComp(fields::ExprSetComp {
                elt,
                generators,
                range: _,
                node: _,
            }) => Self::SetComp(ExprSetComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            fields::Expr::DictComp(fields::ExprDictComp {
                key,
                value,
                generators,
                range: _,
                node: _,
            }) => Self::DictComp(ExprDictComp {
                key: key.map(Into::into),
                value: value.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            fields::Expr::Generator(fields::ExprGenerator {
                elt,
                generators,
                range: _,
                node: _,
                parenthesized: _,
            }) => Self::GeneratorExp(ExprGenerator {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            fields::Expr::Await(fields::ExprAwait {
                value,
                range: _,
                node: _,
            }) => Self::Await(ExprAwait {
                value: value.into(),
            }),
            fields::Expr::Yield(fields::ExprYield {
                value,
                range: _,
                node: _,
            }) => Self::Yield(ExprYield {
                value: value.map(Into::into),
            }),
            fields::Expr::YieldFrom(fields::ExprYieldFrom {
                value,
                range: _,
                node: _,
            }) => Self::YieldFrom(ExprYieldFrom {
                value: value.into(),
            }),
            fields::Expr::Compare(fields::ExprCompare {
                ops,
                operands,
                range: _,
                node: _,
            }) => Self::Compare(ExprCompare {
                ops: ops.iter().map(Into::into).collect(),
                operands: operands.iter().map(Into::into).collect(),
            }),
            fields::Expr::Call(fields::ExprCall {
                func,
                arguments,
                range: _,
                node: _,
            }) => Self::Call(ExprCall {
                func: func.into(),
                arguments: arguments.into(),
            }),
            fields::Expr::FString(fields::ExprFString {
                value,
                range: _,
                node: _,
            }) => Self::FString(ExprFString {
                value: value.into(),
            }),
            fields::Expr::TString(fields::ExprTString {
                value,
                range: _,
                node: _,
            }) => Self::TString(ExprTString {
                value: value.into(),
            }),
            fields::Expr::StringLiteral(fields::ExprStringLiteral {
                value,
                range: _,
                node: _,
            }) => Self::StringLiteral(ExprStringLiteral {
                value: ComparableStringLiteral {
                    value: value.to_str(),
                },
            }),
            fields::Expr::BytesLiteral(fields::ExprBytesLiteral {
                value,
                range: _,
                node: _,
            }) => Self::BytesLiteral(ExprBytesLiteral {
                value: ComparableBytesLiteral {
                    value: value.into(),
                },
            }),
            fields::Expr::NumberLiteral(fields::ExprNumberLiteral {
                value,
                range: _,
                node: _,
            }) => Self::NumberLiteral(ExprNumberLiteral {
                value: value.into(),
            }),
            fields::Expr::BooleanLiteral(fields::ExprBooleanLiteral {
                value,
                range: _,
                node: _,
            }) => Self::BoolLiteral(ExprBoolLiteral { value }),
            fields::Expr::NoneLiteral(_) => Self::NoneLiteral,
            fields::Expr::EllipsisLiteral(_) => Self::EllipsisLiteral,
            fields::Expr::Attribute(fields::ExprAttribute {
                value,
                attr,
                ctx: _,
                range: _,
                node: _,
            }) => Self::Attribute(ExprAttribute {
                value: value.into(),
                attr: attr.as_str(),
            }),
            fields::Expr::Subscript(fields::ExprSubscript {
                value,
                slice,
                ctx: _,
                range: _,
                node: _,
            }) => Self::Subscript(ExprSubscript {
                value: value.into(),
                slice: slice.into(),
            }),
            fields::Expr::Starred(fields::ExprStarred {
                value,
                ctx: _,
                range: _,
                node: _,
            }) => Self::Starred(ExprStarred {
                value: value.into(),
            }),
            fields::Expr::Name(fields::ExprName { node: name, .. }) => name.into(),
            fields::Expr::List(fields::ExprList {
                elts,
                ctx: _,
                range: _,
                node: _,
            }) => Self::List(ExprList {
                elts: elts.iter().map(Into::into).collect(),
            }),
            fields::Expr::Tuple(fields::ExprTuple {
                elts,
                ctx: _,
                range: _,
                node: _,
                parenthesized: _,
            }) => Self::Tuple(ExprTuple {
                elts: elts.iter().map(Into::into).collect(),
            }),
            fields::Expr::Slice(fields::ExprSlice {
                lower,
                upper,
                step,
                range: _,
                node: _,
            }) => Self::Slice(ExprSlice {
                lower: lower.map(Into::into),
                upper: upper.map(Into::into),
                step: step.map(Into::into),
            }),
            fields::Expr::IpyEscapeCommand(fields::ExprIpyEscapeCommand {
                kind,
                value,
                range: _,
                node: _,
            }) => Self::IpyEscapeCommand(ExprIpyEscapeCommand { kind, value }),
        }
    }
}

impl<'a> From<flat::View<'a, ast::ExprName>> for ComparableExpr<'a> {
    fn from(expr: flat::View<'a, ast::ExprName>) -> Self {
        Self::Name(ExprName {
            id: expr.id().as_str(),
        })
    }
}

impl<'a> From<flat::View<'a, ast::TypeParams>> for ComparableTypeParams<'a> {
    fn from(type_params: flat::View<'a, ast::TypeParams>) -> Self {
        Self {
            type_params: type_params.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::TypeParam>> for ComparableTypeParam<'a> {
    fn from(type_param: flat::View<'a, ast::TypeParam>) -> Self {
        match type_param.fields() {
            fields::TypeParam::TypeVar(fields::TypeParamTypeVar {
                name,
                bound,
                default,
                range: _,
                node: _,
            }) => Self::TypeVar(TypeParamTypeVar {
                name: name.as_str(),
                bound: bound.map(Into::into),
                default: default.map(Into::into),
            }),
            fields::TypeParam::TypeVarTuple(fields::TypeParamTypeVarTuple {
                name,
                default,
                range: _,
                node: _,
            }) => Self::TypeVarTuple(TypeParamTypeVarTuple {
                name: name.as_str(),
                default: default.map(Into::into),
            }),
            fields::TypeParam::ParamSpec(fields::TypeParamParamSpec {
                name,
                default,
                range: _,
                node: _,
            }) => Self::ParamSpec(TypeParamParamSpec {
                name: name.as_str(),
                default: default.map(Into::into),
            }),
        }
    }
}

impl<'a> From<flat::View<'a, ast::Stmt>> for ComparableStmt<'a> {
    fn from(stmt: flat::View<'a, ast::Stmt>) -> Self {
        match stmt.fields() {
            fields::Stmt::FunctionDef(fields::StmtFunctionDef {
                is_async,
                name,
                parameters,
                body,
                decorator_list,
                returns,
                type_params,
                range: _,
                node: _,
            }) => Self::FunctionDef(StmtFunctionDef {
                is_async,
                name: name.as_str(),
                parameters: parameters.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.map(Into::into),
                type_params: type_params.map(Into::into),
            }),
            fields::Stmt::ClassDef(fields::StmtClassDef {
                name,
                arguments,
                body,
                decorator_list,
                type_params,
                range: _,
                node: _,
            }) => Self::ClassDef(StmtClassDef {
                name: name.as_str(),
                arguments: arguments.map(Into::into).unwrap_or_default(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                type_params: type_params.map(Into::into),
            }),
            fields::Stmt::Return(fields::StmtReturn {
                value,
                range: _,
                node: _,
            }) => Self::Return(StmtReturn {
                value: value.map(Into::into),
            }),
            fields::Stmt::Delete(fields::StmtDelete {
                targets,
                range: _,
                node: _,
            }) => Self::Delete(StmtDelete {
                targets: targets.iter().map(Into::into).collect(),
            }),
            fields::Stmt::TypeAlias(fields::StmtTypeAlias {
                range: _,
                node: _,
                name,
                type_params,
                value,
            }) => Self::TypeAlias(StmtTypeAlias {
                name: name.into(),
                type_params: type_params.map(Into::into),
                value: value.into(),
            }),
            fields::Stmt::Assign(fields::StmtAssign {
                targets,
                value,
                range: _,
                node: _,
            }) => Self::Assign(StmtAssign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
            }),
            fields::Stmt::AugAssign(fields::StmtAugAssign {
                target,
                op,
                value,
                range: _,
                node: _,
            }) => Self::AugAssign(StmtAugAssign {
                target: target.into(),
                op: op.into(),
                value: value.into(),
            }),
            fields::Stmt::AnnAssign(fields::StmtAnnAssign {
                target,
                annotation,
                value,
                simple,
                range: _,
                node: _,
            }) => Self::AnnAssign(StmtAnnAssign {
                target: target.into(),
                annotation: annotation.into(),
                value: value.map(Into::into),
                simple,
            }),
            fields::Stmt::For(fields::StmtFor {
                is_async,
                target,
                iter,
                body,
                orelse,
                range: _,
                node: _,
            }) => Self::For(StmtFor {
                is_async,
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            fields::Stmt::While(fields::StmtWhile {
                test,
                body,
                orelse,
                range: _,
                node: _,
            }) => Self::While(StmtWhile {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            fields::Stmt::If(fields::StmtIf {
                test,
                body,
                elif_else_clauses,
                range: _,
                node: _,
            }) => Self::If(StmtIf {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                elif_else_clauses: elif_else_clauses.iter().map(Into::into).collect(),
            }),
            fields::Stmt::With(fields::StmtWith {
                is_async,
                items,
                body,
                range: _,
                node: _,
            }) => Self::With(StmtWith {
                is_async,
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
            }),
            fields::Stmt::Match(fields::StmtMatch {
                subject,
                cases,
                range: _,
                node: _,
            }) => Self::Match(StmtMatch {
                subject: subject.into(),
                cases: cases.iter().map(Into::into).collect(),
            }),
            fields::Stmt::Raise(fields::StmtRaise {
                exc,
                cause,
                range: _,
                node: _,
            }) => Self::Raise(StmtRaise {
                exc: exc.map(Into::into),
                cause: cause.map(Into::into),
            }),
            fields::Stmt::Try(fields::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
                node: _,
            }) => Self::Try(StmtTry {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
                is_star,
            }),
            fields::Stmt::Assert(fields::StmtAssert {
                test,
                msg,
                range: _,
                node: _,
            }) => Self::Assert(StmtAssert {
                test: test.into(),
                msg: msg.map(Into::into),
            }),
            fields::Stmt::Import(fields::StmtImport {
                names,
                is_lazy,
                range: _,
                node: _,
            }) => Self::Import(StmtImport {
                names: names.iter().map(Into::into).collect(),
                is_lazy,
            }),
            fields::Stmt::ImportFrom(fields::StmtImportFrom {
                module,
                names,
                level,
                is_lazy,
                range: _,
                node: _,
            }) => Self::ImportFrom(StmtImportFrom {
                module: module.map(super::super::flat::View::<crate::Identifier>::as_str),
                names: names.iter().map(Into::into).collect(),
                level,
                is_lazy,
            }),
            fields::Stmt::Global(fields::StmtGlobal {
                names,
                range: _,
                node: _,
            }) => Self::Global(StmtGlobal {
                names: names
                    .iter()
                    .map(super::super::flat::View::<crate::Identifier>::as_str)
                    .collect(),
            }),
            fields::Stmt::Nonlocal(fields::StmtNonlocal {
                names,
                range: _,
                node: _,
            }) => Self::Nonlocal(StmtNonlocal {
                names: names
                    .iter()
                    .map(super::super::flat::View::<crate::Identifier>::as_str)
                    .collect(),
            }),
            fields::Stmt::IpyEscapeCommand(fields::StmtIpyEscapeCommand {
                kind,
                value,
                range: _,
                node: _,
            }) => Self::IpyEscapeCommand(StmtIpyEscapeCommand { kind, value }),
            fields::Stmt::Expr(fields::StmtExpr {
                value,
                range: _,
                node: _,
            }) => Self::Expr(StmtExpr {
                value: value.into(),
            }),
            fields::Stmt::Pass(_) => Self::Pass,
            fields::Stmt::Break(_) => Self::Break,
            fields::Stmt::Continue(_) => Self::Continue,
        }
    }
}

impl<'a> From<flat::View<'a, ast::Mod>> for ComparableMod<'a> {
    fn from(mod_: flat::View<'a, ast::Mod>) -> Self {
        match mod_.classify() {
            flat::ModKind::Module(module) => Self::Module(module.into()),
            flat::ModKind::Expression(expr) => Self::Expression(expr.into()),
        }
    }
}

impl<'a> From<flat::View<'a, ast::ModModule>> for ComparableModModule<'a> {
    fn from(module: flat::View<'a, ast::ModModule>) -> Self {
        Self {
            body: module.body().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<flat::View<'a, ast::ModExpression>> for ComparableModExpression<'a> {
    fn from(expr: flat::View<'a, ast::ModExpression>) -> Self {
        Self {
            body: expr.body().into(),
        }
    }
}

impl<'a> From<flat::View<'a, Expr>> for HashableExpr<'a> {
    fn from(expr: flat::View<'a, Expr>) -> Self {
        /// Returns a version of the given expression that can be hashed and compared according to
        /// Python  semantics.
        fn as_hashable(expr: flat::View<'_, Expr>) -> HashableExprKind<'_> {
            if let Some(constant) = as_hashable_constant(expr) {
                return constant;
            }

            match expr.classify() {
                flat::ExprKind::Named(named) => HashableExprKind::NamedExpr {
                    target: ComparableExpr::from(named.target()),
                    value: Box::new(as_hashable(named.value())),
                },
                _ => HashableExprKind::Comparable(ComparableExpr::from(expr)),
            }
        }

        /// Returns a hashable representation if the expression's value is statically known.
        fn as_hashable_constant(expr: flat::View<'_, Expr>) -> Option<HashableExprKind<'_>> {
            if let Some(number) = as_number(expr) {
                return Some(HashableExprKind::Number(number));
            }

            let kind = match expr.classify() {
                flat::ExprKind::Tuple(tuple) => HashableExprKind::Tuple(
                    tuple
                        .elts()
                        .iter()
                        .map(as_hashable_constant)
                        .collect::<Option<_>>()?,
                ),
                _ if expr.is_literal_expr() => {
                    HashableExprKind::Comparable(ComparableExpr::from(expr))
                }
                _ => return None,
            };

            Some(kind)
        }

        fn as_number(expr: flat::View<'_, Expr>) -> Option<HashableNumber> {
            match expr.fields() {
                fields::Expr::BooleanLiteral(fields::ExprBooleanLiteral {
                    node: boolean, ..
                }) => Some(HashableNumber::real(HashableReal::Integer(i128::from(
                    u8::from(boolean.value()),
                )))),
                fields::Expr::NumberLiteral(fields::ExprNumberLiteral { node: number, .. }) => {
                    match &number.value() {
                        Number::Int(int) => HashableReal::from_int(int).map(HashableNumber::real),
                        Number::Float(float) => {
                            Some(HashableNumber::real(HashableReal::from_float(*float)))
                        }
                        Number::Complex { real, imag } => Some(HashableNumber::complex(
                            HashableReal::from_float(*real),
                            HashableReal::from_float(*imag),
                        )),
                    }
                }
                fields::Expr::UnaryOp(fields::ExprUnaryOp { op, operand, .. }) => match op {
                    ast::UnaryOp::UAdd => as_number(operand),
                    ast::UnaryOp::USub => as_number(operand).map(HashableNumber::negate),
                    ast::UnaryOp::Invert | ast::UnaryOp::Not => None,
                },
                fields::Expr::BinOp(fields::ExprBinOp {
                    left,
                    op: op @ (ast::Operator::Add | ast::Operator::Sub),
                    right,
                    ..
                }) => {
                    let real = as_number(left)?.into_real()?.into_float();
                    let fields::Expr::NumberLiteral(fields::ExprNumberLiteral {
                        value:
                            Number::Complex {
                                real: complex_real,
                                imag,
                            },
                        ..
                    }) = right.fields()
                    else {
                        return None;
                    };
                    let complex_real = HashableReal::from_float(*complex_real);
                    if !complex_real.is_zero() {
                        return None;
                    }
                    let mut imag = HashableReal::from_float(*imag);
                    if op.is_sub() {
                        imag.negate();
                    }
                    Some(HashableNumber::complex(real, imag))
                }
                _ => None,
            }
        }

        Self(as_hashable(expr))
    }
}
impl<'a> From<flat::View<'a, ast::Expr>> for Box<ComparableExpr<'a>> {
    fn from(expr: flat::View<'a, ast::Expr>) -> Self {
        Box::new(ComparableExpr::from(expr))
    }
}

impl<'a> From<flat::View<'a, ast::Pattern>> for Box<ComparablePattern<'a>> {
    fn from(pattern: flat::View<'a, ast::Pattern>) -> Self {
        Box::new(pattern.into())
    }
}

impl<'a> From<flat::DictItemView<'a>> for ComparableDictItem<'a> {
    fn from(item: flat::DictItemView<'a>) -> Self {
        Self {
            key: item.key.map(Into::into),
            value: item.value.into(),
        }
    }
}

impl<'a> From<flat::FStringValue<'a>> for ComparableFString<'a> {
    fn from(value: flat::FStringValue<'a>) -> Self {
        let mut elements = Vec::new();
        for part in value {
            match part {
                flat::FStringPartView::Literal(literal) => {
                    push_literal(&mut elements, literal.value());
                }
                flat::FStringPartView::FString(fstring) => {
                    for element in fstring.elements() {
                        match element.classify() {
                            flat::InterpolatedStringElementKind::Literal(literal) => {
                                push_literal(&mut elements, literal.value());
                            }
                            flat::InterpolatedStringElementKind::Interpolation(expr) => {
                                elements.push(expr.into());
                            }
                        }
                    }
                }
            }
        }
        Self {
            elements: elements.into_boxed_slice(),
        }
    }
}

impl<'a> From<flat::TStringValue<'a>> for ComparableTString<'a> {
    fn from(value: flat::TStringValue<'a>) -> Self {
        let mut strings = vec![ComparableInterpolatedStringElement::Literal("".into())];
        let mut interpolations = Vec::new();
        for element in value
            .iter()
            .flat_map(super::super::flat::View::<crate::TString>::elements)
        {
            match element.classify() {
                flat::InterpolatedStringElementKind::Literal(literal) => {
                    push_literal(&mut strings, literal.value());
                }
                flat::InterpolatedStringElementKind::Interpolation(expr) => {
                    interpolations.push(expr.into());
                    strings.push(ComparableInterpolatedStringElement::Literal("".into()));
                }
            }
        }
        Self {
            strings: strings.into_boxed_slice(),
            interpolations: interpolations.into_boxed_slice(),
        }
    }
}

fn push_literal<'a>(elements: &mut Vec<ComparableInterpolatedStringElement<'a>>, literal: &'a str) {
    if let Some(ComparableInterpolatedStringElement::Literal(existing)) = elements.last_mut() {
        existing.to_mut().push_str(literal);
    } else {
        elements.push(ComparableInterpolatedStringElement::Literal(literal.into()));
    }
}
