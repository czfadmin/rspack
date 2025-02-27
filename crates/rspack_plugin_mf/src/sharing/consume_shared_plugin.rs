use std::sync::Mutex;
use std::{fmt, path::Path, sync::Arc};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use rspack_core::{
  AdditionalChunkRuntimeRequirementsArgs, Compilation, CompilationParams, Context,
  DependencyCategory, DependencyType, FactorizeArgs, ModuleExt, ModuleFactoryResult,
  NormalModuleCreateData, Plugin, PluginAdditionalChunkRuntimeRequirementsOutput, PluginContext,
  PluginFactorizeHookOutput, PluginNormalModuleFactoryCreateModuleHookOutput,
  PluginThisCompilationHookOutput, ResolveOptionsWithDependencyType, ResolveResult, Resolver,
  RuntimeGlobals, ThisCompilationArgs,
};
use rspack_error::internal_error;
use rustc_hash::FxHashMap;

use super::{
  consume_shared_module::ConsumeSharedModule,
  consume_shared_runtime_module::ConsumeSharedRuntimeModule,
};

#[derive(Debug, Clone)]
pub struct ConsumeOptions {
  pub import: Option<String>,
  pub import_resolved: Option<String>,
  pub share_key: String,
  pub share_scope: String,
  pub required_version: Option<ConsumeVersion>,
  pub package_name: Option<String>,
  pub strict_version: bool,
  pub singleton: bool,
  pub eager: bool,
}

#[derive(Debug, Clone)]
pub enum ConsumeVersion {
  Version(String),
  False,
}

impl fmt::Display for ConsumeVersion {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ConsumeVersion::Version(v) => write!(f, "{}", v),
      ConsumeVersion::False => write!(f, "*"),
    }
  }
}

static RELATIVE_REQUEST: Lazy<Regex> =
  Lazy::new(|| Regex::new(r"^\.\.?(\/|$)").expect("Invalid regex"));
static ABSOLUTE_REQUEST: Lazy<Regex> =
  Lazy::new(|| Regex::new(r"^(\/|[A-Za-z]:\\|\\\\)").expect("Invalid regex"));
static PACKAGE_NAME: Lazy<Regex> =
  Lazy::new(|| Regex::new(r"^((?:@[^\\/]+[\\/])?[^\\/]+)").expect("Invalid regex"));

#[derive(Debug)]
struct MatchedConsumes {
  resolved: FxHashMap<String, Arc<ConsumeOptions>>,
  unresolved: FxHashMap<String, Arc<ConsumeOptions>>,
  prefixed: FxHashMap<String, Arc<ConsumeOptions>>,
}

fn resolve_matched_configs(
  compilation: &mut Compilation,
  resolver: Arc<Resolver>,
  configs: &[(String, Arc<ConsumeOptions>)],
) -> MatchedConsumes {
  let mut resolved = FxHashMap::default();
  let mut unresolved = FxHashMap::default();
  let mut prefixed = FxHashMap::default();
  for (request, config) in configs {
    if RELATIVE_REQUEST.is_match(request) {
      let Ok(ResolveResult::Resource(resource)) =
        resolver.resolve(compilation.options.context.as_ref(), request)
      else {
        compilation
          .push_batch_diagnostic(internal_error!("Can't resolve shared module {request}").into());
        continue;
      };
      resolved.insert(resource.path.to_string_lossy().into_owned(), config.clone());
      compilation.file_dependencies.insert(resource.path);
    } else if ABSOLUTE_REQUEST.is_match(request) {
      resolved.insert(request.to_owned(), config.clone());
    } else if request.ends_with('/') {
      prefixed.insert(request.to_owned(), config.clone());
    } else {
      unresolved.insert(request.to_owned(), config.clone());
    }
  }
  MatchedConsumes {
    resolved,
    unresolved,
    prefixed,
  }
}

async fn get_description_file(mut dir: &Path) -> Option<serde_json::Value> {
  let description_filename = "package.json";
  loop {
    let description_file = dir.join(description_filename);
    if let Ok(data) = tokio::fs::read(description_file).await
      && let Ok(data) = serde_json::from_slice::<serde_json::Value>(&data)
    {
      return Some(data);
    }
    if let Some(parent) = dir.parent() {
      dir = parent;
    } else {
      return None;
    }
  }
}

fn get_required_version_from_description_file(
  data: serde_json::Value,
  package_name: &str,
) -> Option<ConsumeVersion> {
  let Some(data) = data.as_object() else {
    return None;
  };
  let get_version_from_dependencies = |dependencies: &str| {
    data
      .get(dependencies)
      .and_then(|d| d.as_object())
      .and_then(|deps| deps.get(package_name))
      .and_then(|v| v.as_str())
      .map(|v| ConsumeVersion::Version(v.to_string()))
  };
  get_version_from_dependencies("optionalDependencies")
    .or_else(|| get_version_from_dependencies("dependencies"))
    .or_else(|| get_version_from_dependencies("peerDependencies"))
    .or_else(|| get_version_from_dependencies("devDependencies"))
}

#[derive(Debug)]
pub struct ConsumeSharedPlugin {
  consumes: Vec<(String, Arc<ConsumeOptions>)>,
  resolver: Mutex<Option<Arc<Resolver>>>,
  compiler_context: Mutex<Option<Context>>,
  matched_consumes: Mutex<Option<Arc<MatchedConsumes>>>,
}

impl ConsumeSharedPlugin {
  pub fn new(consumes: Vec<(String, ConsumeOptions)>) -> Self {
    Self {
      consumes: consumes
        .into_iter()
        .map(|(k, v)| (k, Arc::new(v)))
        .collect(),
      resolver: Default::default(),
      compiler_context: Default::default(),
      matched_consumes: Default::default(),
    }
  }

  fn init_context(&self, compilation: &Compilation) {
    let mut lock = self.compiler_context.lock().expect("should lock");
    *lock = Some(compilation.options.context.clone());
  }

  fn get_context(&self) -> Context {
    let lock = self.compiler_context.lock().expect("should lock");
    lock.clone().expect("init_context first")
  }

  fn init_resolver(&self, compilation: &Compilation) {
    let mut lock = self.resolver.lock().expect("should lock");
    *lock = Some(
      compilation
        .resolver_factory
        .get(ResolveOptionsWithDependencyType {
          resolve_options: None,
          resolve_to_context: false,
          dependency_type: DependencyType::Unknown,
          dependency_category: DependencyCategory::Esm,
        }),
    );
  }

  fn get_resolver(&self) -> Arc<Resolver> {
    let lock = self.resolver.lock().expect("should lock");
    lock.clone().expect("init_resolver first")
  }

  fn init_matched_consumes(&self, compilation: &mut Compilation, resolver: Arc<Resolver>) {
    let mut lock = self.matched_consumes.lock().expect("should lock");
    *lock = Some(Arc::new(resolve_matched_configs(
      compilation,
      resolver,
      &self.consumes,
    )));
  }

  fn get_matched_consumes(&self) -> Arc<MatchedConsumes> {
    let lock = self.matched_consumes.lock().expect("should lock");
    lock.clone().expect("init_matched_consumes first")
  }

  async fn create_consume_shared_module(
    &self,
    context: &Context,
    request: &str,
    config: Arc<ConsumeOptions>,
  ) -> ConsumeSharedModule {
    let direct_fallback = matches!(&config.import, Some(i) if RELATIVE_REQUEST.is_match(i) | ABSOLUTE_REQUEST.is_match(i));
    let import_resolved = config
      .import
      .as_ref()
      .and_then(|import| {
        let resolver = self.get_resolver();
        resolver
          .resolve(
            if direct_fallback {
              self.get_context()
            } else {
              context.clone()
            }
            .as_ref(),
            import,
          )
          .ok()
      })
      .and_then(|i| match i {
        ResolveResult::Resource(r) => Some(r.path.to_string_lossy().into_owned()),
        ResolveResult::Ignored => None,
      });
    let required_version = if let Some(version) = config.required_version.as_ref() {
      Some(version.clone())
    } else {
      let package_name = if let Some(name) = &config.package_name {
        Some(name.as_str())
      } else if ABSOLUTE_REQUEST.is_match(request) {
        None
      } else if let Some(caps) = PACKAGE_NAME.captures(request)
        && let Some(mat) = caps.get(0)
      {
        Some(mat.as_str())
      } else {
        None
      };
      if let Some(package_name) = package_name
        && let Some(data) = get_description_file(context.as_ref()).await
      {
        // TODO: emit warning
        get_required_version_from_description_file(data, package_name)
      } else {
        None
      }
    };
    ConsumeSharedModule::new(
      context.clone(),
      ConsumeOptions {
        import: import_resolved
          .is_some()
          .then(|| config.import.clone())
          .and_then(|i| i),
        import_resolved,
        share_key: config.share_key.clone(),
        share_scope: config.share_scope.clone(),
        required_version,
        package_name: config.package_name.clone(),
        strict_version: config.strict_version,
        singleton: config.singleton,
        eager: config.eager,
      },
    )
  }
}

#[async_trait]
impl Plugin for ConsumeSharedPlugin {
  fn name(&self) -> &'static str {
    "rspack.ConsumeSharedPlugin"
  }

  async fn this_compilation(
    &self,
    args: ThisCompilationArgs<'_>,
    params: &CompilationParams,
  ) -> PluginThisCompilationHookOutput {
    args.this_compilation.set_dependency_factory(
      DependencyType::ConsumeSharedFallback,
      params.normal_module_factory.clone(),
    );
    self.init_context(args.this_compilation);
    self.init_resolver(args.this_compilation);
    self.init_matched_consumes(args.this_compilation, self.get_resolver());
    Ok(())
  }

  async fn factorize(
    &self,
    _ctx: PluginContext,
    args: FactorizeArgs<'_>,
  ) -> PluginFactorizeHookOutput {
    let dep = args.dependency;
    if matches!(
      dep.dependency_type(),
      DependencyType::ConsumeSharedFallback | DependencyType::ProvideModuleForShared
    ) {
      return Ok(None);
    }
    let request = dep.request();
    let consumes = self.get_matched_consumes();
    if let Some(matched) = consumes.unresolved.get(request) {
      let module = self
        .create_consume_shared_module(args.context, request, matched.clone())
        .await;
      return Ok(Some(ModuleFactoryResult::new(module.boxed())));
    }
    for (prefix, options) in &consumes.prefixed {
      if request.starts_with(prefix) {
        let remainder = &request[prefix.len()..];
        let module = self
          .create_consume_shared_module(
            args.context,
            request,
            Arc::new(ConsumeOptions {
              import: options.import.as_ref().map(|i| i.to_owned() + remainder),
              import_resolved: options.import_resolved.clone(),
              share_key: options.share_key.clone() + remainder,
              share_scope: options.share_scope.clone(),
              required_version: options.required_version.clone(),
              package_name: options.package_name.clone(),
              strict_version: options.strict_version,
              singleton: options.singleton,
              eager: options.eager,
            }),
          )
          .await;
        return Ok(Some(ModuleFactoryResult::new(module.boxed())));
      }
    }
    Ok(None)
  }

  async fn normal_module_factory_create_module(
    &self,
    _ctx: PluginContext,
    args: &NormalModuleCreateData,
  ) -> PluginNormalModuleFactoryCreateModuleHookOutput {
    if matches!(
      args.dependency_type,
      DependencyType::ConsumeSharedFallback | DependencyType::ProvideModuleForShared
    ) {
      return Ok(None);
    }
    let resource = &args.resource_resolve_data.resource;
    let consumes = self.get_matched_consumes();
    if let Some(options) = consumes.resolved.get(resource) {
      let module = self
        .create_consume_shared_module(&args.context, resource, options.clone())
        .await;
      return Ok(Some(module.boxed()));
    }
    Ok(None)
  }

  fn additional_tree_runtime_requirements(
    &self,
    _ctx: PluginContext,
    args: &mut AdditionalChunkRuntimeRequirementsArgs,
  ) -> PluginAdditionalChunkRuntimeRequirementsOutput {
    args.runtime_requirements.insert(RuntimeGlobals::MODULE);
    args
      .runtime_requirements
      .insert(RuntimeGlobals::MODULE_CACHE);
    args
      .runtime_requirements
      .insert(RuntimeGlobals::MODULE_FACTORIES_ADD_ONLY);
    args
      .runtime_requirements
      .insert(RuntimeGlobals::SHARE_SCOPE_MAP);
    args
      .runtime_requirements
      .insert(RuntimeGlobals::INITIALIZE_SHARING);
    args
      .runtime_requirements
      .insert(RuntimeGlobals::HAS_OWN_PROPERTY);
    args
      .compilation
      .add_runtime_module(args.chunk, Box::<ConsumeSharedRuntimeModule>::default());
    Ok(())
  }
}
