use rustpython_parser::ast::Constant;

use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::{Pattern, PatternKind};
use crate::shared_traits::AsFormat;

pub struct FormatPattern<'a> {
    item: &'a Pattern,
}

impl AsFormat<ASTFormatContext> for Pattern {
    type Format<'a> = FormatPattern<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatPattern { item: self }
    }
}

impl Format<ASTFormatContext> for FormatPattern<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let pattern = self.item;

        match &pattern.node {
            PatternKind::MatchValue { value } => {
                write!(f, [value.format()])?;
            }
            PatternKind::MatchSingleton { value } => match value {
                Constant::None => write!(f, [text("None")])?,
                Constant::Bool(value) => {
                    if *value {
                        write!(f, [text("True")])?;
                    } else {
                        write!(f, [text("False")])?;
                    }
                }
                _ => unreachable!("singleton pattern must be None or bool"),
            },
            PatternKind::MatchSequence { patterns } => {
                write!(f, [text("[")])?;
                if let Some(pattern) = patterns.first() {
                    write!(f, [pattern.format()])?;
                }
                for pattern in patterns.iter().skip(1) {
                    write!(f, [text(","), space(), pattern.format()])?;
                }
                write!(f, [text("]")])?;
            }
            PatternKind::MatchMapping {
                keys,
                patterns,
                rest,
            } => {
                write!(f, [text("{")])?;
                if let Some(pattern) = patterns.first() {
                    write!(f, [keys[0].format(), text(":"), space(), pattern.format()])?;
                }
                for (key, pattern) in keys.iter().skip(1).zip(patterns.iter().skip(1)) {
                    write!(
                        f,
                        [
                            text(","),
                            space(),
                            key.format(),
                            text(":"),
                            space(),
                            pattern.format()
                        ]
                    )?;
                }
                if let Some(rest) = &rest {
                    write!(
                        f,
                        [
                            text(","),
                            space(),
                            text("**"),
                            space(),
                            dynamic_text(rest, None)
                        ]
                    )?;
                }
                write!(f, [text("}")])?;
            }
            PatternKind::MatchClass {
                cls,
                patterns,
                kwd_attrs,
                kwd_patterns,
            } => {
                write!(f, [cls.format()])?;
                if !patterns.is_empty() {
                    write!(f, [text("(")])?;
                    if let Some(pattern) = patterns.first() {
                        write!(f, [pattern.format()])?;
                    }
                    for pattern in patterns.iter().skip(1) {
                        write!(f, [text(","), space(), pattern.format()])?;
                    }
                    write!(f, [text(")")])?;
                }
                if !kwd_attrs.is_empty() {
                    write!(f, [text("(")])?;
                    if let Some(attr) = kwd_attrs.first() {
                        write!(f, [dynamic_text(attr, None)])?;
                    }
                    for attr in kwd_attrs.iter().skip(1) {
                        write!(f, [text(","), space(), dynamic_text(attr, None)])?;
                    }
                    write!(f, [text(")")])?;
                }
                if !kwd_patterns.is_empty() {
                    write!(f, [text("(")])?;
                    if let Some(pattern) = kwd_patterns.first() {
                        write!(f, [pattern.format()])?;
                    }
                    for pattern in kwd_patterns.iter().skip(1) {
                        write!(f, [text(","), space(), pattern.format()])?;
                    }
                    write!(f, [text(")")])?;
                }
            }
            PatternKind::MatchStar { name } => {
                if let Some(name) = name {
                    write!(f, [text("*"), dynamic_text(name, None)])?;
                } else {
                    write!(f, [text("*_")])?;
                }
            }
            PatternKind::MatchAs { pattern, name } => {
                if let Some(pattern) = &pattern {
                    write!(f, [pattern.format()])?;
                    write!(f, [space()])?;
                    write!(f, [text("as")])?;
                    write!(f, [space()])?;
                }
                if let Some(name) = name {
                    write!(f, [dynamic_text(name, None)])?;
                } else {
                    write!(f, [text("_")])?;
                }
            }
            PatternKind::MatchOr { patterns } => {
                write!(f, [patterns[0].format()])?;
                for pattern in patterns.iter().skip(1) {
                    write!(f, [space(), text("|"), space(), pattern.format()])?;
                }
            }
        }

        Ok(())
    }
}
