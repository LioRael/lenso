use lenso_auth_sdk::{
    ActorAssertionVerifier, AuthOutcome, CredentialEvidence, authenticate_request,
    decode_auth_response,
};
use lenso_capability_http_endpoint::{ExtractorFuture, FromRequest, HandleRequest, prelude::*};
use lenso_kernel::InvocationContext;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, lenso::PluginConfig)]
#[serde(deny_unknown_fields)]
pub struct ProtectedConfig {
    pub issuer: String,
    pub public_key: String,
}

#[lenso::plugin(id = "example.protected", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {
    #[config]
    config: ProtectedConfig,
    auth: lenso::Port<lenso_capability_auth::AuthClient>,
}

pub struct Credential(Option<CredentialEvidence>);
impl FromRequest<Plugin> for Credential {
    fn from_request<'a>(
        _: &'a Plugin,
        _: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(async move {
            Ok(Self(request.credential.as_ref().map(|value| {
                CredentialEvidence::new(&value.scheme, &value.value)
            })))
        })
    }
}

#[endpoint]
impl Plugin {
    #[get("protected", "/protected")]
    async fn protected(
        &self,
        context: InvocationContext,
        credential: Credential,
    ) -> Result<Json<serde_json::Value>, Problem> {
        let denied = || {
            Problem::new(
                StatusCode::UNAUTHORIZED,
                "authentication_required",
                "Authentication required",
            )
        };
        let response = self
            .auth
            .authenticate_with_context(context, authenticate_request(credential.0))
            .await
            .map_err(|_| denied())?;
        let AuthOutcome::Authenticated(assertion) =
            decode_auth_response(response).map_err(|_| denied())?
        else {
            return Err(denied());
        };
        ActorAssertionVerifier::from_public_key_base64(
            &self.config.issuer,
            &self.config.public_key,
        )
        .map_err(|_| denied())?
        .verify_for(
            &assertion,
            "example.protected:read",
            time::OffsetDateTime::now_utc(),
        )
        .map_err(|_| denied())?;
        if assertion.actor_kind() != "user" {
            return Err(denied());
        }
        Ok(Json(serde_json::json!({"subject": assertion.subject()})))
    }
}
