use crate::metadata;
use lenso_kernel::InvocationContext;

/// Owns filename validation and normalization; it never reads the filesystem.
#[lenso::plugin(id = "example.metadata-normalizer", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {}

#[lenso::provides(metadata::Metadata)]
impl Plugin {
    async fn normalize(
        &self,
        _context: InvocationContext,
        request: metadata::NormalizeRequest,
    ) -> Result<metadata::NormalizeResponse, metadata::NormalizeError> {
        let name = request.name.trim();
        if name.is_empty()
            || name.len() > 255
            || name
                .chars()
                .any(|value| value.is_control() || value == '/' || value == '\\')
        {
            return Err(metadata::NormalizeError::InvalidName);
        }
        if request.size_bytes < 0 {
            return Err(metadata::NormalizeError::InvalidSize);
        }
        let extension = name
            .rsplit_once('.')
            .filter(|(stem, suffix)| !stem.is_empty() && !suffix.is_empty())
            .map_or_else(|| "none".to_owned(), |(_, suffix)| suffix.to_lowercase());
        Ok(metadata::NormalizeResponse {
            name: name.to_owned(),
            extension,
            size_bytes: request.size_bytes,
        })
    }
}
