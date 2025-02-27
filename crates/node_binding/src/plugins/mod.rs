mod loader;
use std::fmt::Debug;
use std::path::PathBuf;

use async_trait::async_trait;
use napi::{Env, Result};
use rspack_binding_macros::js_fn_into_threadsafe_fn;
use rspack_binding_values::JsExecuteModuleArg;
use rspack_binding_values::{AfterResolveData, JsChunkAssetArgs, JsModule};
use rspack_binding_values::{BeforeResolveData, JsAssetEmittedArgs, ToJsModule};
use rspack_binding_values::{JsResolveForSchemeInput, JsResolveForSchemeResult};
use rspack_core::{ChunkAssetArgs, ModuleIdentifier, NormalModuleAfterResolveArgs};
use rspack_core::{NormalModuleBeforeResolveArgs, PluginNormalModuleFactoryAfterResolveOutput};
use rspack_core::{PluginNormalModuleFactoryBeforeResolveOutput, ResourceData};
use rspack_core::{PluginNormalModuleFactoryResolveForSchemeOutput, PluginShouldEmitHookOutput};
use rspack_error::internal_error;
use rspack_napi_shared::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use rspack_napi_shared::NapiResultExt;

pub use self::loader::JsLoaderResolver;
use crate::{DisabledHooks, Hook, JsCompilation, JsHooks};

pub struct JsHooksAdapter {
  disabled_hooks: DisabledHooks,
  pub make_tsfn: ThreadsafeFunction<(), ()>,
  pub compilation_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub this_compilation_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub process_assets_stage_additional_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_pre_process_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_derived_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_additions_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_none_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_count_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_compatibility_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_size_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_dev_tooling_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_inline_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_summarize_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_hash_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_optimize_transfer_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_analyse_tsfn: ThreadsafeFunction<(), ()>,
  pub process_assets_stage_report_tsfn: ThreadsafeFunction<(), ()>,
  pub emit_tsfn: ThreadsafeFunction<(), ()>,
  pub asset_emitted_tsfn: ThreadsafeFunction<JsAssetEmittedArgs, ()>,
  pub should_emit_tsfn: ThreadsafeFunction<JsCompilation, Option<bool>>,
  pub after_emit_tsfn: ThreadsafeFunction<(), ()>,
  pub optimize_modules_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub optimize_tree_tsfn: ThreadsafeFunction<(), ()>,
  pub optimize_chunk_modules_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub before_compile_tsfn: ThreadsafeFunction<(), ()>,
  pub after_compile_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub finish_modules_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub finish_make_tsfn: ThreadsafeFunction<JsCompilation, ()>,
  pub build_module_tsfn: ThreadsafeFunction<JsModule, ()>, // TODO
  pub chunk_asset_tsfn: ThreadsafeFunction<JsChunkAssetArgs, ()>,
  pub before_resolve: ThreadsafeFunction<BeforeResolveData, (Option<bool>, BeforeResolveData)>,
  pub after_resolve: ThreadsafeFunction<AfterResolveData, Option<bool>>,
  pub context_module_before_resolve: ThreadsafeFunction<BeforeResolveData, Option<bool>>,
  pub normal_module_factory_resolve_for_scheme:
    ThreadsafeFunction<JsResolveForSchemeInput, JsResolveForSchemeResult>,
  pub succeed_module_tsfn: ThreadsafeFunction<JsModule, ()>,
  pub still_valid_module_tsfn: ThreadsafeFunction<JsModule, ()>,
  pub execute_module_tsfn: ThreadsafeFunction<JsExecuteModuleArg, Option<String>>,
}

impl Debug for JsHooksAdapter {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "rspack_plugin_js_hooks_adapter")
  }
}

#[async_trait]
impl rspack_core::Plugin for JsHooksAdapter {
  fn name(&self) -> &'static str {
    "rspack_plugin_js_hooks_adapter"
  }

  async fn compilation(
    &self,
    args: rspack_core::CompilationArgs<'_>,
    _params: &rspack_core::CompilationParams,
  ) -> rspack_core::PluginCompilationHookOutput {
    if self.is_hook_disabled(&Hook::Compilation) {
      return Ok(());
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        args.compilation,
      )
    });

    self
      .compilation_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call compilation: {err}"))?
  }

  async fn this_compilation(
    &self,
    args: rspack_core::ThisCompilationArgs<'_>,
    _params: &rspack_core::CompilationParams,
  ) -> rspack_core::PluginThisCompilationHookOutput {
    if self.is_hook_disabled(&Hook::ThisCompilation) {
      return Ok(());
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        args.this_compilation,
      )
    });

    self
      .this_compilation_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call this_compilation: {err}"))?
  }

  async fn chunk_asset(&self, args: &ChunkAssetArgs) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::ChunkAsset) {
      return Ok(());
    }

    self
      .chunk_asset_tsfn
      .call(
        JsChunkAssetArgs::from(args),
        ThreadsafeFunctionCallMode::NonBlocking,
      )
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to chunk asset: {err}"))?
  }

  #[tracing::instrument(name = "js_hooks_adapter::make", skip_all)]
  async fn make(
    &self,
    _ctx: rspack_core::PluginContext,
    _compilation: &mut rspack_core::Compilation,
    _param: &mut rspack_core::MakeParam,
  ) -> rspack_core::PluginMakeHookOutput {
    if self.is_hook_disabled(&Hook::Make) {
      return Ok(());
    }

    // We don't need to expose `compilation` to Node as it's already been exposed via `compilation` hook
    self
      .make_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call make: {err}",))?
  }

  async fn before_resolve(
    &self,
    _ctx: rspack_core::PluginContext,
    args: &mut NormalModuleBeforeResolveArgs,
  ) -> PluginNormalModuleFactoryBeforeResolveOutput {
    if self.is_hook_disabled(&Hook::BeforeResolve) {
      return Ok(None);
    }
    match self
      .before_resolve
      .call(args.clone().into(), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call this_compilation: {err}"))?
    {
      Ok((ret, resolve_data)) => {
        args.request = resolve_data.request;
        args.context = resolve_data.context;
        Ok(ret)
      }
      Err(err) => Err(err),
    }
  }

  async fn after_resolve(
    &self,
    _ctx: rspack_core::PluginContext,
    args: &NormalModuleAfterResolveArgs,
  ) -> PluginNormalModuleFactoryAfterResolveOutput {
    if self.is_hook_disabled(&Hook::AfterResolve) {
      return Ok(None);
    }
    self
      .after_resolve
      .call(args.clone().into(), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call this_compilation: {err}"))?
  }
  async fn context_module_before_resolve(
    &self,
    _ctx: rspack_core::PluginContext,
    args: &mut NormalModuleBeforeResolveArgs,
  ) -> PluginNormalModuleFactoryBeforeResolveOutput {
    self
      .context_module_before_resolve
      .call(args.clone().into(), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call this_compilation: {err}"))?
  }
  async fn normal_module_factory_resolve_for_scheme(
    &self,
    _ctx: rspack_core::PluginContext,
    args: ResourceData,
  ) -> PluginNormalModuleFactoryResolveForSchemeOutput {
    if self.is_hook_disabled(&Hook::NormalModuleFactoryResolveForScheme) {
      return Ok((args, false));
    }
    let res = self
      .normal_module_factory_resolve_for_scheme
      .call(args.into(), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call this_compilation: {err}"))?;
    res.map(|res| {
      let JsResolveForSchemeResult {
        resource_data,
        stop,
      } = res;
      (
        ResourceData::new(resource_data.resource, PathBuf::from(resource_data.path))
          .query_optional(resource_data.query)
          .fragment_optional(resource_data.fragment),
        stop,
      )
    })
  }

  async fn process_assets_stage_additional(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageAdditional) {
      return Ok(());
    }

    self
      .process_assets_stage_additional_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage additional: {err}",))?
  }

  async fn process_assets_stage_pre_process(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStagePreProcess) {
      return Ok(());
    }

    self
      .process_assets_stage_pre_process_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage pre-process: {err}",))?
  }

  async fn process_assets_stage_derived(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageDerived) {
      return Ok(());
    }

    self
      .process_assets_stage_derived_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage derived: {err}",))?
  }

  async fn process_assets_stage_additions(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageAdditions) {
      return Ok(());
    }

    self
      .process_assets_stage_additions_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage additions: {err}",))?
  }

  async fn process_assets_stage_none(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageNone) {
      return Ok(());
    }

    self
      .process_assets_stage_none_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets: {err}",))?
  }

  async fn process_assets_stage_optimize(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimize) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage optimize: {err}",))?
  }

  async fn process_assets_stage_optimize_count(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimizeCount) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_count_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(
        |err| internal_error!("Failed to call process assets stage optimize count: {err}",),
      )?
  }

  async fn process_assets_stage_optimize_compatibility(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimizeCompatibility) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_compatibility_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| {
        internal_error!("Failed to call process assets stage optimize compatibility: {err}",)
      })?
  }

  async fn process_assets_stage_optimize_size(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimizeSize) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_size_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage optimize size: {err}",))?
  }

  async fn process_assets_stage_dev_tooling(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageDevTooling) {
      return Ok(());
    }

    self
      .process_assets_stage_dev_tooling_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage dev tooling: {err}",))?
  }

  async fn process_assets_stage_optimize_inline(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimizeInline) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_inline_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| {
        internal_error!("Failed to call process assets stage optimize inline: {err}",)
      })?
  }

  async fn process_assets_stage_summarize(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageSummarize) {
      return Ok(());
    }

    // Directly calling hook processAssets without converting assets to JsAssets, instead, we use APIs to get `Source` lazily on the Node side.
    self
      .process_assets_stage_summarize_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage summarize: {err}",))?
  }

  async fn process_assets_stage_optimize_hash(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimizeHash) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_hash_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage summarize: {err}",))?
  }

  async fn process_assets_stage_optimize_transfer(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageOptimizeTransfer) {
      return Ok(());
    }

    self
      .process_assets_stage_optimize_transfer_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| {
        internal_error!("Failed to call process assets stage optimize transfer: {err}",)
      })?
  }

  async fn process_assets_stage_analyse(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageAnalyse) {
      return Ok(());
    }

    self
      .process_assets_stage_analyse_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage analyse: {err}",))?
  }

  async fn process_assets_stage_report(
    &self,
    _ctx: rspack_core::PluginContext,
    _args: rspack_core::ProcessAssetsArgs<'_>,
  ) -> rspack_core::PluginProcessAssetsHookOutput {
    if self.is_hook_disabled(&Hook::ProcessAssetsStageReport) {
      return Ok(());
    }
    // Directly calling hook processAssets without converting assets to JsAssets, instead, we use APIs to get `Source` lazily on the Node side.
    self
      .process_assets_stage_report_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call process assets stage report: {err}",))?
  }

  async fn optimize_modules(
    &self,
    compilation: &mut rspack_core::Compilation,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::OptimizeModules) {
      return Ok(());
    }
    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        compilation,
      )
    });
    self
      .optimize_modules_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::Blocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call optimize modules: {err}"))?
  }

  async fn optimize_tree(
    &self,
    _compilation: &mut rspack_core::Compilation,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::OptimizeTree) {
      return Ok(());
    }
    self
      .optimize_tree_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call optimize tree: {err}",))?
  }

  async fn optimize_chunk_modules(
    &self,
    args: rspack_core::OptimizeChunksArgs<'_>,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::OptimizeChunkModules) {
      return Ok(());
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        args.compilation,
      )
    });

    self
      .optimize_chunk_modules_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to compilation: {err}"))?
  }

  async fn before_compile(
    &self,
    _params: &rspack_core::CompilationParams,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::BeforeCompile) {
      return Ok(());
    }

    self
      .before_compile_tsfn
      .call({}, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call before compile: {err}",))?
  }

  async fn after_compile(
    &self,
    compilation: &mut rspack_core::Compilation,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::AfterCompile) {
      return Ok(());
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        compilation,
      )
    });

    self
      .after_compile_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call after compile: {err}"))?
  }

  async fn finish_make(
    &self,
    compilation: &mut rspack_core::Compilation,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::FinishMake) {
      return Ok(());
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        compilation,
      )
    });

    self
      .finish_make_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call finish make: {err}"))?
  }

  async fn build_module(&self, module: &mut dyn rspack_core::Module) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::BuildModule) {
      return Ok(());
    }

    self
      .build_module_tsfn
      .call(
        module.to_js_module().expect("Convert to js_module failed."),
        ThreadsafeFunctionCallMode::NonBlocking,
      )
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call build module: {err}"))?
  }

  async fn finish_modules(
    &self,
    compilation: &mut rspack_core::Compilation,
  ) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::FinishModules) {
      return Ok(());
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        compilation,
      )
    });

    self
      .finish_modules_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to finish modules: {err}"))?
  }

  async fn emit(&self, _: &mut rspack_core::Compilation) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::Emit) {
      return Ok(());
    }

    self
      .emit_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call emit: {err}"))?
  }

  async fn asset_emitted(&self, args: &rspack_core::AssetEmittedArgs) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::AssetEmitted) {
      return Ok(());
    }

    let args: JsAssetEmittedArgs = args.into();
    self
      .asset_emitted_tsfn
      .call(args, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call asset emitted: {err}"))?
  }

  async fn should_emit(
    &self,
    compilation: &mut rspack_core::Compilation,
  ) -> PluginShouldEmitHookOutput {
    if self.is_hook_disabled(&Hook::ShouldEmit) {
      return Ok(None);
    }

    let compilation = JsCompilation::from_compilation(unsafe {
      std::mem::transmute::<&'_ mut rspack_core::Compilation, &'static mut rspack_core::Compilation>(
        compilation,
      )
    });

    let res = self
      .should_emit_tsfn
      .call(compilation, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await;
    res.map_err(|err| internal_error!("Failed to call should emit: {err}"))?
  }

  async fn after_emit(&self, _: &mut rspack_core::Compilation) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::AfterEmit) {
      return Ok(());
    }

    self
      .after_emit_tsfn
      .call((), ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call after emit: {err}"))?
  }

  async fn succeed_module(&self, args: &dyn rspack_core::Module) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::SucceedModule) {
      return Ok(());
    }
    let js_module = args
      .to_js_module()
      .expect("Failed to convert module to JsModule");
    self
      .succeed_module_tsfn
      .call(js_module, ThreadsafeFunctionCallMode::NonBlocking)
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call succeed_module hook: {err}"))?
  }

  async fn still_valid_module(&self, args: &dyn rspack_core::Module) -> rspack_error::Result<()> {
    if self.is_hook_disabled(&Hook::StillValidModule) {
      return Ok(());
    }

    self
      .still_valid_module_tsfn
      .call(
        args.to_js_module().expect("Convert to js_module failed."),
        ThreadsafeFunctionCallMode::NonBlocking,
      )
      .into_rspack_result()?
      .await
      .map_err(|err| internal_error!("Failed to call still_valid_module hook: {err}"))?
  }

  fn execute_module(
    &self,
    entry: ModuleIdentifier,
    runtime_modules: Vec<ModuleIdentifier>,
    codegen_results: &rspack_core::CodeGenerationResults,
  ) -> rspack_error::Result<Option<String>> {
    if self.is_hook_disabled(&Hook::ExecuteModule) {
      return Ok(None);
    }

    self
      .execute_module_tsfn
      .call(
        JsExecuteModuleArg {
          entry: entry.to_string(),
          runtime_modules: runtime_modules
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
          codegen_results: codegen_results.clone().into(),
        },
        ThreadsafeFunctionCallMode::NonBlocking,
      )
      .into_rspack_result()?
      .blocking_recv()
      .map_err(|recv_err| rspack_error::internal_error!(recv_err.to_string()))?
  }
}

impl JsHooksAdapter {
  pub fn from_js_hooks(env: Env, js_hooks: JsHooks, disabled_hooks: DisabledHooks) -> Result<Self> {
    let JsHooks {
      make,
      process_assets_stage_additional,
      process_assets_stage_pre_process,
      process_assets_stage_derived,
      process_assets_stage_additions,
      process_assets_stage_none,
      process_assets_stage_optimize,
      process_assets_stage_optimize_count,
      process_assets_stage_optimize_compatibility,
      process_assets_stage_optimize_size,
      process_assets_stage_dev_tooling,
      process_assets_stage_optimize_inline,
      process_assets_stage_summarize,
      process_assets_stage_optimize_hash,
      process_assets_stage_optimize_transfer,
      process_assets_stage_analyse,
      process_assets_stage_report,
      this_compilation,
      compilation,
      should_emit,
      emit,
      asset_emitted,
      after_emit,
      optimize_modules,
      optimize_tree,
      optimize_chunk_modules,
      before_resolve,
      after_resolve,
      context_module_before_resolve,
      normal_module_factory_resolve_for_scheme,
      before_compile,
      after_compile,
      finish_modules,
      finish_make,
      build_module,
      chunk_asset,
      succeed_module,
      still_valid_module,
      execute_module,
    } = js_hooks;

    let process_assets_stage_additional_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_additional, env);
    let process_assets_stage_pre_process_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_pre_process, env);
    let process_assets_stage_derived_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_derived, env);
    let process_assets_stage_additions_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_additions, env);
    let process_assets_stage_none_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_none, env);
    let process_assets_stage_optimize_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize, env);
    let process_assets_stage_optimize_count_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize_count, env);
    let process_assets_stage_optimize_compatibility_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize_compatibility, env);
    let process_assets_stage_optimize_size_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize_size, env);
    let process_assets_stage_dev_tooling_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_dev_tooling, env);
    let process_assets_stage_optimize_inline_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize_inline, env);
    let process_assets_stage_optimize_transfer_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize_transfer, env);
    let process_assets_stage_analyse_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_analyse, env);
    let process_assets_stage_summarize_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_summarize, env);
    let process_assets_stage_optimize_hash_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_optimize_hash, env);
    let process_assets_stage_report_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(process_assets_stage_report, env);
    let emit_tsfn: ThreadsafeFunction<(), ()> = js_fn_into_threadsafe_fn!(emit, env);
    let should_emit_tsfn: ThreadsafeFunction<JsCompilation, Option<bool>> =
      js_fn_into_threadsafe_fn!(should_emit, env);
    let asset_emitted_tsfn: ThreadsafeFunction<JsAssetEmittedArgs, ()> =
      js_fn_into_threadsafe_fn!(asset_emitted, env);
    let after_emit_tsfn: ThreadsafeFunction<(), ()> = js_fn_into_threadsafe_fn!(after_emit, env);
    let this_compilation_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(this_compilation, env);
    let compilation_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(compilation, env);
    let make_tsfn: ThreadsafeFunction<(), ()> = js_fn_into_threadsafe_fn!(make, env);
    let optimize_modules_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(optimize_modules, env);
    let optimize_tree_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(optimize_tree, env);
    let optimize_chunk_modules_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(optimize_chunk_modules, env);
    let before_compile_tsfn: ThreadsafeFunction<(), ()> =
      js_fn_into_threadsafe_fn!(before_compile, env);
    let after_compile_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(after_compile, env);
    let finish_make_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(finish_make, env);
    let build_module_tsfn: ThreadsafeFunction<JsModule, ()> =
      js_fn_into_threadsafe_fn!(build_module, env);
    let finish_modules_tsfn: ThreadsafeFunction<JsCompilation, ()> =
      js_fn_into_threadsafe_fn!(finish_modules, env);
    let context_module_before_resolve: ThreadsafeFunction<BeforeResolveData, Option<bool>> =
      js_fn_into_threadsafe_fn!(context_module_before_resolve, env);
    let before_resolve: ThreadsafeFunction<BeforeResolveData, (Option<bool>, BeforeResolveData)> =
      js_fn_into_threadsafe_fn!(before_resolve, env);
    let after_resolve: ThreadsafeFunction<AfterResolveData, Option<bool>> =
      js_fn_into_threadsafe_fn!(after_resolve, env);
    let normal_module_factory_resolve_for_scheme: ThreadsafeFunction<
      JsResolveForSchemeInput,
      JsResolveForSchemeResult,
    > = js_fn_into_threadsafe_fn!(normal_module_factory_resolve_for_scheme, env);
    let chunk_asset_tsfn: ThreadsafeFunction<JsChunkAssetArgs, ()> =
      js_fn_into_threadsafe_fn!(chunk_asset, env);
    let succeed_module_tsfn: ThreadsafeFunction<JsModule, ()> =
      js_fn_into_threadsafe_fn!(succeed_module, env);
    let still_valid_module_tsfn: ThreadsafeFunction<JsModule, ()> =
      js_fn_into_threadsafe_fn!(still_valid_module, env);
    let execute_module_tsfn: ThreadsafeFunction<JsExecuteModuleArg, Option<String>> =
      js_fn_into_threadsafe_fn!(execute_module, env);

    Ok(JsHooksAdapter {
      disabled_hooks,
      make_tsfn,
      process_assets_stage_additional_tsfn,
      process_assets_stage_pre_process_tsfn,
      process_assets_stage_derived_tsfn,
      process_assets_stage_additions_tsfn,
      process_assets_stage_none_tsfn,
      process_assets_stage_optimize_tsfn,
      process_assets_stage_optimize_count_tsfn,
      process_assets_stage_optimize_compatibility_tsfn,
      process_assets_stage_optimize_size_tsfn,
      process_assets_stage_dev_tooling_tsfn,
      process_assets_stage_optimize_inline_tsfn,
      process_assets_stage_summarize_tsfn,
      process_assets_stage_optimize_hash_tsfn,
      process_assets_stage_optimize_transfer_tsfn,
      process_assets_stage_analyse_tsfn,
      process_assets_stage_report_tsfn,
      compilation_tsfn,
      this_compilation_tsfn,
      should_emit_tsfn,
      emit_tsfn,
      asset_emitted_tsfn,
      after_emit_tsfn,
      optimize_modules_tsfn,
      optimize_tree_tsfn,
      optimize_chunk_modules_tsfn,
      before_compile_tsfn,
      after_compile_tsfn,
      before_resolve,
      context_module_before_resolve,
      normal_module_factory_resolve_for_scheme,
      finish_modules_tsfn,
      finish_make_tsfn,
      build_module_tsfn,
      chunk_asset_tsfn,
      after_resolve,
      succeed_module_tsfn,
      still_valid_module_tsfn,
      execute_module_tsfn,
    })
  }

  #[allow(clippy::unwrap_used)]
  fn is_hook_disabled(&self, hook: &Hook) -> bool {
    self.disabled_hooks.read().expect("").contains(hook)
  }
}
