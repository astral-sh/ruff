use ruff_formatter::prelude::*;
use ruff_formatter::{format_args, write, Format};

use crate::context::ASTFormatContext;
use crate::cst::Arguments;
use crate::shared_traits::AsFormat;

pub struct FormatArguments<'a> {
    item: &'a Arguments,
}

impl AsFormat<ASTFormatContext<'_>> for Arguments {
    type Format<'a> = FormatArguments<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatArguments { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatArguments<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let args = self.item;

        let mut first = true;

        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            if !std::mem::take(&mut first) {
                write!(f, [text(",")])?;
                write!(f, [soft_line_break_or_space()])?;
            }

            write!(
                f,
                [group(&format_args![format_with(|f| {
                    write!(f, [arg.format()])?;
                    if let Some(i) = i.checked_sub(defaults_start) {
                        if arg.annotation.is_some() {
                            write!(f, [space()])?;
                            write!(f, [text("=")])?;
                            write!(f, [space()])?;
                        } else {
                            write!(f, [text("=")])?;
                        }
                        write!(f, [args.defaults[i].format()])?;
                    }
                    Ok(())
                })])]
            )?;

            if i + 1 == args.posonlyargs.len() {
                if !std::mem::take(&mut first) {
                    write!(f, [text(",")])?;
                    write!(f, [soft_line_break_or_space()])?;
                }
                write!(f, [text("/")])?;
            }
        }

        if let Some(vararg) = &args.vararg {
            if !std::mem::take(&mut first) {
                write!(f, [text(",")])?;
                write!(f, [soft_line_break_or_space()])?;
            }
            first = false;

            write!(f, [text("*")])?;
            write!(f, [vararg.format()])?;
        } else if !args.kwonlyargs.is_empty() {
            if !std::mem::take(&mut first) {
                write!(f, [text(",")])?;
                write!(f, [soft_line_break_or_space()])?;
            }
            first = false;

            write!(f, [text("*")])?;
        }

        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            if !std::mem::take(&mut first) {
                write!(f, [text(",")])?;
                write!(f, [soft_line_break_or_space()])?;
            }

            write!(
                f,
                [group(&format_args![format_with(|f| {
                    write!(f, [kwarg.format()])?;
                    if let Some(default) = i
                        .checked_sub(defaults_start)
                        .and_then(|i| args.kw_defaults.get(i))
                    {
                        if kwarg.annotation.is_some() {
                            write!(f, [space()])?;
                            write!(f, [text("=")])?;
                            write!(f, [space()])?;
                        } else {
                            write!(f, [text("=")])?;
                        }
                        write!(f, [default.format()])?;
                    }
                    Ok(())
                })])]
            )?;
        }
        if let Some(kwarg) = &args.kwarg {
            if !std::mem::take(&mut first) {
                write!(f, [text(",")])?;
                write!(f, [soft_line_break_or_space()])?;
            }

            write!(f, [text("**")])?;
            write!(f, [kwarg.format()])?;
        }

        if !first {
            write!(f, [if_group_breaks(&text(","))])?;
        }

        Ok(())
    }
}
