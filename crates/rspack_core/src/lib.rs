#![feature(let_chains)]
#![feature(if_let_guard)]
#![feature(iter_intersperse)]
#![feature(box_patterns)]
#![feature(anonymous_lifetime_in_impl_trait)]
#![feature(hash_raw_entry)]

use std::sync::atomic::AtomicBool;
use std::{fmt, sync::Arc};
mod dependencies_block;
pub use dependencies_block::{
  AsyncDependenciesBlock, AsyncDependenciesBlockIdentifier, DependenciesBlock, DependencyLocation,
};
mod fake_namespace_object;
pub use fake_namespace_object::*;
mod module_profile;
pub use module_profile::*;
use rspack_database::Database;
pub mod external_module;
pub use external_module::*;
mod logger;
pub use logger::*;
pub mod cache;
mod missing_module;
pub use missing_module::*;
mod normal_module;
mod raw_module;
pub use raw_module::*;
mod exports_info;
pub use exports_info::*;
pub mod module;
pub mod parser_and_generator;
pub use module::*;
pub use parser_and_generator::*;
mod runtime_globals;
pub use normal_module::*;
pub use runtime_globals::RuntimeGlobals;
mod plugin;
pub use plugin::*;
mod context_module;
pub use context_module::*;
mod context_module_factory;
pub use context_module_factory::*;
mod init_fragment;
pub use init_fragment::*;
mod module_factory;
pub use module_factory::*;
mod normal_module_factory;
pub use normal_module_factory::*;
mod compiler;
pub use compiler::*;
mod options;
pub use options::*;
mod module_graph;
pub use module_graph::*;
mod chunk;
pub use chunk::*;
mod dependency;
pub use dependency::*;
mod utils;
pub use utils::*;
mod chunk_graph;
pub use chunk_graph::*;
mod build_chunk_graph;
mod stats;
pub use stats::*;
mod runtime;
mod runtime_module;
pub use runtime::*;
pub use runtime_module::*;
mod code_generation_results;
pub use code_generation_results::*;
mod entrypoint;
pub use entrypoint::*;
mod loader;
pub use loader::*;
// mod external;
// pub use external::*;
mod chunk_group;
pub use chunk_group::*;
mod ukey;
pub use ukey::*;
mod module_graph_module;
pub use module_graph_module::*;
pub mod resolver;
pub use resolver::*;
pub mod tree_shaking;

pub use rspack_loader_runner::{get_scheme, ResourceData, Scheme, BUILTIN_LOADER_PREFIX};
pub use rspack_sources;

#[cfg(debug_assertions)]
pub mod debug_info;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceType {
  JavaScript,
  Css,
  Wasm,
  Asset,
  Remote,
  ShareInit,
  ConsumeShared,
  #[default]
  Unknown,
}

impl std::fmt::Display for SourceType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      SourceType::JavaScript => write!(f, "javascript"),
      SourceType::Css => write!(f, "css"),
      SourceType::Wasm => write!(f, "wasm"),
      SourceType::Asset => write!(f, "asset"),
      SourceType::Remote => write!(f, "remote"),
      SourceType::ShareInit => write!(f, "share-init"),
      SourceType::ConsumeShared => write!(f, "consume-shared"),
      SourceType::Unknown => write!(f, "unknown"),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleType {
  Json,
  Css,
  CssModule,
  CssAuto,
  Js,
  JsDynamic,
  JsEsm,
  Jsx,
  JsxDynamic,
  JsxEsm,
  Tsx,
  Ts,
  WasmSync,
  WasmAsync,
  AssetInline,
  AssetResource,
  AssetSource,
  Asset,
  Runtime,
  Remote,
  Fallback,
  ProvideShared,
  ConsumeShared,
}

impl ModuleType {
  pub fn is_css_like(&self) -> bool {
    matches!(self, Self::Css | Self::CssModule | Self::CssAuto)
  }

  pub fn is_js_like(&self) -> bool {
    matches!(
      self,
      ModuleType::Js | ModuleType::JsEsm | ModuleType::JsDynamic | ModuleType::Ts
    ) || self.is_jsx_like()
  }

  pub fn is_asset_like(&self) -> bool {
    matches!(
      self,
      ModuleType::Asset | ModuleType::AssetInline | ModuleType::AssetResource
    )
  }
  pub fn is_jsx_like(&self) -> bool {
    matches!(
      self,
      ModuleType::Tsx | ModuleType::Jsx | ModuleType::JsxEsm | ModuleType::JsxDynamic
    )
  }

  pub fn is_wasm_like(&self) -> bool {
    matches!(self, ModuleType::WasmSync | ModuleType::WasmAsync)
  }

  pub fn is_js_auto(&self) -> bool {
    matches!(
      self,
      ModuleType::Js | ModuleType::Jsx | ModuleType::Ts | ModuleType::Tsx
    )
  }

  pub fn is_js_esm(&self) -> bool {
    matches!(
      self,
      ModuleType::JsEsm | ModuleType::JsxEsm | ModuleType::Ts | ModuleType::Tsx
    )
  }

  pub fn is_js_dynamic(&self) -> bool {
    matches!(
      self,
      ModuleType::JsDynamic | ModuleType::JsxDynamic | ModuleType::Ts | ModuleType::Tsx
    )
  }

  /// Webpack arbitrary determines the binary type from [NormalModule.binary](https://github.com/webpack/webpack/blob/1f99ad6367f2b8a6ef17cce0e058f7a67fb7db18/lib/NormalModule.js#L302)
  pub fn is_binary(&self) -> bool {
    self.is_asset_like() || self.is_wasm_like()
  }

  pub fn as_str(&self) -> &str {
    match self {
      ModuleType::Js => "javascript/auto",
      ModuleType::JsEsm => "javascript/esm",
      ModuleType::JsDynamic => "javascript/dynamic",

      ModuleType::Jsx => "javascriptx",
      ModuleType::JsxEsm => "javascriptx/esm",
      ModuleType::JsxDynamic => "javascriptx/dynamic",

      ModuleType::Ts => "typescript",
      ModuleType::Tsx => "typescriptx",

      ModuleType::Css => "css",
      ModuleType::CssModule => "css/module",
      ModuleType::CssAuto => "css/auto",

      ModuleType::Json => "json",

      ModuleType::WasmSync => "webassembly/sync",
      ModuleType::WasmAsync => "webassembly/async",

      ModuleType::Asset => "asset",
      ModuleType::AssetSource => "asset/source",
      ModuleType::AssetResource => "asset/resource",
      ModuleType::AssetInline => "asset/inline",
      ModuleType::Runtime => "runtime",
      ModuleType::Remote => "remote-module",
      ModuleType::Fallback => "fallback-module",
      ModuleType::ProvideShared => "provide-module",
      ModuleType::ConsumeShared => "consume-shared-module",
    }
  }
}

impl fmt::Display for ModuleType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.as_str(),)
  }
}

impl TryFrom<&str> for ModuleType {
  type Error = rspack_error::Error;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    match value {
      "mjs" => Ok(Self::JsEsm),
      "cjs" => Ok(Self::JsDynamic),
      "js" | "javascript" | "js/auto" | "javascript/auto" => Ok(Self::Js),
      "js/dynamic" | "javascript/dynamic" => Ok(Self::JsDynamic),
      "js/esm" | "javascript/esm" => Ok(Self::JsEsm),

      // TODO: remove in 0.5.0
      "jsx" | "javascriptx" | "jsx/auto" | "javascriptx/auto" => Ok(Self::Jsx),
      "jsx/dynamic" | "javascriptx/dynamic" => Ok(Self::JsxDynamic),
      "jsx/esm" | "javascriptx/esm" => Ok(Self::JsxEsm),

      // TODO: remove in 0.5.0
      "ts" | "typescript" => Ok(Self::Ts),
      "tsx" | "typescriptx" => Ok(Self::Tsx),

      "css" => Ok(Self::Css),
      "css/module" => Ok(Self::CssModule),
      "css/auto" => Ok(Self::CssAuto),

      "json" => Ok(Self::Json),

      "webassembly/sync" => Ok(Self::WasmSync),
      "webassembly/async" => Ok(Self::WasmAsync),

      "asset" => Ok(Self::Asset),
      "asset/resource" => Ok(Self::AssetResource),
      "asset/source" => Ok(Self::AssetSource),
      "asset/inline" => Ok(Self::AssetInline),

      _ => {
        use rspack_error::internal_error;
        Err(internal_error!("invalid module type: {value}"))
      }
    }
  }
}

pub type ChunkByUkey = Database<Chunk>;
pub type ChunkGroupByUkey = Database<ChunkGroup>;
pub(crate) type SharedPluginDriver = Arc<PluginDriver>;

pub static IS_NEW_TREESHAKING: AtomicBool = AtomicBool::new(false);
