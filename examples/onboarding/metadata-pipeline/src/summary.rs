use std::collections::BTreeMap;

use crate::metadata;
use lenso::Port;
use lenso_capability_http_endpoint::{
    prelude::*,
    response::{Problem, StatusCode},
};
use lenso_kernel::InvocationContext;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileMetadata {
    name: String,
    size_bytes: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    files: Vec<FileMetadata>,
}

#[derive(Debug, Serialize)]
struct Summary {
    files: Vec<metadata::NormalizeResponse>,
    count: usize,
    total_bytes: i64,
    by_extension: BTreeMap<String, usize>,
}

/// Owns aggregation and the HTTP entrypoint; normalization is a required Port.
#[lenso::plugin(id = "example.metadata-summary", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {
    metadata: Port<metadata::MetadataClient>,
}

#[endpoint]
impl Plugin {
    #[post("metadata.summarize", "/metadata/summary")]
    async fn summarize(
        &self,
        context: InvocationContext,
        Json(batch): Json<Batch>,
    ) -> Result<Json<Summary>, Problem> {
        if batch.files.len() > 100 {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "batch_too_large",
                "provide at most 100 synthetic metadata records",
            ));
        }
        let mut files = Vec::with_capacity(batch.files.len());
        let mut total_bytes = 0_i64;
        let mut by_extension = BTreeMap::new();
        for file in batch.files {
            let normalized = self
                .metadata
                .normalize_with_context(
                    context.clone(),
                    metadata::NormalizeRequest {
                        name: file.name,
                        size_bytes: file.size_bytes,
                    },
                )
                .await
                .map_err(|failure| match failure {
                    metadata::MetadataInvocationError::Domain(error) => Problem::new(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "invalid_metadata",
                        format!("normalizer rejected metadata: {error:?}"),
                    ),
                    metadata::MetadataInvocationError::Runtime(_) => Problem::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "normalizer_unavailable",
                        "the metadata normalizer is unavailable",
                    ),
                })?;
            total_bytes = total_bytes
                .checked_add(normalized.size_bytes)
                .ok_or_else(|| {
                    Problem::new(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "total_overflow",
                        "aggregate size exceeds the supported integer range",
                    )
                })?;
            *by_extension
                .entry(normalized.extension.clone())
                .or_default() += 1;
            files.push(normalized);
        }
        Ok(Json(Summary {
            count: files.len(),
            files,
            total_bytes,
            by_extension,
        }))
    }
}
