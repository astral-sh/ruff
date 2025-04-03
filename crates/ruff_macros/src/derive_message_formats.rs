use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::token::{Dot, Paren};
use syn::{Block, Expr, ExprLit, ExprMethodCall, ItemFn, Lit, Stmt};

pub(crate) fn derive_message_formats(func: &ItemFn) -> TokenStream {
    let mut strings = quote!();

    if let Err(err) = parse_block(&func.block, &mut strings) {
        return err;
    }

    quote! {
        #func
        fn message_formats() -> &'static [&'static str] {
            &[#strings]
        }
    }
}

fn parse_block(block: &Block, strings: &mut TokenStream) -> Result<(), TokenStream> {
    let Some(Stmt::Expr(last, _)) = block.stmts.last() else {
        panic!("expected last statement in block to be an expression")
    };
    parse_expr(last, strings)?;
    Ok(())
}

fn parse_expr(expr: &Expr, strings: &mut TokenStream) -> Result<(), TokenStream> {
    match expr {
        Expr::Macro(mac) if mac.mac.path.is_ident("format") => {
            let mut tokens = mac.mac.tokens.to_token_stream().into_iter();
            let Some(first_token) = tokens.next() else {
                return Err(
                    quote_spanned!(expr.span() => compile_error!("expected `format!` to have an argument")),
                );
            };
            // do not throw an error if the `format!` argument contains a formatting argument

            if !first_token.to_string().contains('{') {
                // comma and string
                if tokens.next().is_none() || tokens.next().is_none() {
                    return Err(
                        quote_spanned!(expr.span() => compile_error!("prefer `String::to_string` over `format!` without arguments")),
                    );
                }
            }
            strings.extend(quote! {#first_token,});
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
                    return Err(
                        quote_spanned!(expr.span() => compile_error!("expected `String::to_string` method on str literal")),
                    );
                };

                let str_token = literal_string.token();

                strings.extend(quote! {#str_token,});
                Ok(())
            }
            _ => Err(
                quote_spanned!(expr.span() => compile_error!("expected `String::to_string` method on str literal")),
            ),
        },
        Expr::Match(block) => {
            for arm in &block.arms {
                parse_expr(&arm.body, strings)?;
            }
            Ok(())
        }
        _ => Err(quote_spanned!(
            expr.span() =>
            compile_error!("expected last expression to be a `format!` macro, a static String or a match block")
        )),
    }
}
