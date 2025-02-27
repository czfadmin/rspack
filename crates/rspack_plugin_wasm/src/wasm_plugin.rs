use std::fmt::Debug;

use rayon::prelude::*;
use rspack_core::{
  ApplyContext, CompilationArgs, CompilationParams, CompilerOptions, DependencyType, ModuleType,
  ParserAndGenerator, Plugin, PluginCompilationHookOutput, PluginContext,
  PluginRenderManifestHookOutput, RenderManifestArgs, RenderManifestEntry, SourceType,
};
use rspack_error::Result;

use crate::{AsyncWasmParserAndGenerator, ModuleIdToFileName};

pub struct EnableWasmLoadingPlugin;

#[derive(Debug, Default)]
pub struct AsyncWasmPlugin {
  pub module_id_to_filename_without_ext: ModuleIdToFileName,
}

impl AsyncWasmPlugin {
  pub fn new() -> AsyncWasmPlugin {
    Self {
      module_id_to_filename_without_ext: Default::default(),
    }
  }
}

#[async_trait::async_trait]
impl Plugin for AsyncWasmPlugin {
  fn name(&self) -> &'static str {
    "rspack.AsyncWebAssemblyModulesPlugin"
  }

  async fn compilation(
    &self,
    args: CompilationArgs<'_>,
    params: &CompilationParams,
  ) -> PluginCompilationHookOutput {
    args.compilation.set_dependency_factory(
      DependencyType::WasmImport,
      params.normal_module_factory.clone(),
    );
    args.compilation.set_dependency_factory(
      DependencyType::WasmExportImported,
      params.normal_module_factory.clone(),
    );
    Ok(())
  }

  fn apply(
    &self,
    ctx: PluginContext<&mut ApplyContext>,
    _options: &mut CompilerOptions,
  ) -> Result<()> {
    let module_id_to_filename_without_ext = self.module_id_to_filename_without_ext.clone();

    let builder = move || {
      Box::new({
        AsyncWasmParserAndGenerator {
          module_id_to_filename: module_id_to_filename_without_ext.clone(),
        }
      }) as Box<dyn ParserAndGenerator>
    };

    ctx
      .context
      .register_parser_and_generator_builder(ModuleType::WasmAsync, Box::new(builder));

    Ok(())
  }

  // fn render(&self, _ctx: PluginContext, _args: &RenderArgs) -> PluginRenderStartupHookOutput {
  //
  // }

  async fn render_manifest(
    &self,
    _ctx: PluginContext,
    args: RenderManifestArgs<'_>,
  ) -> PluginRenderManifestHookOutput {
    let compilation = args.compilation;
    let chunk = args.chunk();
    let module_graph = &compilation.module_graph;

    let ordered_modules = compilation
      .chunk_graph
      .get_chunk_modules(&args.chunk_ukey, module_graph);

    let files = ordered_modules
      .par_iter()
      .filter(|m| *m.module_type() == ModuleType::WasmAsync)
      .map(|m| {
        let code_gen_result = compilation
          .code_generation_results
          .get(&m.identifier(), Some(&chunk.runtime))?;

        let result = code_gen_result.get(&SourceType::Wasm).map(|source| {
          let (output_path, asset_info) = self
            .module_id_to_filename_without_ext
            .get(&m.identifier())
            .map(|s| s.clone())
            .expect("should have wasm_filename");
          RenderManifestEntry::new(source.clone(), output_path, asset_info, false, false)
        });

        Ok(result)
      })
      .collect::<Result<Vec<Option<RenderManifestEntry>>>>()?
      .into_iter()
      .flatten()
      .collect::<Vec<RenderManifestEntry>>();

    Ok(files)
  }
}
