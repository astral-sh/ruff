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
use crate::flat;

impl<'a> From<ast::Alias<'a>> for ComparableAlias<'a> {
    fn from(alias: ast::Alias<'a>) -> Self {
        Self {
            name: alias.name().as_str(),
            asname: alias.asname().map(crate::Identifier::as_str),
        }
    }
}

impl<'a> From<ast::WithItem<'a>> for ComparableWithItem<'a> {
    fn from(with_item: ast::WithItem<'a>) -> Self {
        Self {
            context_expr: (with_item.context_expr()).into(),
            optional_vars: with_item.optional_vars().map(Into::into),
        }
    }
}

impl<'a> From<ast::PatternArguments<'a>> for ComparablePatternArguments<'a> {
    fn from(parameters: ast::PatternArguments<'a>) -> Self {
        Self {
            patterns: parameters.patterns().iter().map(Into::into).collect(),
            keywords: parameters.keywords().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<ast::PatternKeyword<'a>> for ComparablePatternKeyword<'a> {
    fn from(keyword: ast::PatternKeyword<'a>) -> Self {
        Self {
            attr: keyword.attr().as_str(),
            pattern: (keyword.pattern()).into(),
        }
    }
}

impl<'a> From<ast::Pattern<'a>> for ComparablePattern<'a> {
    fn from(pattern: ast::Pattern<'a>) -> Self {
        match pattern.fields() {
            ast::PatternFields::MatchValue(ast::PatternMatchValueFields { value, .. }) => {
                Self::MatchValue(PatternMatchValue {
                    value: value.into(),
                })
            }
            ast::PatternFields::MatchSingleton(ast::PatternMatchSingletonFields {
                value, ..
            }) => Self::MatchSingleton(PatternMatchSingleton {
                value: (&value).into(),
            }),
            ast::PatternFields::MatchSequence(ast::PatternMatchSequenceFields {
                patterns, ..
            }) => Self::MatchSequence(PatternMatchSequence {
                patterns: patterns.iter().map(Into::into).collect(),
            }),
            ast::PatternFields::MatchMapping(ast::PatternMatchMappingFields { items, .. }) => {
                Self::MatchMapping(PatternMatchMapping {
                    items: items
                        .into_iter()
                        .map(|item| match item {
                            crate::MappingPatternItem::Entry { key, pattern } => {
                                crate::MappingPatternItem::Entry {
                                    key: key.into(),
                                    pattern: pattern.into(),
                                }
                            }
                            crate::MappingPatternItem::Rest(identifier) => {
                                crate::MappingPatternItem::Rest(identifier.as_str())
                            }
                        })
                        .collect(),
                })
            }
            ast::PatternFields::MatchClass(ast::PatternMatchClassFields {
                cls, arguments, ..
            }) => Self::MatchClass(PatternMatchClass {
                cls: cls.into(),
                arguments: arguments.into(),
            }),
            ast::PatternFields::MatchStar(ast::PatternMatchStarFields { name, .. }) => {
                Self::MatchStar(PatternMatchStar {
                    name: name.map(crate::Identifier::as_str),
                })
            }
            ast::PatternFields::MatchAs(ast::PatternMatchAsFields { pattern, name, .. }) => {
                Self::MatchAs(PatternMatchAs {
                    pattern: pattern.map(Into::into),
                    name: name.map(crate::Identifier::as_str),
                })
            }
            ast::PatternFields::MatchOr(ast::PatternMatchOrFields { patterns, .. }) => {
                Self::MatchOr(PatternMatchOr {
                    patterns: patterns.iter().map(Into::into).collect(),
                })
            }
        }
    }
}

impl<'a> From<ast::MatchCase<'a>> for ComparableMatchCase<'a> {
    fn from(match_case: ast::MatchCase<'a>) -> Self {
        Self {
            pattern: (match_case.pattern()).into(),
            guard: match_case.guard().map(Into::into),
            body: match_case.body().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<ast::Decorator<'a>> for ComparableDecorator<'a> {
    fn from(decorator: ast::Decorator<'a>) -> Self {
        Self {
            expression: (decorator.expression()).into(),
        }
    }
}

impl<'a> From<ast::Arguments<'a>> for ComparableArguments<'a> {
    fn from(arguments: ast::Arguments<'a>) -> Self {
        Self {
            args: arguments.args().iter().map(Into::into).collect(),
            keywords: arguments.keywords().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<ast::Parameters<'a>> for ComparableParameters<'a> {
    fn from(parameters: ast::Parameters<'a>) -> Self {
        Self {
            posonlyargs: parameters.posonlyargs().iter().map(Into::into).collect(),
            args: parameters.args().iter().map(Into::into).collect(),
            vararg: parameters.vararg().map(Into::into),
            kwonlyargs: parameters.kwonlyargs().iter().map(Into::into).collect(),
            kwarg: parameters.kwarg().map(Into::into),
        }
    }
}

impl<'a> From<ast::Parameter<'a>> for ComparableParameter<'a> {
    fn from(arg: ast::Parameter<'a>) -> Self {
        Self {
            arg: arg.name().as_str(),
            annotation: arg.annotation().map(Into::into),
        }
    }
}

impl<'a> From<ast::ParameterWithDefault<'a>> for ComparableParameterWithDefault<'a> {
    fn from(arg: ast::ParameterWithDefault<'a>) -> Self {
        Self {
            def: (arg.parameter()).into(),
            default: arg.default().map(Into::into),
        }
    }
}

impl<'a> From<ast::Keyword<'a>> for ComparableKeyword<'a> {
    fn from(keyword: ast::Keyword<'a>) -> Self {
        Self {
            arg: keyword.arg().map(crate::Identifier::as_str),
            value: (keyword.value()).into(),
        }
    }
}

impl<'a> From<ast::Comprehension<'a>> for ComparableComprehension<'a> {
    fn from(comprehension: ast::Comprehension<'a>) -> Self {
        Self {
            target: (comprehension.target()).into(),
            iter: (comprehension.iter()).into(),
            ifs: comprehension.ifs().iter().map(Into::into).collect(),
            is_async: comprehension.is_async(),
        }
    }
}

impl<'a> From<ast::ExceptHandler<'a>> for ComparableExceptHandler<'a> {
    fn from(except_handler: ast::ExceptHandler<'a>) -> Self {
        let ast::ExceptHandlerFields::ExceptHandler(ast::ExceptHandlerExceptHandlerFields {
            type_,
            name,
            body,
            ..
        }) = except_handler.fields();
        Self::ExceptHandler(ExceptHandlerExceptHandler {
            type_: type_.map(Into::into),
            name: name.map(crate::Identifier::as_str),
            body: body.iter().map(Into::into).collect(),
        })
    }
}

impl<'a> From<ast::InterpolatedStringElement<'a>> for ComparableInterpolatedStringElement<'a> {
    fn from(interpolated_string_element: ast::InterpolatedStringElement<'a>) -> Self {
        match interpolated_string_element.fields() {
            ast::InterpolatedStringElementFields::Literal(
                ast::InterpolatedStringLiteralElementFields { value, .. },
            ) => Self::Literal(value.into()),
            ast::InterpolatedStringElementFields::Interpolation(
                ast::InterpolatedElementFields {
                    node: formatted_value,
                    ..
                },
            ) => formatted_value.into(),
        }
    }
}

impl<'a> From<ast::InterpolatedElement<'a>> for InterpolatedElement<'a> {
    fn from(interpolated_element: ast::InterpolatedElement<'a>) -> Self {
        let ast::InterpolatedElementFields {
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

impl<'a> From<ast::InterpolatedElement<'a>> for ComparableInterpolatedStringElement<'a> {
    fn from(interpolated_element: ast::InterpolatedElement<'a>) -> Self {
        Self::InterpolatedElement(interpolated_element.into())
    }
}

impl<'a> From<ast::ElifElseClause<'a>> for ComparableElifElseClause<'a> {
    fn from(elif_else_clause: ast::ElifElseClause<'a>) -> Self {
        let ast::ElifElseClauseFields {
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

impl<'a> From<ast::StringLiteral<'a>> for ComparableStringLiteral<'a> {
    fn from(string_literal: ast::StringLiteral<'a>) -> Self {
        Self {
            value: string_literal.value(),
        }
    }
}

impl<'a> From<ast::BytesLiteral<'a>> for ComparableBytesLiteral<'a> {
    fn from(bytes_literal: ast::BytesLiteral<'a>) -> Self {
        Self {
            value: Cow::Borrowed(bytes_literal.value()),
        }
    }
}

impl<'a> From<ast::Expr<'a>> for ComparableExpr<'a> {
    fn from(expr: ast::Expr<'a>) -> Self {
        match expr.fields() {
            ast::ExprFields::BoolOp(ast::ExprBoolOpFields {
                op,
                values,
                range: _,
                node: _,
            }) => Self::BoolOp(ExprBoolOp {
                op: op.into(),
                values: values.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::Named(ast::ExprNamedFields {
                target,
                value,
                range: _,
                node: _,
            }) => Self::NamedExpr(ExprNamed {
                target: target.into(),
                value: value.into(),
            }),
            ast::ExprFields::BinOp(ast::ExprBinOpFields {
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
            ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields {
                op,
                operand,
                range: _,
                node: _,
            }) => Self::UnaryOp(ExprUnaryOp {
                op: op.into(),
                operand: operand.into(),
            }),
            ast::ExprFields::Lambda(ast::ExprLambdaFields {
                parameters,
                body,
                range: _,
                node: _,
            }) => Self::Lambda(ExprLambda {
                parameters: parameters.map(Into::into),
                body: body.into(),
            }),
            ast::ExprFields::If(ast::ExprIfFields {
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
            ast::ExprFields::Dict(ast::ExprDictFields {
                items,
                range: _,
                node: _,
            }) => Self::Dict(ExprDict {
                items: items.iter().map(ComparableDictItem::from).collect(),
            }),
            ast::ExprFields::Set(ast::ExprSetFields {
                elts,
                range: _,
                node: _,
            }) => Self::Set(ExprSet {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::ListComp(ast::ExprListCompFields {
                elt,
                generators,
                range: _,
                node: _,
            }) => Self::ListComp(ExprListComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::SetComp(ast::ExprSetCompFields {
                elt,
                generators,
                range: _,
                node: _,
            }) => Self::SetComp(ExprSetComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::DictComp(ast::ExprDictCompFields {
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
            ast::ExprFields::Generator(ast::ExprGeneratorFields {
                elt,
                generators,
                range: _,
                node: _,
                parenthesized: _,
            }) => Self::GeneratorExp(ExprGenerator {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::Await(ast::ExprAwaitFields {
                value,
                range: _,
                node: _,
            }) => Self::Await(ExprAwait {
                value: value.into(),
            }),
            ast::ExprFields::Yield(ast::ExprYieldFields {
                value,
                range: _,
                node: _,
            }) => Self::Yield(ExprYield {
                value: value.map(Into::into),
            }),
            ast::ExprFields::YieldFrom(ast::ExprYieldFromFields {
                value,
                range: _,
                node: _,
            }) => Self::YieldFrom(ExprYieldFrom {
                value: value.into(),
            }),
            ast::ExprFields::Compare(ast::ExprCompareFields {
                ops,
                operands,
                range: _,
                node: _,
            }) => Self::Compare(ExprCompare {
                ops: ops.iter().map(Into::into).collect(),
                operands: operands.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::Call(ast::ExprCallFields {
                func,
                arguments,
                range: _,
                node: _,
            }) => Self::Call(ExprCall {
                func: func.into(),
                arguments: arguments.into(),
            }),
            ast::ExprFields::FString(ast::ExprFStringFields {
                value,
                range: _,
                node: _,
            }) => Self::FString(ExprFString {
                value: value.into(),
            }),
            ast::ExprFields::TString(ast::ExprTStringFields {
                value,
                range: _,
                node: _,
            }) => Self::TString(ExprTString {
                value: value.into(),
            }),
            ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields {
                value,
                range: _,
                node: _,
            }) => Self::StringLiteral(ExprStringLiteral {
                value: ComparableStringLiteral {
                    value: value.to_str(),
                },
            }),
            ast::ExprFields::BytesLiteral(ast::ExprBytesLiteralFields {
                value,
                range: _,
                node: _,
            }) => Self::BytesLiteral(ExprBytesLiteral {
                value: ComparableBytesLiteral {
                    value: value.into(),
                },
            }),
            ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields {
                value,
                range: _,
                node: _,
            }) => Self::NumberLiteral(ExprNumberLiteral {
                value: value.into(),
            }),
            ast::ExprFields::BooleanLiteral(ast::ExprBooleanLiteralFields {
                value,
                range: _,
                node: _,
            }) => Self::BoolLiteral(ExprBoolLiteral { value }),
            ast::ExprFields::NoneLiteral(_) => Self::NoneLiteral,
            ast::ExprFields::EllipsisLiteral(_) => Self::EllipsisLiteral,
            ast::ExprFields::Attribute(ast::ExprAttributeFields {
                value,
                attr,
                ctx: _,
                range: _,
                node: _,
            }) => Self::Attribute(ExprAttribute {
                value: value.into(),
                attr: attr.as_str(),
            }),
            ast::ExprFields::Subscript(ast::ExprSubscriptFields {
                value,
                slice,
                ctx: _,
                range: _,
                node: _,
            }) => Self::Subscript(ExprSubscript {
                value: value.into(),
                slice: slice.into(),
            }),
            ast::ExprFields::Starred(ast::ExprStarredFields {
                value,
                ctx: _,
                range: _,
                node: _,
            }) => Self::Starred(ExprStarred {
                value: value.into(),
            }),
            ast::ExprFields::Name(ast::ExprNameFields { node: name, .. }) => name.into(),
            ast::ExprFields::List(ast::ExprListFields {
                elts,
                ctx: _,
                range: _,
                node: _,
            }) => Self::List(ExprList {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::Tuple(ast::ExprTupleFields {
                elts,
                ctx: _,
                range: _,
                node: _,
                parenthesized: _,
            }) => Self::Tuple(ExprTuple {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::ExprFields::Slice(ast::ExprSliceFields {
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
            ast::ExprFields::IpyEscapeCommand(ast::ExprIpyEscapeCommandFields {
                kind,
                value,
                range: _,
                node: _,
            }) => Self::IpyEscapeCommand(ExprIpyEscapeCommand { kind, value }),
        }
    }
}

impl<'a> From<ast::ExprName<'a>> for ComparableExpr<'a> {
    fn from(expr: ast::ExprName<'a>) -> Self {
        Self::Name(ExprName {
            id: expr.id().as_str(),
        })
    }
}

impl<'a> From<ast::TypeParams<'a>> for ComparableTypeParams<'a> {
    fn from(type_params: ast::TypeParams<'a>) -> Self {
        Self {
            type_params: type_params.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<ast::TypeParam<'a>> for ComparableTypeParam<'a> {
    fn from(type_param: ast::TypeParam<'a>) -> Self {
        match type_param.fields() {
            ast::TypeParamFields::TypeVar(ast::TypeParamTypeVarFields {
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
            ast::TypeParamFields::TypeVarTuple(ast::TypeParamTypeVarTupleFields {
                name,
                default,
                range: _,
                node: _,
            }) => Self::TypeVarTuple(TypeParamTypeVarTuple {
                name: name.as_str(),
                default: default.map(Into::into),
            }),
            ast::TypeParamFields::ParamSpec(ast::TypeParamParamSpecFields {
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

impl<'a> From<ast::Stmt<'a>> for ComparableStmt<'a> {
    fn from(stmt: ast::Stmt<'a>) -> Self {
        match stmt.fields() {
            ast::StmtFields::FunctionDef(ast::StmtFunctionDefFields {
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
            ast::StmtFields::ClassDef(ast::StmtClassDefFields {
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
            ast::StmtFields::Return(ast::StmtReturnFields {
                value,
                range: _,
                node: _,
            }) => Self::Return(StmtReturn {
                value: value.map(Into::into),
            }),
            ast::StmtFields::Delete(ast::StmtDeleteFields {
                targets,
                range: _,
                node: _,
            }) => Self::Delete(StmtDelete {
                targets: targets.iter().map(Into::into).collect(),
            }),
            ast::StmtFields::TypeAlias(ast::StmtTypeAliasFields {
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
            ast::StmtFields::Assign(ast::StmtAssignFields {
                targets,
                value,
                range: _,
                node: _,
            }) => Self::Assign(StmtAssign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
            }),
            ast::StmtFields::AugAssign(ast::StmtAugAssignFields {
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
            ast::StmtFields::AnnAssign(ast::StmtAnnAssignFields {
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
            ast::StmtFields::For(ast::StmtForFields {
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
            ast::StmtFields::While(ast::StmtWhileFields {
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
            ast::StmtFields::If(ast::StmtIfFields {
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
            ast::StmtFields::With(ast::StmtWithFields {
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
            ast::StmtFields::Match(ast::StmtMatchFields {
                subject,
                cases,
                range: _,
                node: _,
            }) => Self::Match(StmtMatch {
                subject: subject.into(),
                cases: cases.iter().map(Into::into).collect(),
            }),
            ast::StmtFields::Raise(ast::StmtRaiseFields {
                exc,
                cause,
                range: _,
                node: _,
            }) => Self::Raise(StmtRaise {
                exc: exc.map(Into::into),
                cause: cause.map(Into::into),
            }),
            ast::StmtFields::Try(ast::StmtTryFields {
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
            ast::StmtFields::Assert(ast::StmtAssertFields {
                test,
                msg,
                range: _,
                node: _,
            }) => Self::Assert(StmtAssert {
                test: test.into(),
                msg: msg.map(Into::into),
            }),
            ast::StmtFields::Import(ast::StmtImportFields {
                names,
                is_lazy,
                range: _,
                node: _,
            }) => Self::Import(StmtImport {
                names: names.iter().map(Into::into).collect(),
                is_lazy,
            }),
            ast::StmtFields::ImportFrom(ast::StmtImportFromFields {
                module,
                names,
                level,
                is_lazy,
                range: _,
                node: _,
            }) => Self::ImportFrom(StmtImportFrom {
                module: module.map(crate::Identifier::as_str),
                names: names.iter().map(Into::into).collect(),
                level,
                is_lazy,
            }),
            ast::StmtFields::Global(ast::StmtGlobalFields {
                names,
                range: _,
                node: _,
            }) => Self::Global(StmtGlobal {
                names: names.iter().map(crate::Identifier::as_str).collect(),
            }),
            ast::StmtFields::Nonlocal(ast::StmtNonlocalFields {
                names,
                range: _,
                node: _,
            }) => Self::Nonlocal(StmtNonlocal {
                names: names.iter().map(crate::Identifier::as_str).collect(),
            }),
            ast::StmtFields::IpyEscapeCommand(ast::StmtIpyEscapeCommandFields {
                kind,
                value,
                range: _,
                node: _,
            }) => Self::IpyEscapeCommand(StmtIpyEscapeCommand { kind, value }),
            ast::StmtFields::Expr(ast::StmtExprFields {
                value,
                range: _,
                node: _,
            }) => Self::Expr(StmtExpr {
                value: value.into(),
            }),
            ast::StmtFields::Pass(_) => Self::Pass,
            ast::StmtFields::Break(_) => Self::Break,
            ast::StmtFields::Continue(_) => Self::Continue,
        }
    }
}

impl<'a> From<ast::Mod<'a>> for ComparableMod<'a> {
    fn from(mod_: ast::Mod<'a>) -> Self {
        match mod_.classify() {
            flat::ModKind::Module(module) => Self::Module(module.into()),
            flat::ModKind::Expression(expr) => Self::Expression(expr.into()),
        }
    }
}

impl<'a> From<ast::ModModule<'a>> for ComparableModModule<'a> {
    fn from(module: ast::ModModule<'a>) -> Self {
        Self {
            body: module.body().iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<ast::ModExpression<'a>> for ComparableModExpression<'a> {
    fn from(expr: ast::ModExpression<'a>) -> Self {
        Self {
            body: expr.body().into(),
        }
    }
}

impl<'a> From<Expr<'a>> for HashableExpr<'a> {
    fn from(expr: Expr<'a>) -> Self {
        /// Returns a version of the given expression that can be hashed and compared according to
        /// Python  semantics.
        fn as_hashable(expr: Expr<'_>) -> HashableExprKind<'_> {
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
        fn as_hashable_constant(expr: Expr<'_>) -> Option<HashableExprKind<'_>> {
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

        fn as_number(expr: Expr<'_>) -> Option<HashableNumber> {
            match expr.fields() {
                ast::ExprFields::BooleanLiteral(ast::ExprBooleanLiteralFields {
                    node: boolean,
                    ..
                }) => Some(HashableNumber::real(HashableReal::Integer(i128::from(
                    u8::from(boolean.value()),
                )))),
                ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields {
                    node: number,
                    ..
                }) => match &number.value() {
                    Number::Int(int) => HashableReal::from_int(int).map(HashableNumber::real),
                    Number::Float(float) => {
                        Some(HashableNumber::real(HashableReal::from_float(*float)))
                    }
                    Number::Complex { real, imag } => Some(HashableNumber::complex(
                        HashableReal::from_float(*real),
                        HashableReal::from_float(*imag),
                    )),
                },
                ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields { op, operand, .. }) => match op {
                    ast::UnaryOp::UAdd => as_number(operand),
                    ast::UnaryOp::USub => as_number(operand).map(HashableNumber::negate),
                    ast::UnaryOp::Invert | ast::UnaryOp::Not => None,
                },
                ast::ExprFields::BinOp(ast::ExprBinOpFields {
                    left,
                    op: op @ (ast::Operator::Add | ast::Operator::Sub),
                    right,
                    ..
                }) => {
                    let real = as_number(left)?.into_real()?.into_float();
                    let ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields {
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
impl<'a> From<ast::Expr<'a>> for Box<ComparableExpr<'a>> {
    fn from(expr: ast::Expr<'a>) -> Self {
        Box::new(ComparableExpr::from(expr))
    }
}

impl<'a> From<ast::Pattern<'a>> for Box<ComparablePattern<'a>> {
    fn from(pattern: ast::Pattern<'a>) -> Self {
        Box::new(pattern.into())
    }
}

impl<'a> From<ast::DictItem<'a>> for ComparableDictItem<'a> {
    fn from(item: ast::DictItem<'a>) -> Self {
        Self {
            key: item.key().map(Into::into),
            value: item.value().into(),
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
        for element in value.iter().flat_map(crate::TString::elements) {
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
