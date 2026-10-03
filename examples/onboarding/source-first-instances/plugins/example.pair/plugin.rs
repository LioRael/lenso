use crate::metadata;
use lenso_capability_http_endpoint::prelude::*;
use lenso_kernel::InvocationContext;

#[lenso::plugin(id = "example.pair", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {
    #[dependency(id = "left")]
    left: metadata::MetadataClient,
    #[dependency(id = "right")]
    right: metadata::MetadataClient,
}

#[endpoint]
impl Plugin {
    #[get("pair", "/instances")]
    async fn pair(&self, context: InvocationContext) -> Result<Json<Vec<String>>, Problem> {
        let request = metadata::NormalizeRequest {
            name: "hello".into(),
            size_bytes: 0,
        };
        let left = self
            .left
            .normalize_with_context(context.clone(), request.clone())
            .await;
        let right = self.right.normalize_with_context(context, request).await;
        match (left, right) {
            (Ok(left), Ok(right)) => Ok(Json(vec![left.name, right.name])),
            _ => Err(Problem::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "label_unavailable",
                "label Plugin unavailable",
            )),
        }
    }
}
