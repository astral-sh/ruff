use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{Block, Expr, ItemFn, Stmt};

pub fn derive_message_formats(func: &ItemFn) -> proc_macro2::TokenStream {
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
    let Some(Stmt::Expr(last)) = block.stmts.last() else {panic!("expected last statement in block to be an expression")};
    parse_expr(last, strings)?;
    Ok(())
}

fn parse_expr(expr: &Expr, strings: &mut TokenStream) -> Result<(), TokenStream> {
    match expr {
        Expr::Macro(mac) if mac.mac.path.is_ident("format") => {
            let Some(first_token) = mac.mac.tokens.to_token_stream().into_iter().next() else {
                return Err(quote_spanned!(expr.span() => compile_error!("expected format! to have an argument")))
            };
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
        Expr::Match(block) => {
            for arm in &block.arms {
                parse_expr(&arm.body, strings)?;
            }
            Ok(())
        }
        _ => Err(quote_spanned!(
            expr.span() =>
            compile_error!("expected last expression to be a format! macro or a match block")
        )),
    }
}
