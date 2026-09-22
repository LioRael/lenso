//! Typed extraction from the portable HTTP Endpoint request.

use futures::future::LocalBoxFuture;
use lenso_kernel::InvocationContext;
use serde::de::DeserializeOwned;

use crate::{EndpointHandleInvocationError, HandleRequest, HandleResponse, response};

/// A typed extractor rejection before the authored handler runs.
#[derive(Debug)]
pub enum ExtractorRejection {
    /// Returns one intentional HTTP response.
    Response(HandleResponse),
    /// Preserves a Domain Error or Runtime Failure from asynchronous extraction.
    Invocation(EndpointHandleInvocationError),
}

impl From<HandleResponse> for ExtractorRejection {
    fn from(response: HandleResponse) -> Self {
        Self::Response(response)
    }
}

impl From<EndpointHandleInvocationError> for ExtractorRejection {
    fn from(error: EndpointHandleInvocationError) -> Self {
        Self::Invocation(error)
    }
}

impl From<response::ResponseBuildError> for ExtractorRejection {
    fn from(error: response::ResponseBuildError) -> Self {
        Self::Invocation(error.into())
    }
}

/// Boxed local extraction result used by an authored Endpoint handler.
pub type ExtractorFuture<'a, T> = LocalBoxFuture<'a, Result<T, ExtractorRejection>>;

/// Extracts one typed handler argument from an inbound HTTP request.
///
/// Extractors may inspect the Endpoint provider, await explicitly bound
/// Capability clients, and enrich the invocation context for later extractors
/// and the handler. They must not perform the target Plugin's final business
/// authorization decision.
pub trait FromRequest<P: ?Sized>: Sized {
    /// Extracts this value or rejects dispatch before the handler runs.
    fn from_request<'a>(
        provider: &'a P,
        context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self>;
}

/// A JSON request body decoded into `T`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Json<T>(pub T);

impl<P, T> FromRequest<P> for Json<T>
where
    P: ?Sized,
    T: DeserializeOwned + 'static,
{
    fn from_request<'a>(
        _provider: &'a P,
        _context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(futures::future::ready(extract_json(request)))
    }
}

/// Route path parameters decoded into `T` by field name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Path<T>(pub T);

impl<P, T> FromRequest<P> for Path<T>
where
    P: ?Sized,
    T: DeserializeOwned + 'static,
{
    fn from_request<'a>(
        _provider: &'a P,
        _context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(futures::future::ready(extract_path(request)))
    }
}

/// The URL query parameters decoded into `T`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryParams<T>(pub T);

impl<P, T> FromRequest<P> for QueryParams<T>
where
    P: ?Sized,
    T: DeserializeOwned + 'static,
{
    fn from_request<'a>(
        _provider: &'a P,
        _context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(futures::future::ready(extract_query(request)))
    }
}

/// The trusted request identifier assigned by Web Ingress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestId(pub String);

impl<P> FromRequest<P> for RequestId
where
    P: ?Sized,
{
    fn from_request<'a>(
        _provider: &'a P,
        _context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(futures::future::ready(Ok(Self(request.request_id.clone()))))
    }
}

/// The raw request body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Body(pub crate::Bytes);

impl<P> FromRequest<P> for Body
where
    P: ?Sized,
{
    fn from_request<'a>(
        _provider: &'a P,
        _context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(futures::future::ready(Ok(Self(request.body.clone()))))
    }
}

/// Request headers in arrival order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Headers {
    values: Vec<(String, String)>,
}

impl Headers {
    /// Returns the first header value matching `name`, ignoring ASCII case.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

impl<P> FromRequest<P> for Headers
where
    P: ?Sized,
{
    fn from_request<'a>(
        _provider: &'a P,
        _context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        Box::pin(futures::future::ready(Ok(Self {
            values: request
                .headers
                .iter()
                .map(|header| (header.name.clone(), header.value.clone()))
                .collect(),
        })))
    }
}

impl<P, T> FromRequest<P> for Option<Json<T>>
where
    P: ?Sized,
    T: DeserializeOwned + 'static,
{
    fn from_request<'a>(
        provider: &'a P,
        context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        if request.body.as_ref().is_empty() {
            return Box::pin(futures::future::ready(Ok(None)));
        }
        Box::pin(async move {
            Json::<T>::from_request(provider, context, request)
                .await
                .map(Some)
        })
    }
}

fn extract_json<T>(request: &HandleRequest) -> Result<Json<T>, ExtractorRejection>
where
    T: DeserializeOwned,
{
    if !has_json_content_type(request) {
        return Err(response::problem(
            response::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_content_type_required",
            "The request content type must be application/json.",
        )
        .into());
    }
    serde_json::from_slice(request.body.as_ref())
        .map(Json)
        .map_err(|_| {
            response::problem(
                response::StatusCode::BAD_REQUEST,
                "invalid_json_body",
                "The request body is not valid JSON for this endpoint.",
            )
            .into()
        })
}

fn extract_path<T>(request: &HandleRequest) -> Result<Path<T>, ExtractorRejection>
where
    T: DeserializeOwned,
{
    let parameters = request
        .path_parameters
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter.value.as_str()))
        .collect::<Vec<_>>();
    let encoded = serde_urlencoded::to_string(parameters).map_err(|_| invalid_path())?;
    serde_urlencoded::from_str(&encoded)
        .map(Path)
        .map_err(|_| invalid_path())
}

fn extract_query<T>(request: &HandleRequest) -> Result<QueryParams<T>, ExtractorRejection>
where
    T: DeserializeOwned,
{
    serde_urlencoded::from_str(request.query.as_deref().unwrap_or_default())
        .map(QueryParams)
        .map_err(|_| {
            response::problem(
                response::StatusCode::BAD_REQUEST,
                "invalid_query",
                "The query string is not valid for this endpoint.",
            )
            .into()
        })
}

fn has_json_content_type(request: &HandleRequest) -> bool {
    request.headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("content-type")
            && header
                .value
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn invalid_path() -> ExtractorRejection {
    response::problem(
        response::StatusCode::BAD_REQUEST,
        "invalid_path_parameters",
        "The route path parameters are not valid for this endpoint.",
    )
    .into()
}

#[cfg(test)]
mod tests {
    use lenso_kernel::{CancellationToken, InvocationContext};
    use serde::Deserialize;

    use super::*;
    use crate::{Bytes, HandleRequest, HandleRequestHeadersItem};

    #[derive(Debug, Deserialize, PartialEq)]
    struct Payload {
        name: String,
    }

    fn context() -> InvocationContext {
        InvocationContext::new(1, None, CancellationToken::new())
    }

    fn request(body: &[u8], content_type: Option<&str>) -> HandleRequest {
        HandleRequest {
            body: Bytes::from(body.to_vec()),
            credential: None,
            headers: content_type
                .map(|value| {
                    vec![HandleRequestHeadersItem {
                        name: "content-type".to_owned(),
                        value: value.to_owned(),
                    }]
                })
                .unwrap_or_default(),
            method: "POST".to_owned(),
            path: "/items".to_owned(),
            path_parameters: Vec::new(),
            query: None,
            request_id: "extract-1".to_owned(),
            route_id: "items.create".to_owned(),
        }
    }

    #[test]
    fn body_and_headers_copy_the_portable_request() {
        let request = request(b"raw", Some("text/plain"));
        let mut context = context();
        let body =
            futures::executor::block_on(Body::from_request(&(), &mut context, &request)).unwrap();
        let headers =
            futures::executor::block_on(Headers::from_request(&(), &mut context, &request))
                .unwrap();
        assert_eq!(body.0.as_ref(), b"raw");
        assert_eq!(headers.get("Content-Type"), Some("text/plain"));
    }

    #[test]
    fn optional_json_treats_an_empty_body_as_absent() {
        let request = request(b"", None);
        let mut context = context();
        let value = futures::executor::block_on(Option::<Json<Payload>>::from_request(
            &(),
            &mut context,
            &request,
        ))
        .unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn optional_json_still_rejects_invalid_payloads() {
        let request = request(b"{", Some("application/json"));
        let mut context = context();
        let error = futures::executor::block_on(Option::<Json<Payload>>::from_request(
            &(),
            &mut context,
            &request,
        ))
        .unwrap_err();
        assert!(matches!(error, ExtractorRejection::Response(_)));
    }
}
