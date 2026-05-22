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
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, FnArg, GenericArgument, ItemFn, Pat,
    PathArguments, ReturnType, Type, TypeReference,
};

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

/// Annotate an `async fn` to auto-derive
/// [`ContextContributor`](https://docs.rs/kick-rs-core/latest/kick_rs_core/trait.ContextContributor.html)
/// on a unit struct of the same name.
///
/// Function-style sugar over the trait impl:
///
/// ```ignore
/// #[contributor]
/// async fn LoadTenant() -> KickResult<Tenant> {
///     Ok(Tenant { id: 42 })
/// }
///
/// #[contributor]
/// async fn LoadProject(tenant: &Tenant) -> KickResult<Project> {
///     Ok(Project { tenant_id: tenant.id })
/// }
///
/// #[contributor]
/// async fn LoadTenantDb(
///     ctx: &dyn ContributorRequest,
///     tenant: &Tenant,
/// ) -> KickResult<TenantDb> {
///     let cfg = ctx.inject::<TenantConfig>();
///     Ok(TenantDb::for_tenant(&tenant.slug, cfg.pool_size).await?)
/// }
/// ```
///
/// Rules:
/// - Must be an `async fn`.
/// - Return type must be `KickResult<KeyType>` (the inner type becomes
///   the `Key` associated type).
/// - First parameter may optionally be named `ctx` with type
///   `&dyn ContributorRequest`; that lets you call `ctx.inject::<T>()`
///   inside the body.
/// - Remaining parameters of the form `name: &T` become the `Deps`
///   tuple in declaration order.
/// - Anything else is a compile error.
///
/// The function name is used verbatim as the generated struct name —
/// stick to PascalCase (`LoadTenant`, not `load_tenant`) for idiomatic
/// Rust. The function visibility (`pub`, `pub(crate)`, default) carries
/// through to the struct.
///
/// Stateful contributors (those holding fields) still need the manual
/// `impl ContextContributor` form — this macro only covers the
/// unit-struct case.
#[proc_macro_attribute]
pub fn contributor(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // Must be async.
    if input.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            input.sig.fn_token,
            "`#[contributor]` requires an `async fn`",
        )
        .to_compile_error()
        .into();
    }
    // No generics/where for now — keeps things tractable.
    if !input.sig.generics.params.is_empty() || input.sig.generics.where_clause.is_some() {
        return syn::Error::new_spanned(
            input.sig.generics.clone(),
            "`#[contributor]` does not yet support generic functions",
        )
        .to_compile_error()
        .into();
    }

    // Return type must be KickResult<Key>.
    let key_type = match extract_kick_result_inner(&input.sig.output) {
        Ok(ty) => ty,
        Err(e) => return e.to_compile_error().into(),
    };

    // Parse params: optional ctx + zero-or-more `&T` deps.
    let (ctx_pat, dep_idents, dep_inner_types) = match parse_contributor_params(&input.sig.inputs) {
        Ok(triple) => triple,
        Err(e) => return e.to_compile_error().into(),
    };

    let kick_path = resolve_kick_path();
    let trait_path = quote! { #kick_path::ContextContributor };
    let req_path = quote! { #kick_path::ContributorRequest };
    let deps_path = quote! { #kick_path::ContributorDeps };
    let result_path = quote! { #kick_path::KickResult };

    let struct_name = &input.sig.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let body = &input.block;

    let deps_tuple_ty: TokenStream2 = if dep_inner_types.is_empty() {
        quote! { () }
    } else {
        quote! { ( #( #dep_inner_types, )* ) }
    };
    let deps_pat: TokenStream2 = if dep_idents.is_empty() {
        quote! { _ }
    } else {
        quote! { ( #( #dep_idents, )* ) }
    };

    let expanded = quote! {
        #(#attrs)*
        #vis struct #struct_name;

        #[automatically_derived]
        impl #trait_path for #struct_name {
            type Key = #key_type;
            type Deps = #deps_tuple_ty;

            async fn resolve<'a>(
                &'a self,
                #ctx_pat: &'a (dyn #req_path + 'a),
                #deps_pat: <Self::Deps as #deps_path>::Resolved<'a>,
            ) -> #result_path<Self::Key> #body
        }
    };

    expanded.into()
}

/// Extract `T` from a return type of the form `KickResult<T>`.
fn extract_kick_result_inner(rt: &ReturnType) -> Result<Type, syn::Error> {
    let ty = match rt {
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                rt,
                "`#[contributor]` requires a return type of `KickResult<Key>`",
            ));
        }
        ReturnType::Type(_, ty) => ty.as_ref(),
    };

    let Type::Path(p) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "`#[contributor]` return must be `KickResult<Key>`",
        ));
    };
    let last = p
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new_spanned(ty, "empty return type path"))?;
    if last.ident != "KickResult" {
        return Err(syn::Error::new_spanned(
            ty,
            "`#[contributor]` return must be `KickResult<Key>`",
        ));
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "`KickResult` requires one type argument",
        ));
    };
    let arg = args
        .args
        .iter()
        .find_map(|a| match a {
            GenericArgument::Type(t) => Some(t.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            syn::Error::new_spanned(ty, "`KickResult<...>` needs a concrete type argument")
        })?;
    Ok(arg)
}

/// Parse the function args.
///
/// Returns:
/// - `ctx_pat`: the binding pattern to use for the `ctx` parameter
///   (`ctx` if the user took it, `_ctx` otherwise).
/// - `dep_idents`: the names of the dep parameters, in declaration
///   order.
/// - `dep_inner_types`: the inner `T` of each `&T` dep parameter.
fn parse_contributor_params(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
) -> Result<(TokenStream2, Vec<syn::Ident>, Vec<Type>), syn::Error> {
    let mut ctx_pat: Option<TokenStream2> = None;
    let mut dep_idents: Vec<syn::Ident> = Vec::new();
    let mut dep_inner_types: Vec<Type> = Vec::new();

    for (i, arg) in inputs.iter().enumerate() {
        let FnArg::Typed(pat_ty) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "`#[contributor]` functions cannot take `self`",
            ));
        };

        let Pat::Ident(pat_ident) = pat_ty.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &pat_ty.pat,
                "`#[contributor]` parameters must be plain identifiers",
            ));
        };
        let ident = &pat_ident.ident;

        // Check whether this is the special `ctx` arg.
        if i == 0 && (ident == "ctx" || ident == "_ctx") {
            // Validate the type looks plausible — bare `&_` is fine.
            if !matches!(pat_ty.ty.as_ref(), Type::Reference(_)) {
                return Err(syn::Error::new_spanned(
                    &pat_ty.ty,
                    "the `ctx` parameter must be `&dyn ContributorRequest`",
                ));
            }
            ctx_pat = Some(quote! { #ident });
            continue;
        }

        // Otherwise: dep parameter. Type must be `&T`.
        let Type::Reference(TypeReference { elem, .. }) = pat_ty.ty.as_ref() else {
            return Err(syn::Error::new_spanned(
                &pat_ty.ty,
                "`#[contributor]` dep parameters must be `&Type`",
            ));
        };
        dep_idents.push(ident.clone());
        dep_inner_types.push((**elem).clone());
    }

    let ctx_pat = ctx_pat.unwrap_or_else(|| quote! { _ctx });
    Ok((ctx_pat, dep_idents, dep_inner_types))
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
