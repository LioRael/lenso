use std::path::PathBuf;

use lenso_capability_configuration_source::{
    FetchError, FetchRequest, FetchResponse, FetchResponseConfigurationsItem, SourceProvider,
};
use lenso_engine_authoring::{
    FilePluginConfigurationSnapshotSource, PluginConfigurationAuthoritySource,
};
use lenso_plugin_sdk::{CreateContext, Ctx, Plugin};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    path: PathBuf,
}

struct FileSource {
    path: PathBuf,
}

impl Plugin for FileSource {
    const CONFIGURATION_SCHEMA: Option<&'static str> = Some(include_str!("../config.schema.json"));

    fn create(context: CreateContext) -> Result<Self, String> {
        let configuration: Configuration = serde_json::from_value(context.config().clone())
            .map_err(|_| "invalid file source configuration".to_owned())?;
        if !configuration.path.is_absolute() {
            return Err("file source path must be absolute".to_owned());
        }
        Ok(Self {
            path: configuration.path,
        })
    }
}

impl SourceProvider for FileSource {
    fn fetch(&self, _: Ctx, _: FetchRequest) -> Result<FetchResponse, FetchError> {
        // This identity is local parsing context only. The Host disregards it
        // and binds its own source identity and writable field scopes.
        let context = PluginConfigurationAuthoritySource::new("bootstrap_plugin", "file")
            .map_err(|_| FetchError::InvalidSource)?;
        let snapshot = FilePluginConfigurationSnapshotSource::new(&self.path, context)
            .read()
            .map_err(|_| FetchError::InvalidSource)?;
        Ok(FetchResponse {
            revision: snapshot.revision().to_string(),
            configurations: snapshot
                .configurations()
                .iter()
                .map(|value| FetchResponseConfigurationsItem {
                    plugin_id: value.plugin_id().to_owned(),
                    instance_key: value.instance_key().to_owned(),
                    toml: value.toml().to_owned(),
                })
                .collect(),
        })
    }
}

lenso_capability_configuration_source::export_source_plugin!(FileSource);
