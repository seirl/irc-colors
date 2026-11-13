#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, LitStr, Token};

#[proc_macro]
pub fn __irc_format_impl(input: TokenStream) -> TokenStream {
    let invocation = syn::parse_macro_input!(input as Invocation);
    expand(&invocation.krate, invocation.args).into()
}

/// Outer wrapper syntax passed by `macro_rules! irc_format`: `$crate; <user args>`.
struct Invocation {
    krate: TokenStream2,
    args: TokenStream2,
}

impl Parse for Invocation {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut krate = TokenStream2::new();
        while !input.is_empty() && !input.peek(Token![;]) {
            krate.extend([input.parse::<TokenTree>()?]);
        }
        input.parse::<Token![;]>()?;
        Ok(Self {
            krate,
            args: input.parse()?,
        })
    }
}

/// Parsed user arguments: `irc_format!("...", pos0, pos1, name = val)`.
struct FormatCall {
    fmt_lit: LitStr,
    positional: Vec<Expr>,
    named: Vec<NamedArg>,
}

struct NamedArg {
    ident: Ident,
    name: String,
    expr: Expr,
}

enum CallArg {
    Positional(Expr),
    Named(NamedArg),
}

impl Parse for CallArg {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let fork = input.fork();
        let is_named = fork.call(Ident::parse_any).is_ok()
            && !fork.peek(Token![==])
            && !fork.peek(Token![=>])
            && fork.peek(Token![=]);

        if is_named {
            let ident = input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;
            let expr: Expr = input.parse()?;
            Ok(Self::Named(NamedArg {
                name: ident.unraw().to_string(),
                ident,
                expr,
            }))
        } else {
            Ok(Self::Positional(input.parse()?))
        }
    }
}

impl Parse for FormatCall {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let fmt_lit: LitStr = input.parse()?;
        let mut positional = Vec::new();
        let mut named: Vec<NamedArg> = Vec::new();

        if input.parse::<Option<Token![,]>>()?.is_some() {
            for arg in Punctuated::<CallArg, Token![,]>::parse_terminated(input)? {
                match arg {
                    CallArg::Positional(expr) if named.is_empty() => positional.push(expr),
                    CallArg::Positional(_) => {
                        return Err(
                            input.error("positional arguments cannot follow named arguments")
                        );
                    }
                    CallArg::Named(arg) if named.iter().any(|prev| prev.name == arg.name) => {
                        return Err(syn::Error::new(arg.ident.span(), "duplicate argument"));
                    }
                    CallArg::Named(arg) => named.push(arg),
                }
            }
        }

        Ok(Self {
            fmt_lit,
            positional,
            named,
        })
    }
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
enum Piece<'a> {
    Literal(&'a str),
    Placeholder { arg: &'a str, spec: &'a str },
}

fn parse_pieces(mut s: &str) -> Result<Vec<Piece<'_>>, ()> {
    let mut pieces = Vec::new();
    while !s.is_empty() {
        if let Some(rest) = s.strip_prefix("{{") {
            pieces.push(Piece::Literal(&s[..1]));
            s = rest;
        } else if let Some(rest) = s.strip_prefix("}}") {
            pieces.push(Piece::Literal(&s[..1]));
            s = rest;
        } else if let Some(rest) = s.strip_prefix('{') {
            let (inside, after) = rest.split_once('}').ok_or(())?;
            let (arg, spec) = inside.split_once(':').unwrap_or((inside, ""));
            pieces.push(Piece::Placeholder { arg, spec });
            s = after;
        } else if s.starts_with('}') {
            return Err(());
        } else {
            let end = s.find(['{', '}']).unwrap_or(s.len());
            pieces.push(Piece::Literal(&s[..end]));
            s = &s[end..];
        }
    }
    Ok(pieces)
}

fn has_dynamic_count(spec: &str) -> bool {
    let mut chars = spec.chars();
    let rest = match (chars.next(), chars.next()) {
        (Some(_), Some('<' | '^' | '>')) => chars.as_str(),
        _ => spec,
    };
    rest.contains('$') || rest.contains(".*")
}

struct ResolvedArg {
    place: TokenStream2,
    reference: TokenStream2,
}

fn resolve_arg(
    raw: &str,
    next_pos: &mut usize,
    fmt_span: Span,
    pos_vars: &[Ident],
    named_args: &[NamedArg],
    named_vars: &[Ident],
) -> Result<ResolvedArg, ()> {
    if raw.is_empty() {
        let var = pos_vars.get(*next_pos).ok_or(())?;
        *next_pos += 1;
        return Ok(ResolvedArg {
            place: quote!(*#var),
            reference: quote!(#var),
        });
    }
    if raw.bytes().all(|b| b.is_ascii_digit()) {
        let idx: usize = raw.parse().map_err(|_| ())?;
        let var = pos_vars.get(idx).ok_or(())?;
        return Ok(ResolvedArg {
            place: quote!(*#var),
            reference: quote!(#var),
        });
    }
    if raw.starts_with("r#") {
        return Err(());
    }
    if let Some(idx) = named_args.iter().position(|a| a.name == raw) {
        let var = &named_vars[idx];
        return Ok(ResolvedArg {
            place: quote!(*#var),
            reference: quote!(#var),
        });
    }

    let mut ident: Ident = syn::parse_str(raw)
        .or_else(|_| syn::parse_str(&format!("r#{raw}")))
        .map_err(|_| ())?;
    ident.set_span(fmt_span);
    Ok(ResolvedArg {
        place: quote!(#ident),
        reference: quote!(&#ident),
    })
}

fn expand(krate: &TokenStream2, raw_args: TokenStream2) -> TokenStream2 {
    let Ok(call) = syn::parse2::<FormatCall>(raw_args.clone()) else {
        return fallback_to_rustc(krate, &raw_args);
    };

    let fmt_value = call.fmt_lit.value();
    let Ok(pieces) = parse_pieces(&fmt_value) else {
        return fallback_to_rustc(krate, &raw_args);
    };

    let pos_vars: Vec<Ident> = (0..call.positional.len())
        .map(|i| format_ident!("__irc_pos_{i}"))
        .collect();
    let named_vars: Vec<Ident> = (0..call.named.len())
        .map(|i| format_ident!("__irc_named_{i}"))
        .collect();

    let mut stmts = Vec::new();
    let mut literal_buf = String::new();
    let mut next_pos = 0usize;

    for piece in pieces {
        match piece {
            Piece::Literal(s) => {
                literal_buf.extend(s.chars().filter(|&c| !is_irc_control_char(c)));
            }
            Piece::Placeholder { arg, spec } => {
                if has_dynamic_count(spec) {
                    return syn::Error::new(
                        call.fmt_lit.span(),
                        "irc_format! does not support dynamic width/precision (`$`, `.*`)",
                    )
                    .into_compile_error();
                }
                if !literal_buf.is_empty() {
                    let text = std::mem::take(&mut literal_buf);
                    stmts.push(quote! {
                        #krate::__private::push_clean_str(&mut __irc_text, #text);
                    });
                }
                let Ok(ResolvedArg { place, reference }) = resolve_arg(
                    arg,
                    &mut next_pos,
                    call.fmt_lit.span(),
                    &pos_vars,
                    &call.named,
                    &named_vars,
                ) else {
                    return fallback_to_rustc(krate, &raw_args);
                };
                if spec.is_empty() {
                    stmts.push(quote! {
                        #krate::__private::Wrap(#reference).__write_plain(&mut __irc_text);
                    });
                } else {
                    let mini_fmt = format!("{{0:{spec}}}");
                    stmts.push(quote! {
                        let _ = ::core::write!(
                            &mut #krate::__private::SanitizingWriter(&mut __irc_text),
                            #mini_fmt,
                            #place
                        );
                    });
                }
            }
        }
    }
    if !literal_buf.is_empty() {
        stmts.push(quote! {
            #krate::__private::push_clean_str(&mut __irc_text, #literal_buf);
        });
    }

    let fmt_lit = &call.fmt_lit;
    let pos_exprs = &call.positional;
    let named_idents: Vec<&Ident> = call.named.iter().map(|a| &a.ident).collect();
    let named_exprs: Vec<&Expr> = call.named.iter().map(|a| &a.expr).collect();

    quote! {
        match (#(&(#pos_exprs),)* #(&(#named_exprs),)*) {
            (#(#pos_vars,)* #(#named_vars,)*) => {
                if false {
                    let _ = ::core::format_args!(
                        #fmt_lit
                        #(, *#pos_vars)*
                        #(, #named_idents = *#named_vars)*
                    );
                }
                use #krate::__private::FallbackDisplay as _;
                let mut __irc_text = #krate::Text::new();
                #(#stmts)*
                __irc_text
            }
        }
    }
}

/// Delegates invalid invocations to `core::format_args!` so `rustc` emits its
/// native format-string diagnostics while still type-checking as `Text`.
fn fallback_to_rustc(krate: &TokenStream2, raw_args: &TokenStream2) -> TokenStream2 {
    quote! {{
        let _ = ::core::format_args!(#raw_args);
        if false {
            #krate::Text::new()
        } else {
            ::core::unreachable!("irc_format! internal parser mismatch")
        }
    }}
}

const fn is_irc_control_char(c: char) -> bool {
    matches!(
        c,
        '\x00'
            | '\x01'
            | '\x02'
            | '\x03'
            | '\x04'
            | '\x0f'
            | '\x11'
            | '\x16'
            | '\x1d'
            | '\x1e'
            | '\x1f'
    )
}

#[cfg(test)]
mod lib_test;
