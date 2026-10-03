//! One source Plugin, one Instance and the existing HTTP Endpoint macros.
use lenso_capability_http_endpoint::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Greet {
    name: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Greeting {
    message: String,
}

#[lenso::plugin]
#[derive(Clone, Debug, Default)]
pub struct GreetingHttp {}

#[endpoint]
impl GreetingHttp {
    #[post("greeting.create", "/greet")]
    async fn greet(&self, Json(input): Json<Greet>) -> Result<Json<Greeting>, Problem> {
        let name = input.name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_name",
                "name must contain 1–80 characters after trimming",
            ));
        }
        Ok(Json(Greeting {
            message: format!("Hello, {name}!"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_capability_http_endpoint::testing::EndpointTest;

    #[tokio::test(flavor = "current_thread")]
    async fn trims_valid_names_and_rejects_empty_or_overlong_names() {
        let endpoint = EndpointTest::new(GreetingHttp::default());
        for (name, status) in [
            ("  Lenso  ".to_owned(), StatusCode::OK),
            ("   ".to_owned(), StatusCode::BAD_REQUEST),
            ("界".repeat(81), StatusCode::BAD_REQUEST),
        ] {
            let response = endpoint
                .request("greeting.create")
                .json(&Greet { name })
                .unwrap()
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            if status == StatusCode::OK {
                assert_eq!(
                    response.json::<Greeting>().unwrap().message,
                    "Hello, Lenso!"
                );
            }
        }
    }
}
