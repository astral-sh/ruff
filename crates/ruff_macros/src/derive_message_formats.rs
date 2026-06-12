use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::token::{Dot, Paren};
use syn::{Block, Error, Expr, ExprLit, ExprMethodCall, ItemFn, Lit, LitStr, Stmt, Token};

pub(crate) fn derive_message_formats(func: &ItemFn) -> syn::Result<TokenStream> {
    let mut strings = quote!();

    parse_block(&func.block, &mut strings)?;

    Ok(quote! {
        #func
        fn message_formats() -> &'static [&'static str] {
            &[#strings]
        }
    })
}

fn parse_block(block: &Block, strings: &mut TokenStream) -> syn::Result<()> {
    let Some(Stmt::Expr(last, _)) = block.stmts.last() else {
        return Err(Error::new_spanned(
            block,
            "expected block to end in an expression",
        ));
    };
    parse_expr(last, strings)?;
    Ok(())
}

fn parse_expr(expr: &Expr, strings: &mut TokenStream) -> syn::Result<()> {
    match expr {
        Expr::Macro(mac) if mac.mac.path.is_ident("format") => {
            let arguments = mac.mac.parse_body::<FormatArguments>()?;
            // do not throw an error if the `format!` argument contains a formatting argument

            if !arguments.format.value().contains('{') && !arguments.has_arguments {
                return Err(Error::new(
                    expr.span(),
                    "prefer `String::to_string` over `format!` without arguments",
                ));
            }
            let format = arguments.format;
            strings.extend(quote! {#format,});
            Ok(())
        }
        Expr::Block(block) => parse_block(&block.block, strings),
        Expr::If(expr) => {
            parse_block(&expr.then_branch, strings)?;
            if let Some((_, then)) = &expr.else_branch {
                parse_expr(then, strings)?;
            }
            Ok(())
        }
        Expr::MethodCall(method_call) => match method_call {
            ExprMethodCall {
                method,
                receiver,
                attrs,
                dot_token,
                turbofish: None,
                paren_token,
                args,
            } if *method == *"to_string"
                && attrs.is_empty()
                && args.is_empty()
                && *paren_token == Paren::default()
                && *dot_token == Dot::default() =>
            {
                let Expr::Lit(ExprLit {
                    lit: Lit::Str(ref literal_string),
                    ..
                }) = **receiver
                else {
                    return Err(Error::new(
                        expr.span(),
                        "expected `String::to_string` method on str literal",
                    ));
                };

                let str_token = literal_string.token();

                strings.extend(quote! {#str_token,});
                Ok(())
            }
            _ => Err(Error::new(
                expr.span(),
                "expected `String::to_string` method on str literal",
            )),
        },
        Expr::Match(block) => {
            for arm in &block.arms {
                parse_expr(&arm.body, strings)?;
            }
            Ok(())
        }
        _ => Err(Error::new(
            expr.span(),
            "expected last expression to be a `format!` macro, a static String or a match block",
        )),
    }
}

struct FormatArguments {
    format: LitStr,
    has_arguments: bool,
}

impl Parse for FormatArguments {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let format = input.parse()?;
        let has_arguments = if input.is_empty() {
            false
        } else {
            input.parse::<Token![,]>()?;
            let has_arguments = !input.is_empty();
            input.parse::<TokenStream>()?;
            has_arguments
        };

        Ok(Self {
            format,
            has_arguments,
        })
    }
}
