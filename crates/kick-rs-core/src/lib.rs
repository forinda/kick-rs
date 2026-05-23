#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod adapter;
pub mod container;
pub mod contributor;
pub mod error;
pub mod introspect;
pub mod module;
pub mod mount_sort;
pub mod plugin;
pub mod scope;
pub mod service_impl;
pub mod token;

pub use adapter::{
    define_adapter, Adapter, AdapterContext, AdapterDef, AdapterFactory, BuildContext,
};
pub use container::{Container, ContainerBuilder};
pub use contributor::{
    erase as erase_contributor, AnyContributor, ContextContributor, ContributorDeps,
    ContributorPipeline, ContributorRequest, ContributorRequestExt, ContributorStore,
    ErasedContributor, MutableContributorRequest, OnErrorAction,
};
pub use error::{KickError, KickResult};
pub use introspect::{Introspect, IntrospectionKind, IntrospectionSnapshot};
pub use module::{define_module, Module, ModuleBuilder, ProviderSpec};
pub use plugin::{define_plugin, Plugin, PluginDef, PluginFactory};
pub use scope::Scope;
pub use service_impl::ServiceImpl;
pub use token::Token;
