//! Copyable `#[plugin]` + `#[endpoint]` Web backend example.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use lenso_capability_http_endpoint::{
    prelude::*,
    response::{Problem, StatusCode},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateGreeting {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GreetingPath {
    greeting_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Greeting {
    id: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_revision: Option<u64>,
}

/// Plugin-owned shape for one Host-authorized business object.
#[derive(Clone, Debug, Deserialize, lenso::PluginConfig)]
#[serde(deny_unknown_fields)]
pub struct GreetingPolicy {
    pub exclamation_count: u8,
}

/// A request-pinned policy value; another Host publication cannot alter it.
pub struct PinnedGreetingPolicy {
    pub revision: u64,
    pub value: GreetingPolicy,
}

impl std::fmt::Debug for PinnedGreetingPolicy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PinnedGreetingPolicy")
            .field("revision", &self.revision)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// The Host supplies this private binding only when it selects a dynamic source.
pub trait GreetingPolicySource {
    fn capture(&self) -> Result<PinnedGreetingPolicy, ()>;
}

#[lenso::plugin]
#[derive(Clone, Default)]
pub struct GreetingsHttp {
    // `HttpEndpoint` dispatch clones the provider; `Rc` keeps one store.
    next_id: Rc<Cell<u64>>,
    greetings: Rc<RefCell<BTreeMap<String, Greeting>>>,
    policy: Option<Rc<dyn GreetingPolicySource>>,
}

impl std::fmt::Debug for GreetingsHttp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GreetingsHttp")
            .field("business_policy_bound", &self.policy.is_some())
            .finish_non_exhaustive()
    }
}

impl GreetingsHttp {
    /// Binds one Host-owned authority without changing the resolved App Plan.
    pub fn bind_business_policy(&mut self, source: Rc<dyn GreetingPolicySource>) {
        self.policy = Some(source);
    }
}

#[endpoint]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl GreetingsHttp {
    #[post("greetings.create", "/greetings")]
    async fn create(
        &self,
        Json(input): Json<CreateGreeting>,
    ) -> Result<(StatusCode, Json<Greeting>), Problem> {
        let policy = self
            .policy
            .as_ref()
            .map(|source| source.capture())
            .transpose()
            .map_err(|()| {
                Problem::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "business_policy_unavailable",
                    "the authorized business policy is unavailable",
                )
            })?;
        let name = input.name.trim();
        if name.is_empty() {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_name",
                "name must not be empty",
            ));
        }

        let sequence = self.next_id.get() + 1;
        self.next_id.set(sequence);
        let greeting = Greeting {
            id: format!("greeting-{sequence}"),
            message: format!(
                "Hello, {name}{}",
                "!".repeat(
                    policy
                        .as_ref()
                        .map_or(1, |policy| usize::from(policy.value.exclamation_count))
                )
            ),
            policy_revision: policy.map(|policy| policy.revision),
        };
        self.greetings
            .borrow_mut()
            .insert(greeting.id.clone(), greeting.clone());
        Ok((StatusCode::CREATED, Json(greeting)))
    }

    #[get("greetings.read", "/greetings/{greeting_id}")]
    async fn read(&self, Path(path): Path<GreetingPath>) -> Result<Json<Greeting>, Problem> {
        self.greetings
            .borrow()
            .get(&path.greeting_id)
            .cloned()
            .map(Json)
            .ok_or_else(|| {
                Problem::new(
                    StatusCode::NOT_FOUND,
                    "greeting_not_found",
                    "the greeting does not exist",
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use lenso_capability_http_endpoint::testing::EndpointTest;

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn creates_and_reads_a_greeting_without_opening_a_socket() {
        let endpoint = EndpointTest::new(GreetingsHttp::default());
        let created = endpoint
            .request("greetings.create")
            .json(&CreateGreeting {
                name: "Lenso".to_owned(),
            })
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let greeting = created.json::<Greeting>().unwrap();

        let read = endpoint
            .request("greetings.read")
            .path_parameter("greeting_id", &greeting.id)
            .send()
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK);
        assert_eq!(read.json::<Greeting>().unwrap(), greeting);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn turns_business_rejections_into_problem_responses() {
        let response = EndpointTest::new(GreetingsHttp::default())
            .request("greetings.create")
            .json(&CreateGreeting {
                name: String::new(),
            })
            .unwrap()
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.header("content-type"),
            Some("application/problem+json; charset=utf-8")
        );
    }
}
