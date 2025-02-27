#![feature(let_chains)]

mod minify;

use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::sync::{mpsc, Mutex};

use async_trait::async_trait;
use minify::{match_object, minify};
use once_cell::sync::OnceCell;
use rayon::prelude::*;
use regex::Regex;
use rspack_core::rspack_sources::{ConcatSource, MapOptions, RawSource, SourceExt, SourceMap};
use rspack_core::rspack_sources::{Source, SourceMapSource, SourceMapSourceOptions};
use rspack_core::{
  AssetInfo, CompilationAsset, JsChunkHashArgs, Plugin, PluginContext, PluginJsChunkHashHookOutput,
  PluginProcessAssetsOutput, ProcessAssetsArgs,
};
use rspack_error::{internal_error, Diagnostic, Result};
use rspack_regex::RspackRegex;
use rspack_util::try_any_sync;
use swc_config::config_types::BoolOrDataConfig;
use swc_core::base::config::JsMinifyFormatOptions;
pub use swc_ecma_minifier::option::{
  terser::{TerserCompressorOptions, TerserEcmaVersion},
  MangleOptions,
};
#[derive(Debug, Clone, Default)]
pub struct SwcJsMinimizerRspackPluginOptions {
  pub extract_comments: Option<String>,
  pub compress: BoolOrDataConfig<TerserCompressorOptions>,
  pub mangle: BoolOrDataConfig<MangleOptions>,
  pub format: JsMinifyFormatOptions,
  pub test: Option<SwcJsMinimizerRules>,
  pub include: Option<SwcJsMinimizerRules>,
  pub exclude: Option<SwcJsMinimizerRules>,
  pub module: Option<bool>,

  /// Internal fields for hashing only.
  /// This guaranteed these field should only be readonly.
  /// Otherwise, hash would be generated with inconsistencies.
  pub __compress_cache: OnceCell<BoolOrDataConfig<String>>,
  pub __mangle_cache: OnceCell<BoolOrDataConfig<String>>,
  pub __format_cache: OnceCell<String>,
}

impl std::hash::Hash for SwcJsMinimizerRspackPluginOptions {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.extract_comments.hash(state);
    self
      .__format_cache
      .get_or_init(|| serde_json::to_string(&self.format).expect("Should be able to serialize"))
      .hash(state);
    self
      .__compress_cache
      .get_or_init(|| {
        self
          .compress
          .as_ref()
          .map(|v| serde_json::to_string(v).expect("Should be able to serialize"))
      })
      .hash(state);
    self
      .__mangle_cache
      .get_or_init(|| {
        self
          .mangle
          .as_ref()
          .map(|v| serde_json::to_string(v).expect("Should be able to serialize"))
      })
      .hash(state);
    self.test.hash(state);
    self.include.hash(state);
    self.exclude.hash(state);
  }
}

#[derive(Debug, Clone, Hash)]
pub enum SwcJsMinimizerRule {
  String(String),
  Regexp(RspackRegex),
}

impl SwcJsMinimizerRule {
  pub fn try_match(&self, data: &str) -> rspack_error::Result<bool> {
    match self {
      Self::String(s) => Ok(data.starts_with(s)),
      Self::Regexp(r) => Ok(r.test(data)),
    }
  }
}

#[derive(Debug, Clone, Hash)]
pub enum SwcJsMinimizerRules {
  String(String),
  Regexp(rspack_regex::RspackRegex),
  Array(Vec<SwcJsMinimizerRule>),
}

impl SwcJsMinimizerRules {
  pub fn try_match(&self, data: &str) -> rspack_error::Result<bool> {
    match self {
      Self::String(s) => Ok(data.starts_with(s)),
      Self::Regexp(r) => Ok(r.test(data)),
      Self::Array(l) => try_any_sync(l, |i| i.try_match(data)),
    }
  }
}

struct ExtractComments<'a> {
  filename: String,
  condition: &'a Regex,
  banner: String,
}

#[derive(Debug)]
pub struct SwcJsMinimizerRspackPlugin {
  options: SwcJsMinimizerRspackPluginOptions,
}

impl SwcJsMinimizerRspackPlugin {
  pub fn new(options: SwcJsMinimizerRspackPluginOptions) -> Self {
    Self { options }
  }
}

#[async_trait]
impl Plugin for SwcJsMinimizerRspackPlugin {
  fn name(&self) -> &'static str {
    "rspack.SwcJsMinimizerRspackPlugin"
  }

  async fn process_assets_stage_optimize_size(
    &self,
    _ctx: PluginContext,
    args: ProcessAssetsArgs<'_>,
  ) -> PluginProcessAssetsOutput {
    let compilation = args.compilation;
    let minify_options = &self.options;

    let (tx, rx) = mpsc::channel::<Vec<Diagnostic>>();
    // collect all extracted comments info
    let all_extracted_comments = Mutex::new(HashMap::new());
    let extract_comments_condition = minify_options.extract_comments.as_ref().map(|condition| {
      let regexp = if condition.eq("true") {
        // copied from terser-webpack-plugin
        Regex::new(r"@preserve|@lic|@cc_on|^\**!")
      } else {
        Regex::new(&condition[1..condition.len() - 2])
      };
      regexp.expect("Invalid extractComments")
    });
    let emit_source_map_columns = !compilation.options.devtool.cheap();

    compilation
      .assets_mut()
      .par_iter_mut()
      .filter(|(filename, original)| {
        if !(filename.ends_with(".js") || filename.ends_with(".cjs") || filename.ends_with(".mjs")) {
          return false
        }

        let is_matched = match_object(minify_options, filename)
          .unwrap_or(false);

        if !is_matched || original.get_info().minimized {
          return false
        }

        true
      })
      .try_for_each_with(tx,|tx, (filename, original)| -> Result<()>  {
        if let Some(original_source) = original.get_source() {
          let input = original_source.source().to_string();
          let input_source_map = original_source.map(&MapOptions::default());

          let is_module = if let Some(module) = self.options.module {
            module
          } else if let Some(module) = original.info.javascript_module {
            module
          } else {
            filename.ends_with(".mjs")
          };

          let js_minify_options = JsMinifyOptions {
            compress: minify_options.compress.clone(),
            mangle: minify_options.mangle.clone(),
            format: minify_options.format.clone(),
            source_map: BoolOrDataConfig::from_bool(input_source_map.is_some()),
            inline_sources_content: true, /* Using true so original_source can be None in SourceMapSource */
            emit_source_map_columns,
            module: is_module,
            ..Default::default()
          };
          let extract_comments_option = extract_comments_condition.as_ref().map(|condition| {
            let comments_filename = filename.to_string() + ".LICENSE.txt";
            let dir = Path::new(filename).parent().expect("should has parent");
            let relative = Path::new(&comments_filename).strip_prefix(dir).expect("should has common prefix").to_string_lossy().to_string().replace('\\', "/");
            let banner = format!("/*! For license information please see {relative} */");
            ExtractComments {
              filename: comments_filename,
              condition,
              banner
            }
          });
          let output = match minify(
            &js_minify_options,
            input,
            filename,
            &all_extracted_comments,
            &extract_comments_option,
          ) {
            Ok(r) => r,
            Err(e) => {
              tx.send(e.into())
                .map_err(|e| internal_error!(e.to_string()))?;
              return Ok(())
            }
          };
          let source = if let Some(map) = &output.map {
            SourceMapSource::new(SourceMapSourceOptions {
              value: output.code,
              name: filename,
              source_map: SourceMap::from_json(map).map_err(|e| internal_error!(e.to_string()))?,
              original_source: None,
              inner_source_map: input_source_map,
              remove_original_source: true,
            })
            .boxed()
          } else {
            RawSource::from(output.code).boxed()
          };
          let source = if let Some(banner) = extract_comments_option.map(|option| option.banner)
            && all_extracted_comments
            .lock()
            .expect("all_extract_comments lock failed")
            .contains_key(filename)
          {
            ConcatSource::new([
              RawSource::Source(banner).boxed(),
              RawSource::from("\n").boxed(),
              source
            ]).boxed()
          } else {
            source
          };
          original.set_source(Some(source));
          original.get_info_mut().minimized = true;
        }

        Ok(())
    })?;

    compilation.push_batch_diagnostic(rx.into_iter().flatten().collect::<Vec<_>>());

    // write all extracted comments to assets
    all_extracted_comments
      .lock()
      .expect("all_extracted_comments lock failed")
      .clone()
      .into_iter()
      .for_each(|(_, comments)| {
        compilation.emit_asset(
          comments.comments_file_name,
          CompilationAsset {
            source: Some(comments.source),
            info: AssetInfo {
              minimized: true,
              ..Default::default()
            },
          },
        )
      });

    Ok(())
  }

  fn js_chunk_hash(
    &self,
    _ctx: PluginContext,
    args: &mut JsChunkHashArgs,
  ) -> PluginJsChunkHashHookOutput {
    self.name().hash(&mut args.hasher);
    self.options.hash(&mut args.hasher);
    Ok(())
  }
}

#[derive(Debug, Clone, Default)]
pub struct JsMinifyOptions {
  pub compress: BoolOrDataConfig<TerserCompressorOptions>,
  pub mangle: BoolOrDataConfig<MangleOptions>,
  pub format: JsMinifyFormatOptions,
  pub ecma: TerserEcmaVersion,
  pub keep_class_names: bool,
  pub keep_fn_names: bool,
  pub module: bool,
  pub safari10: bool,
  pub toplevel: bool,
  pub source_map: BoolOrDataConfig<TerserSourceMapOption>,
  pub output_path: Option<String>,
  pub inline_sources_content: bool,
  pub emit_source_map_columns: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TerserSourceMapOption {
  pub filename: Option<String>,
  pub url: Option<String>,
  pub root: Option<String>,
  pub content: Option<String>,
}
