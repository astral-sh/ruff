use crate::{
    self as ast, Comprehension, DictItem, Expr, FStringPart, FStringParts, FStringValue,
    InterpolatedStringElement, Keyword, ParameterWithDefault, Parameters, TString,
};

/// A borrowed, resumable part of an expression search.
///
/// Each step performs a fixed amount of work without allocating or recursing. Frames contain only
/// references and flat traversal state, so copying or dropping one never traverses the AST.
#[derive(Clone, Copy)]
pub struct ExpressionSearchFrame<'ast> {
    state: FrameState<'ast>,
}

/// The next action in an expression search.
pub enum ExpressionSearchStep<'ast> {
    /// Apply the search predicate before visiting this expression's children.
    Visit(&'ast Expr),
    /// Search this child, then resume the already-advanced parent frame.
    Enter(ExpressionSearchFrame<'ast>),
    /// The frame advanced without visiting an expression or entering a child.
    Progress,
    /// This frame has no remaining work.
    Done,
}

#[derive(Clone, Copy)]
enum FrameState<'ast> {
    Expression(&'ast Expr),
    ExpressionChildren(&'ast Expr, Phase),
    Expressions(&'ast [Expr]),
    DictItems(&'ast [DictItem], bool),
    ParameterGroups(&'ast Parameters, Phase),
    ParameterDefaults(&'ast [ParameterWithDefault]),
    Comprehensions(&'ast [Comprehension]),
    Comprehension(&'ast Comprehension, Phase),
    Keywords(&'ast [Keyword]),
    FStringParts(&'ast [FStringPart]),
    TStrings(&'ast [TString]),
    InterpolatedElements(&'ast [InterpolatedStringElement]),
    InterpolatedElement(&'ast InterpolatedStringElement, Phase),
    Done,
}

#[derive(Clone, Copy)]
enum Phase {
    First,
    Second,
    Third,
    Done,
}

impl Phase {
    const fn next(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::Third,
            Self::Third | Self::Done => Self::Done,
        }
    }
}

impl<'ast> ExpressionSearchFrame<'ast> {
    /// Search an expression, visiting it before its children.
    pub const fn expression(expression: &'ast Expr) -> Self {
        Self {
            state: FrameState::Expression(expression),
        }
    }

    /// Search expressions in their stored order.
    pub const fn expressions(expressions: &'ast [Expr]) -> Self {
        Self {
            state: FrameState::Expressions(expressions),
        }
    }

    /// Search an interpolation's expression and then its format specification.
    /// Literal text contains no expressions to visit.
    pub const fn interpolated_element(element: &'ast InterpolatedStringElement) -> Self {
        Self {
            state: FrameState::InterpolatedElement(element, Phase::First),
        }
    }

    /// Advance one bounded transition. An entered child must finish before this frame resumes.
    pub fn step(&mut self) -> ExpressionSearchStep<'ast> {
        let state = self.state;
        self.state = FrameState::Done;
        match state {
            FrameState::Expression(expression) => {
                self.state = FrameState::ExpressionChildren(expression, Phase::First);
                ExpressionSearchStep::Visit(expression)
            }
            FrameState::ExpressionChildren(expression, phase) => {
                let step = Self::expression_children(expression, phase);
                if !matches!(step, ExpressionSearchStep::Done) {
                    self.state = FrameState::ExpressionChildren(expression, phase.next());
                }
                step
            }
            FrameState::Expressions(expressions) => {
                let Some((expression, rest)) = expressions.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::Expressions(rest);
                ExpressionSearchStep::Enter(Self::expression(expression))
            }
            FrameState::DictItems(items, visit_key) => {
                let Some((item, rest)) = items.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                if visit_key {
                    self.state = FrameState::DictItems(rest, false);
                    Self::optional_expression(item.key.as_ref())
                } else {
                    self.state = FrameState::DictItems(items, true);
                    ExpressionSearchStep::Enter(Self::expression(&item.value))
                }
            }
            FrameState::ParameterGroups(parameters, phase) => {
                let parameters_in_group = match phase {
                    Phase::First => &parameters.posonlyargs,
                    Phase::Second => &parameters.args,
                    Phase::Third => &parameters.kwonlyargs,
                    Phase::Done => return ExpressionSearchStep::Done,
                };
                self.state = FrameState::ParameterGroups(parameters, phase.next());
                ExpressionSearchStep::Enter(Self {
                    state: FrameState::ParameterDefaults(parameters_in_group),
                })
            }
            FrameState::ParameterDefaults(parameters) => {
                let Some((parameter, rest)) = parameters.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::ParameterDefaults(rest);
                Self::optional_expression(parameter.default.as_deref())
            }
            FrameState::Comprehensions(generators) => {
                let Some((generator, rest)) = generators.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::Comprehensions(rest);
                ExpressionSearchStep::Enter(Self {
                    state: FrameState::Comprehension(generator, Phase::First),
                })
            }
            FrameState::Comprehension(generator, phase) => {
                let child = match phase {
                    Phase::First => Self::expression(&generator.target),
                    Phase::Second => Self::expression(&generator.iter),
                    Phase::Third => Self::expressions(&generator.ifs),
                    Phase::Done => return ExpressionSearchStep::Done,
                };
                self.state = FrameState::Comprehension(generator, phase.next());
                ExpressionSearchStep::Enter(child)
            }
            FrameState::Keywords(keywords) => {
                let Some((keyword, rest)) = keywords.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::Keywords(rest);
                ExpressionSearchStep::Enter(Self::expression(&keyword.value))
            }
            FrameState::FStringParts(parts) => {
                let Some((part, rest)) = parts.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::FStringParts(rest);
                match part {
                    FStringPart::Literal(_) => ExpressionSearchStep::Progress,
                    FStringPart::FString(string) => ExpressionSearchStep::Enter(Self {
                        state: FrameState::InterpolatedElements(&string.elements),
                    }),
                }
            }
            FrameState::TStrings(strings) => {
                let Some((string, rest)) = strings.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::TStrings(rest);
                ExpressionSearchStep::Enter(Self {
                    state: FrameState::InterpolatedElements(&string.elements),
                })
            }
            FrameState::InterpolatedElements(elements) => {
                let Some((element, rest)) = elements.split_first() else {
                    return ExpressionSearchStep::Done;
                };
                self.state = FrameState::InterpolatedElements(rest);
                match element {
                    InterpolatedStringElement::Literal(_) => ExpressionSearchStep::Progress,
                    InterpolatedStringElement::Interpolation(_) => {
                        ExpressionSearchStep::Enter(Self::interpolated_element(element))
                    }
                }
            }
            FrameState::InterpolatedElement(element, phase) => match (element, phase) {
                (InterpolatedStringElement::Literal(_), Phase::First) => {
                    ExpressionSearchStep::Progress
                }
                (InterpolatedStringElement::Interpolation(interpolation), Phase::First) => {
                    self.state = FrameState::InterpolatedElement(element, Phase::Second);
                    ExpressionSearchStep::Enter(Self::expression(&interpolation.expression))
                }
                (InterpolatedStringElement::Interpolation(interpolation), Phase::Second) => {
                    interpolation.format_spec.as_ref().map_or(
                        ExpressionSearchStep::Progress,
                        |spec| {
                            ExpressionSearchStep::Enter(Self {
                                state: FrameState::InterpolatedElements(&spec.elements),
                            })
                        },
                    )
                }
                (_, _) => ExpressionSearchStep::Done,
            },
            FrameState::Done => ExpressionSearchStep::Done,
        }
    }

    fn optional_expression(expression: Option<&'ast Expr>) -> ExpressionSearchStep<'ast> {
        expression.map_or(ExpressionSearchStep::Progress, |expression| {
            ExpressionSearchStep::Enter(Self::expression(expression))
        })
    }

    fn fstring(value: &'ast FStringValue) -> Self {
        let state = match value.iter() {
            FStringParts::Single(mut string) => string.next().map_or(FrameState::Done, |string| {
                FrameState::InterpolatedElements(&string.elements)
            }),
            FStringParts::Concatenated(parts) => FrameState::FStringParts(parts.as_slice()),
        };
        Self { state }
    }

    fn expression_children(expression: &'ast Expr, phase: Phase) -> ExpressionSearchStep<'ast> {
        let child = match expression {
            Expr::BoolOp(ast::ExprBoolOp { values, .. }) => match phase {
                Phase::First => Self::expressions(values),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::FString(ast::ExprFString { value, .. }) => match phase {
                Phase::First => Self::fstring(value),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::TString(ast::ExprTString { value, .. }) => match phase {
                Phase::First => Self {
                    state: FrameState::TStrings(value.as_slice()),
                },
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Named(ast::ExprNamed { target, value, .. }) => match phase {
                Phase::First => Self::expression(target),
                Phase::Second => Self::expression(value),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::BinOp(ast::ExprBinOp { left, right, .. }) => match phase {
                Phase::First => Self::expression(left),
                Phase::Second => Self::expression(right),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => match phase {
                Phase::First => Self::expression(operand),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Lambda(ast::ExprLambda {
                body, parameters, ..
            }) => match phase {
                Phase::First => {
                    let Some(parameters) = parameters else {
                        return ExpressionSearchStep::Progress;
                    };
                    Self {
                        state: FrameState::ParameterGroups(parameters, Phase::First),
                    }
                }
                Phase::Second => Self::expression(body),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => match phase {
                Phase::First => Self::expression(test),
                Phase::Second => Self::expression(body),
                Phase::Third => Self::expression(orelse),
                Phase::Done => return ExpressionSearchStep::Done,
            },
            Expr::Dict(ast::ExprDict { items, .. }) => match phase {
                Phase::First => Self {
                    state: FrameState::DictItems(items, false),
                },
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Set(ast::ExprSet { elts, .. })
            | Expr::List(ast::ExprList { elts, .. })
            | Expr::Tuple(ast::ExprTuple { elts, .. }) => match phase {
                Phase::First => Self::expressions(elts),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::ListComp(ast::ExprListComp {
                elt, generators, ..
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt, generators, ..
            })
            | Expr::Generator(ast::ExprGenerator {
                elt, generators, ..
            }) => match phase {
                Phase::First => Self::expression(elt),
                Phase::Second => Self {
                    state: FrameState::Comprehensions(generators),
                },
                _ => return ExpressionSearchStep::Done,
            },
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => match phase {
                Phase::First => return Self::optional_expression(key.as_deref()),
                Phase::Second => Self::expression(value),
                Phase::Third => Self {
                    state: FrameState::Comprehensions(generators),
                },
                Phase::Done => return ExpressionSearchStep::Done,
            },
            Expr::Await(ast::ExprAwait { value, .. })
            | Expr::YieldFrom(ast::ExprYieldFrom { value, .. })
            | Expr::Attribute(ast::ExprAttribute { value, .. })
            | Expr::Starred(ast::ExprStarred { value, .. }) => match phase {
                Phase::First => Self::expression(value),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Yield(ast::ExprYield { value, .. }) => match phase {
                Phase::First => return Self::optional_expression(value.as_deref()),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Compare(ast::ExprCompare { operands, .. }) => match phase {
                Phase::First => Self::expressions(operands),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                // Note that this is the evaluation order but not necessarily the declaration order
                // (e.g. for `f(*args, a=2, *args2, **kwargs)` it's not)
                match phase {
                    Phase::First => Self::expression(func),
                    Phase::Second => Self::expressions(&arguments.args),
                    Phase::Third => Self {
                        state: FrameState::Keywords(&arguments.keywords),
                    },
                    Phase::Done => return ExpressionSearchStep::Done,
                }
            }
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => match phase {
                Phase::First => Self::expression(value),
                Phase::Second => Self::expression(slice),
                _ => return ExpressionSearchStep::Done,
            },
            Expr::Slice(ast::ExprSlice {
                lower, upper, step, ..
            }) => {
                return match phase {
                    Phase::First => Self::optional_expression(lower.as_deref()),
                    Phase::Second => Self::optional_expression(upper.as_deref()),
                    Phase::Third => Self::optional_expression(step.as_deref()),
                    Phase::Done => ExpressionSearchStep::Done,
                };
            }
            Expr::Name(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::IpyEscapeCommand(_) => return ExpressionSearchStep::Done,
        };
        ExpressionSearchStep::Enter(child)
    }
}
