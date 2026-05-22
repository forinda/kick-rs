//! # kick-rs-macros
//!
//! Opt-in proc-macro sugar for kick-rs. Every macro here expands to a
//! call you could write yourself against [`kick_rs_core`].
//!
//! Currently shipping:
//!
//! - [`service`] — derive [`ServiceImpl`](https://docs.rs/kick-rs-core/latest/kick_rs_core/trait.ServiceImpl.html)
//!   for a struct whose fields are all `Inject<T>` or `Arc<T>`. The macro
//!   rewrites the fields to `Arc<T>` and emits a `build()` that resolves
//!   each from a [`Container`](https://docs.rs/kick-rs-core/latest/kick_rs_core/struct.Container.html).
//!
//! Reserved (no-op pass-through for now, to be filled in later phases):
//!
//! - `handler` — opt-in marker for axum handlers; reserved for future
//!   codegen integration.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

/// Annotate a struct to auto-derive
/// [`ServiceImpl`](https://docs.rs/kick-rs-core/latest/kick_rs_core/trait.ServiceImpl.html).
///
/// Field rules:
///
/// - `Inject<X>` — rewritten in the output struct to `Arc<X>`; the
///   generated `build` resolves `X` from the container.
/// - `Arc<X>` — left as-is; the generated `build` resolves `X` from
///   the container.
/// - Anything else — compile error. Structs with non-DI fields should
///   use `.service_value(value)` directly instead of this macro.
///
/// Unit structs (`struct Foo;`) are supported and expand to
/// `Arc::new(Foo)`.
///
/// Tuple structs (`struct Foo(Bar);`) are rejected — name the fields
/// or use a manual `ServiceImpl` impl.
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let kick_path = resolve_kick_path();
    let service_impl_path = quote! { #kick_path::ServiceImpl };
    let container_path = quote! { #kick_path::Container };

    let data = match &input.data {
        Data::Struct(d) => d,
        _ => {
            return syn::Error::new_spanned(&input, "`#[service]` only supports structs")
                .to_compile_error()
                .into();
        }
    };

    // Build the constructor body + rewritten field types.
    let (constructor_body, rewritten_fields) = match &data.fields {
        Fields::Named(fields) => {
            let mut inits = Vec::with_capacity(fields.named.len());
            let mut rewritten = Vec::with_capacity(fields.named.len());
            for f in &fields.named {
                let name = f.ident.as_ref().expect("named field");
                let Some(inner) = extract_dep_type(&f.ty) else {
                    return syn::Error::new_spanned(
                        &f.ty,
                        "`#[service]` fields must be `Inject<T>` or `Arc<T>`. \
                         For non-DI fields use `.service_value(value)` directly.",
                    )
                    .to_compile_error()
                    .into();
                };
                inits.push(quote! {
                    #name: c.resolve::<#inner>()
                });
                let vis = &f.vis;
                let attrs = &f.attrs;
                rewritten.push(quote! {
                    #(#attrs)* #vis #name: ::std::sync::Arc<#inner>
                });
            }
            (quote! { Self { #(#inits),* } }, Some(rewritten))
        }
        Fields::Unit => (quote! { Self }, None),
        Fields::Unnamed(_) => {
            return syn::Error::new_spanned(
                &input,
                "`#[service]` does not support tuple structs; use named fields or hand-write \
                 a `ServiceImpl` impl.",
            )
            .to_compile_error()
            .into();
        }
    };

    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;

    let rewritten_struct: TokenStream2 = match rewritten_fields {
        Some(fields) => quote! {
            #(#attrs)*
            #vis struct #ident #generics {
                #(#fields),*
            }
        },
        None => quote! {
            #(#attrs)*
            #vis struct #ident #generics;
        },
    };

    let expanded = quote! {
        #rewritten_struct

        #[automatically_derived]
        impl #impl_generics #service_impl_path for #ident #ty_generics #where_clause {
            fn build(c: &#container_path) -> ::std::sync::Arc<Self> {
                ::std::sync::Arc::new(#constructor_body)
            }
        }
    };

    expanded.into()
}

/// Resolve the absolute path prefix exposing [`ServiceImpl`] and
/// [`Container`] in the caller's crate. Prefers the umbrella crate
/// (`kick-rs`) when present; falls back to `kick-rs-core` for adopters
/// who depend on core directly without the umbrella.
fn resolve_kick_path() -> TokenStream2 {
    if let Ok(found) = crate_name("kick-rs") {
        return path_for(found);
    }
    if let Ok(found) = crate_name("kick-rs-core") {
        return path_for(found);
    }
    // Best-effort fallback — emit a path that gives the user a clear
    // `cannot find crate` error pointing them at the missing dep.
    quote! { ::kick_rs_core }
}

fn path_for(found: FoundCrate) -> TokenStream2 {
    match found {
        FoundCrate::Itself => quote! { crate },
        FoundCrate::Name(name) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
    }
}

/// `#[handler]` — placeholder for future codegen.
///
/// Pass-through for now so the attribute name is reserved. A plain
/// `async fn` with axum extractors already works as a handler — no
/// macro needed today. The marker will gain behavior in a later phase
/// (e.g., emitting a route registry entry the CLI can read).
#[proc_macro_attribute]
pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Detect `Inject<T>` or `Arc<T>` in a field type and return the inner `T`.
fn extract_dep_type(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else {
        return None;
    };
    let last = p.path.segments.last()?;
    if last.ident != "Inject" && last.ident != "Arc" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    })
}
