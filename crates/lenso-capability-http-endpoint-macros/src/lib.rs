//! Attribute authoring for statically routed Lenso HTTP Endpoint providers.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use serde_json::{Map, Number, Value};
use syn::{
    Attribute, Error, FnArg, GenericArgument, Ident, ImplItem, ItemImpl, Lit, LitInt, LitStr, Meta,
    PathArguments, Result, ReturnType, Token, Type, braced, bracketed,
    ext::IdentExt,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
};

/// Generates one HTTP Endpoint provider from handler route attributes.
///
/// Supported handler attributes are `get`, `post`, `put`, `patch`, `delete`,
/// `head`, `options`, and `query`. Each accepts a stable route ID and path. One or more
/// `middleware` attributes may name async provider methods that run before the
/// handler and its typed request extractors. Middleware on the impl applies to
/// every route before route-specific middleware. A handler that already has an
/// `#[openapi(...)]` Operation Object may opt into strict type-derived drift
/// detection with `#[openapi_contract(...)]`.
#[proc_macro_attribute]
pub fn endpoint(arguments: TokenStream, input: TokenStream) -> TokenStream {
    let register_plugin = if arguments.is_empty() {
        true
    } else {
        let mode = parse_macro_input!(arguments as Ident);
        if mode != "standalone" {
            return Error::new_spanned(
                mode,
                "endpoint accepts only the internal `standalone` mode",
            )
            .into_compile_error()
            .into();
        }
        false
    };

    let implementation = parse_macro_input!(input as ItemImpl);
    expand_endpoint_with_registration(implementation, register_plugin)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Converts a JSON-like `OpenAPI` Operation Object into a validated string literal.
///
/// This is primarily useful with `lenso_capability_http_endpoint::http_endpoint!`.
/// Handler attributes accept the same object directly through `#[openapi({ ... })]`.
#[proc_macro]
pub fn openapi_operation(input: TokenStream) -> TokenStream {
    let operation = parse_macro_input!(input as OpenApiObject);
    match operation_literal(&operation) {
        Ok(operation) => quote!(#operation).into(),
        Err(error) => error.into_compile_error().into(),
    }
}

#[cfg(test)]
fn expand_endpoint(implementation: ItemImpl) -> Result<proc_macro2::TokenStream> {
    expand_endpoint_with_registration(implementation, true)
}

fn validate_endpoint_implementation(implementation: &ItemImpl) -> Result<()> {
    if implementation.trait_.is_some() {
        return Err(Error::new_spanned(
            implementation.impl_token,
            "endpoint can only be applied to an inherent impl block",
        ));
    }
    if !implementation.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &implementation.generics,
            "endpoint does not support generic impl blocks",
        ));
    }
    Ok(())
}

fn expand_endpoint_with_registration(
    mut implementation: ItemImpl,
    register_plugin: bool,
) -> Result<proc_macro2::TokenStream> {
    validate_endpoint_implementation(&implementation)?;

    let provider = implementation.self_ty.clone();
    let provider_middlewares = take_provider_middlewares(&mut implementation.attrs)?;
    let mut routes = Vec::new();
    for item in &mut implementation.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        let metadata = take_handler_metadata(&mut method.attrs)?;
        if let Some(mut route) = metadata.route {
            if metadata.contract.is_some() && metadata.openapi.is_none() {
                return Err(Error::new_spanned(
                    &method.sig.ident,
                    "openapi_contract requires an OpenAPI Operation Object on the same handler",
                ));
            }
            let contract = metadata
                .contract
                .as_ref()
                .map(|contract| derive_endpoint_contract(method, contract))
                .transpose()?;
            route.openapi = metadata.openapi;
            routes.push(Handler {
                route,
                middlewares: provider_middlewares
                    .iter()
                    .cloned()
                    .chain(metadata.middlewares)
                    .collect(),
                method: method.sig.ident.clone(),
                arguments: handler_arguments(method)?,
                contract,
            });
        } else if !metadata.middlewares.is_empty()
            || metadata.openapi.is_some()
            || metadata.contract.is_some()
        {
            return Err(Error::new_spanned(
                &method.sig.ident,
                "endpoint metadata can only be attached to an HTTP handler",
            ));
        }
    }
    if routes.is_empty() {
        return Err(Error::new_spanned(
            &implementation.self_ty,
            "endpoint impl must declare at least one HTTP handler attribute",
        ));
    }

    let const_routes = routes
        .iter()
        .enumerate()
        .map(|(index, handler)| endpoint_route(&provider, handler, index));
    let implementation_routes = routes
        .iter()
        .enumerate()
        .map(|(index, handler)| endpoint_route(&provider, handler, index));
    let dispatch_arms = routes.iter().map(dispatch_arm);
    let contract_impl = endpoint_contract_impl(&provider, &routes);
    let plugin_registration =
        register_plugin.then(|| quote!(#[lenso::provides(http_endpoint_contract::Endpoint)]));

    Ok(quote! {
        #implementation

        #contract_impl

        const _: () = {
            const ROUTES: &[::lenso_capability_http_endpoint::EndpointRoute] = &[
                #(#const_routes)*
            ];
            ::lenso_capability_http_endpoint::__private::validate_endpoint_routes(ROUTES);
        };

        #plugin_registration
        impl ::lenso_capability_http_endpoint::HttpEndpoint for #provider {
            const ROUTES: &'static [::lenso_capability_http_endpoint::EndpointRoute] = &[
                #(#implementation_routes)*
            ];

            fn dispatch(
                &self,
                context: ::lenso_capability_http_endpoint::__private::InvocationContext,
                request: ::lenso_capability_http_endpoint::HandleRequest,
            ) -> ::lenso_capability_http_endpoint::EndpointFuture {
                let provider = self.clone();
                Box::pin(async move {
                    let route_id = request.route_id.clone();
                    match route_id.as_str() {
                        #(#dispatch_arms,)*
                        _ => Ok(Err(::lenso_capability_http_endpoint::HandleError::Rejected)),
                    }
                })
            }
        }
    })
}

fn endpoint_route(provider: &Type, handler: &Handler, index: usize) -> proc_macro2::TokenStream {
    let route_id = &handler.route.id;
    let method = &handler.route.method;
    let path = &handler.route.path;
    let openapi = handler
        .route
        .openapi
        .as_ref()
        .map(|operation| quote!(.with_openapi(#operation)));
    let contract = handler.contract.as_ref().map(|_| {
        let factory = contract_factory_name(handler, index);
        quote!(.with_openapi_contract(<#provider>::#factory))
    });
    quote! {
        ::lenso_capability_http_endpoint::EndpointRoute::new(
            #route_id,
            #method,
            #path,
        ) #openapi #contract,
    }
}

fn take_provider_middlewares(attributes: &mut Vec<Attribute>) -> Result<Vec<Ident>> {
    let mut middlewares = Vec::new();
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("middleware") {
            let arguments =
                attribute.parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)?;
            if arguments.is_empty() {
                return Err(Error::new_spanned(
                    attribute,
                    "middleware requires at least one provider method",
                ));
            }
            middlewares.extend(arguments);
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(middlewares)
}

fn take_handler_metadata(attributes: &mut Vec<Attribute>) -> Result<HandlerMetadata> {
    let mut route = None;
    let mut middlewares = Vec::new();
    let mut openapi = None;
    let mut contract = None;
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("middleware") {
            let arguments =
                attribute.parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)?;
            if arguments.is_empty() {
                return Err(Error::new_spanned(
                    attribute,
                    "middleware requires at least one provider method",
                ));
            }
            middlewares.extend(arguments);
            continue;
        }
        if attribute.path().is_ident("openapi") {
            if openapi.is_some() {
                return Err(Error::new_spanned(
                    attribute,
                    "an endpoint handler may declare only one OpenAPI Operation Object",
                ));
            }
            let operation = attribute.parse_args::<OpenApiOperation>()?.into_literal()?;
            openapi = Some(operation);
            continue;
        }
        if attribute.path().is_ident("openapi_contract") {
            if contract.is_some() {
                return Err(Error::new_spanned(
                    attribute,
                    "an endpoint handler may declare only one strict OpenAPI contract",
                ));
            }
            contract = Some(match &attribute.meta {
                Meta::Path(_) => OpenApiContractMetadata::default(),
                _ => attribute.parse_args::<OpenApiContractMetadata>()?,
            });
            continue;
        }
        let Some(http_method) = http_method(&attribute) else {
            retained.push(attribute);
            continue;
        };
        if route.is_some() {
            return Err(Error::new_spanned(
                attribute,
                "an endpoint handler may declare only one HTTP method",
            ));
        }
        let arguments = attribute.parse_args::<RouteArguments>()?;
        route = Some(Route {
            method: LitStr::new(http_method, attribute.path().span()),
            id: arguments.route_id,
            path: arguments.path,
            openapi: None,
        });
    }
    *attributes = retained;
    Ok(HandlerMetadata {
        route,
        middlewares,
        openapi,
        contract,
    })
}

enum OpenApiOperation {
    Json(LitStr),
    Object(OpenApiObject),
}

#[derive(Default)]
struct OpenApiContractMetadata {
    success: Option<LitInt>,
    errors: Vec<OpenApiContractError>,
}

impl Parse for OpenApiContractMetadata {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut metadata = Self::default();
        let mut errors_seen = false;
        while !input.is_empty() {
            let key = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "success" => {
                    if metadata.success.is_some() {
                        return Err(Error::new(key.span(), "duplicate openapi_contract success"));
                    }
                    metadata.success = Some(input.parse()?);
                }
                "errors" => {
                    if errors_seen {
                        return Err(Error::new(key.span(), "duplicate openapi_contract errors"));
                    }
                    errors_seen = true;
                    let content;
                    bracketed!(content in input);
                    metadata.errors =
                        Punctuated::<OpenApiContractError, Token![,]>::parse_terminated(&content)?
                            .into_iter()
                            .collect();
                }
                _ => {
                    return Err(Error::new(
                        key.span(),
                        "openapi_contract accepts only `success` and `errors`",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(metadata)
    }
}

struct OpenApiContractError {
    status: LitInt,
    code: LitStr,
}

impl Parse for OpenApiContractError {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let status = content.parse()?;
        content.parse::<Token![,]>()?;
        let code = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("a strict OpenAPI error is `(status, \"code\")`"));
        }
        Ok(Self { status, code })
    }
}

impl OpenApiOperation {
    fn into_literal(self) -> Result<LitStr> {
        match self {
            Self::Json(operation) => {
                validate_openapi_operation(&operation)?;
                Ok(operation)
            }
            Self::Object(operation) => operation_literal(&operation),
        }
    }
}

impl Parse for OpenApiOperation {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(LitStr) {
            return input.parse().map(Self::Json);
        }
        input.parse().map(Self::Object)
    }
}

struct OpenApiObject {
    value: Map<String, Value>,
    span: proc_macro2::Span,
}

impl Parse for OpenApiObject {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let content;
        let brace = braced!(content in input);
        Ok(Self {
            value: parse_object_entries(&content)?,
            span: brace.span.join(),
        })
    }
}

fn parse_object_entries(input: ParseStream<'_>) -> Result<Map<String, Value>> {
    let mut object = Map::new();
    while !input.is_empty() {
        let (key, span) = parse_object_key(input)?;
        input.parse::<Token![:]>()?;
        let value = parse_json_value(input)?;
        if object.insert(key.clone(), value).is_some() {
            return Err(Error::new(
                span,
                format!("duplicate OpenAPI object key `{key}`"),
            ));
        }
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(object)
}

fn parse_object_key(input: ParseStream<'_>) -> Result<(String, proc_macro2::Span)> {
    if input.peek(LitStr) {
        let key = input.parse::<LitStr>()?;
        return Ok((key.value(), key.span()));
    }
    let key = Ident::parse_any(input)?;
    Ok((key.unraw().to_string(), key.span()))
}

fn parse_json_value(input: ParseStream<'_>) -> Result<Value> {
    if input.peek(syn::token::Brace) {
        let content;
        braced!(content in input);
        return parse_object_entries(&content).map(Value::Object);
    }
    if input.peek(syn::token::Bracket) {
        let content;
        bracketed!(content in input);
        let values = Punctuated::<JsonValue, Token![,]>::parse_terminated(&content)?;
        return Ok(Value::Array(
            values.into_iter().map(|value| value.0).collect(),
        ));
    }
    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let literal = input.parse::<Lit>()?;
        return parse_number_literal(&literal, true);
    }
    if input.peek(syn::LitBool) {
        let literal = input.parse::<Lit>()?;
        return parse_literal(&literal);
    }
    if input.peek(Ident::peek_any) {
        let ident = Ident::parse_any(input)?;
        if ident == "null" {
            return Ok(Value::Null);
        }
        return Err(Error::new(ident.span(), "expected a JSON value"));
    }
    let literal = input.parse::<Lit>()?;
    parse_literal(&literal)
}

struct JsonValue(Value);

impl Parse for JsonValue {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        parse_json_value(input).map(Self)
    }
}

fn parse_literal(literal: &Lit) -> Result<Value> {
    match literal {
        Lit::Str(value) => Ok(Value::String(value.value())),
        Lit::Bool(value) => Ok(Value::Bool(value.value)),
        Lit::Int(_) | Lit::Float(_) => parse_number_literal(literal, false),
        _ => Err(Error::new(
            literal.span(),
            "OpenAPI metadata supports JSON string, number, boolean, null, array, and object values",
        )),
    }
}

fn parse_number_literal(literal: &Lit, negative: bool) -> Result<Value> {
    let mut source = literal.to_token_stream().to_string().replace(' ', "");
    if negative {
        source.insert(0, '-');
    }
    let number = source.parse::<Number>().map_err(|_| {
        Error::new(
            literal.span(),
            "OpenAPI numeric values must be unsuffixed JSON numbers",
        )
    })?;
    Ok(Value::Number(number))
}

fn operation_literal(operation: &OpenApiObject) -> Result<LitStr> {
    if operation.value.contains_key("operationId") {
        return Err(Error::new(
            operation.span,
            "OpenAPI operationId is generated from the stable route ID",
        ));
    }
    reject_reserved_openapi_keys(&operation.value, operation.span)?;
    let json = serde_json::to_string(&operation.value).map_err(|error| {
        Error::new(
            operation.span,
            format!("could not encode OpenAPI Operation Object: {error}"),
        )
    })?;
    Ok(LitStr::new(&json, operation.span))
}

fn validate_openapi_operation(operation: &LitStr) -> Result<()> {
    let value = serde_json::from_str::<serde_json::Value>(&operation.value()).map_err(|error| {
        Error::new(
            operation.span(),
            format!("OpenAPI Operation Object is not valid JSON: {error}"),
        )
    })?;
    let Some(object) = value.as_object() else {
        return Err(Error::new(
            operation.span(),
            "OpenAPI operation metadata must be a JSON object",
        ));
    };
    if object.contains_key("operationId") {
        return Err(Error::new(
            operation.span(),
            "OpenAPI operationId is generated from the stable route ID",
        ));
    }
    reject_reserved_openapi_keys(object, operation.span())?;
    Ok(())
}

fn reject_reserved_openapi_keys(
    operation: &Map<String, Value>,
    span: proc_macro2::Span,
) -> Result<()> {
    if operation.contains_key("x-lenso-contract") {
        return Err(Error::new(
            span,
            "x-lenso-contract is reserved for generated strict OpenAPI contract checks",
        ));
    }
    Ok(())
}

fn endpoint_contract_impl(provider: &Type, routes: &[Handler]) -> Option<proc_macro2::TokenStream> {
    let contract_helpers = routes
        .iter()
        .enumerate()
        .filter_map(|(index, handler)| {
            handler
                .contract
                .as_ref()
                .map(|contract| contract_factory(handler, contract, index))
        })
        .collect::<Vec<_>>();
    (!contract_helpers.is_empty()).then(|| {
        quote! {
            impl #provider {
                #(#contract_helpers)*
            }
        }
    })
}

fn contract_factory(
    handler: &Handler,
    contract: &EndpointContract,
    index: usize,
) -> proc_macro2::TokenStream {
    let factory = contract_factory_name(handler, index);
    let route_id = &handler.route.id;
    let method = &handler.route.method;
    let path = &handler.route.path;
    let path_step = contract
        .path
        .as_ref()
        .map(|ty| quote!(contract = contract.path::<#ty>()?;));
    let query_step = contract
        .query
        .as_ref()
        .map(|ty| quote!(contract = contract.query::<#ty>()?;));
    let body_step = contract.body.as_ref().map(|body| {
        let ty = &body.ty;
        if body.required {
            quote!(contract = contract.json_body::<#ty>()?;)
        } else {
            quote!(contract = contract.optional_json_body::<#ty>()?;)
        }
    });
    let success = &contract.success;
    let success_status = contract.success_status;
    let problems = contract.problems.iter().map(|problem| {
        let status = problem.status;
        let code = &problem.code;
        quote!(contract = contract.known_problem(#status, #code)?;)
    });

    quote! {
        fn #factory() -> ::lenso_capability_http_endpoint::OpenApiContractResult {
            let mut contract = ::lenso_capability_http_endpoint::OpenApiContract::new(
                #route_id,
                #method,
                #path,
            );
            #path_step
            #query_step
            #body_step
            contract = contract.success_json::<#success>(#success_status)?;
            #(#problems)*
            contract.build()
        }
    }
}

fn contract_factory_name(handler: &Handler, index: usize) -> Ident {
    Ident::new(
        &format!("__lenso_openapi_contract_{}_{}", handler.method, index),
        handler.method.span(),
    )
}

fn derive_endpoint_contract(
    method: &syn::ImplItemFn,
    metadata: &OpenApiContractMetadata,
) -> Result<EndpointContract> {
    let RequestContractShape {
        path,
        query,
        body,
        mut problems,
    } = derive_strict_request_shape(method)?;

    let (success, error) = endpoint_result_types(method)?;
    let (success, success_status) = strict_success_type(success, metadata.success.as_ref())?;
    extend_problem_contract(error, metadata, &mut problems)?;

    Ok(EndpointContract {
        path,
        query,
        body,
        success,
        success_status,
        problems,
    })
}

fn derive_strict_request_shape(method: &syn::ImplItemFn) -> Result<RequestContractShape> {
    let mut path = None;
    let mut query = None;
    let mut body = None;
    let mut problems = Vec::new();

    for input in &method.sig.inputs {
        let FnArg::Typed(argument) = input else {
            continue;
        };
        let ty = argument.ty.as_ref();
        match final_type_ident(ty).map(Ident::to_string).as_deref() {
            Some("InvocationContext" | "RequestId") => {}
            Some("Path") => {
                if path.is_some() {
                    return Err(Error::new_spanned(
                        ty,
                        "a strict OpenAPI contract supports one Path<T> extractor",
                    ));
                }
                path = Some(extractor_type_argument(ty, "Path")?);
                problems.push(ContractProblem::generated(
                    400,
                    "invalid_path_parameters",
                    ty.span(),
                ));
            }
            Some("QueryParams") => {
                if query.is_some() {
                    return Err(Error::new_spanned(
                        ty,
                        "a strict OpenAPI contract supports one QueryParams<T> extractor",
                    ));
                }
                query = Some(extractor_type_argument(ty, "QueryParams")?);
                problems.push(ContractProblem::generated(400, "invalid_query", ty.span()));
            }
            Some("Json") => {
                if body.is_some() {
                    return Err(Error::new_spanned(
                        ty,
                        "a strict OpenAPI contract supports one Json<T> request body",
                    ));
                }
                body = Some(ContractBody {
                    ty: extractor_type_argument(ty, "Json")?,
                    required: true,
                });
                add_json_extractor_problems(&mut problems, ty.span());
            }
            Some("Option") => {
                let inner = extractor_type_argument(ty, "Option")?;
                if final_type_ident(&inner).is_some_and(|ident| ident == "Json") {
                    if body.is_some() {
                        return Err(Error::new_spanned(
                            ty,
                            "a strict OpenAPI contract supports one Json<T> request body",
                        ));
                    }
                    body = Some(ContractBody {
                        ty: extractor_type_argument(&inner, "Json")?,
                        required: false,
                    });
                    add_json_extractor_problems(&mut problems, ty.span());
                } else {
                    return Err(Error::new_spanned(
                        ty,
                        "strict OpenAPI contracts support Option only around Json<T>",
                    ));
                }
            }
            Some("Body" | "Headers" | "HandleRequest") => {
                return Err(Error::new_spanned(
                    ty,
                    "strict OpenAPI contracts require typed Path, QueryParams, and Json values instead of raw request data",
                ));
            }
            _ => {
                return Err(Error::new_spanned(
                    ty,
                    "strict OpenAPI contracts support InvocationContext, RequestId, Path<T>, QueryParams<T>, Json<T>, and Option<Json<T>> handler arguments",
                ));
            }
        }
    }

    Ok(RequestContractShape {
        path,
        query,
        body,
        problems,
    })
}

fn extend_problem_contract(
    error: &Type,
    metadata: &OpenApiContractMetadata,
    problems: &mut Vec<ContractProblem>,
) -> Result<()> {
    match final_type_ident(error).map(Ident::to_string).as_deref() {
        Some("Problem") => {
            if metadata.errors.is_empty() {
                return Err(Error::new_spanned(
                    error,
                    "a strict OpenAPI contract for Problem errors must enumerate errors = [(status, \"code\")]",
                ));
            }
            for error in &metadata.errors {
                let status = parse_problem_status(&error.status)?;
                if error.code.value().trim().is_empty() {
                    return Err(Error::new_spanned(
                        &error.code,
                        "a strict OpenAPI problem code must not be empty",
                    ));
                }
                if problems.iter().any(|problem: &ContractProblem| {
                    problem.status == status && problem.code.value() == error.code.value()
                }) {
                    return Err(Error::new_spanned(
                        &error.code,
                        "a strict OpenAPI problem status and code may be declared only once",
                    ));
                }
                problems.push(ContractProblem {
                    status,
                    code: error.code.clone(),
                });
            }
        }
        Some("EndpointHandleInvocationError") => {
            if !metadata.errors.is_empty() {
                return Err(Error::new_spanned(
                    error,
                    "strict OpenAPI errors require a Problem handler error type",
                ));
            }
        }
        _ => {
            return Err(Error::new_spanned(
                error,
                "strict OpenAPI contracts support Problem or EndpointHandleInvocationError handler errors",
            ));
        }
    }
    Ok(())
}

fn add_json_extractor_problems(problems: &mut Vec<ContractProblem>, span: proc_macro2::Span) {
    problems.push(ContractProblem::generated(400, "invalid_json_body", span));
    problems.push(ContractProblem::generated(
        415,
        "json_content_type_required",
        span,
    ));
}

fn endpoint_result_types(method: &syn::ImplItemFn) -> Result<(&Type, &Type)> {
    let ReturnType::Type(_, output) = &method.sig.output else {
        return Err(Error::new_spanned(
            &method.sig,
            "a strict OpenAPI handler must return Result<Success, Error>",
        ));
    };
    let Type::Path(path) = output.as_ref() else {
        return Err(Error::new_spanned(
            output,
            "a strict OpenAPI handler must return Result<Success, Error>",
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(Error::new_spanned(
            output,
            "a strict OpenAPI handler must return Result<Success, Error>",
        ));
    };
    if segment.ident != "Result" {
        return Err(Error::new_spanned(
            output,
            "a strict OpenAPI handler must return Result<Success, Error>",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(Error::new_spanned(
            output,
            "a strict OpenAPI handler must return Result<Success, Error>",
        ));
    };
    let types = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [success, error] = types.as_slice() else {
        return Err(Error::new_spanned(
            output,
            "a strict OpenAPI handler must return Result<Success, Error>",
        ));
    };
    Ok((success, error))
}

fn strict_success_type(success: &Type, declared_status: Option<&LitInt>) -> Result<(Type, u16)> {
    if final_type_ident(success).is_some_and(|ident| ident == "Json") {
        let status = declared_status
            .map(parse_success_status)
            .transpose()?
            .unwrap_or(200);
        if status != 200 {
            return Err(Error::new_spanned(
                success,
                "Json<T> always returns status 200; use (StatusCode, Json<T>) for another success status",
            ));
        }
        return Ok((extractor_type_argument(success, "Json")?, status));
    }

    let Type::Tuple(tuple) = success else {
        return Err(Error::new_spanned(
            success,
            "strict OpenAPI contracts support Json<T> or (StatusCode, Json<T>) success values",
        ));
    };
    let values = tuple.elems.iter().collect::<Vec<_>>();
    let [status_type, response] = values.as_slice() else {
        return Err(Error::new_spanned(
            success,
            "strict OpenAPI contracts support (StatusCode, Json<T>) success values",
        ));
    };
    if final_type_ident(status_type).is_none_or(|ident| ident != "StatusCode")
        || final_type_ident(response).is_none_or(|ident| ident != "Json")
    {
        return Err(Error::new_spanned(
            success,
            "strict OpenAPI contracts support (StatusCode, Json<T>) success values",
        ));
    }
    let Some(status) = declared_status else {
        return Err(Error::new_spanned(
            success,
            "(StatusCode, Json<T>) needs openapi_contract(success = status)",
        ));
    };
    Ok((
        extractor_type_argument(response, "Json")?,
        parse_success_status(status)?,
    ))
}

fn extractor_type_argument(ty: &Type, expected: &str) -> Result<Type> {
    let Type::Path(path) = ty else {
        return Err(Error::new_spanned(
            ty,
            format!("strict OpenAPI {expected}<T> needs one type parameter"),
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(Error::new_spanned(
            ty,
            format!("strict OpenAPI {expected}<T> needs one type parameter"),
        ));
    };
    if segment.ident != expected {
        return Err(Error::new_spanned(
            ty,
            format!("expected strict OpenAPI {expected}<T>"),
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(Error::new_spanned(
            ty,
            format!("strict OpenAPI {expected}<T> needs one type parameter"),
        ));
    };
    let types = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [inner] = types.as_slice() else {
        return Err(Error::new_spanned(
            ty,
            format!("strict OpenAPI {expected}<T> needs one type parameter"),
        ));
    };
    Ok((*inner).clone())
}

fn parse_success_status(status: &LitInt) -> Result<u16> {
    let parsed = status.base10_parse::<u16>()?;
    if !(200..=299).contains(&parsed) {
        return Err(Error::new_spanned(
            status,
            "strict OpenAPI success status must be between 200 and 299",
        ));
    }
    Ok(parsed)
}

fn parse_problem_status(status: &LitInt) -> Result<u16> {
    let parsed = status.base10_parse::<u16>()?;
    if !(400..=599).contains(&parsed) {
        return Err(Error::new_spanned(
            status,
            "strict OpenAPI problem status must be between 400 and 599",
        ));
    }
    Ok(parsed)
}

fn handler_arguments(method: &syn::ImplItemFn) -> Result<Vec<HandlerArgument>> {
    let mut arguments = Vec::new();
    let mut context_count = 0;
    let mut request_count = 0;
    for (index, input) in method.sig.inputs.iter().enumerate() {
        let FnArg::Typed(argument) = input else {
            continue;
        };
        let ty = (*argument.ty).clone();
        let kind = match final_type_ident(&ty).map(Ident::to_string).as_deref() {
            Some("InvocationContext") => {
                context_count += 1;
                ArgumentKind::Context
            }
            Some("HandleRequest") => {
                request_count += 1;
                ArgumentKind::Request
            }
            _ => ArgumentKind::Extractor(Ident::new(
                &format!("__lenso_extracted_{index}"),
                argument.span(),
            )),
        };
        arguments.push(HandlerArgument { ty, kind });
    }
    if context_count > 1 || request_count > 1 {
        return Err(Error::new_spanned(
            &method.sig.inputs,
            "an endpoint handler accepts at most one InvocationContext and one HandleRequest",
        ));
    }
    Ok(arguments)
}

fn final_type_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    path.path.segments.last().map(|segment| &segment.ident)
}

fn dispatch_arm(handler: &Handler) -> proc_macro2::TokenStream {
    let route_id = &handler.route.id;
    let method = &handler.method;
    let middleware_steps = handler.middlewares.iter().map(|middleware| {
        quote! {
            let (context, request) = match provider.#middleware(context, request).await {
                Ok(outcome) => match outcome.into_result() {
                    Ok(next) => next,
                    Err(response) => return Ok(Ok(response)),
                },
                Err(::lenso_capability_http_endpoint::EndpointHandleInvocationError::Domain(
                    error,
                )) => return Ok(Err(error)),
                Err(::lenso_capability_http_endpoint::EndpointHandleInvocationError::Runtime(
                    error,
                )) => return Err(error),
            };
        }
    });
    let extractor_steps = handler.arguments.iter().filter_map(|argument| {
        let ArgumentKind::Extractor(binding) = &argument.kind else {
            return None;
        };
        let ty = &argument.ty;
        Some(quote! {
            let #binding: #ty = match <#ty as
                ::lenso_capability_http_endpoint::__private::FromRequest<Self>
            >::from_request(&provider, &mut context, &request).await {
                Ok(value) => value,
                Err(::lenso_capability_http_endpoint::__private::ExtractorRejection::Response(
                    response,
                )) => return Ok(Ok(response)),
                Err(::lenso_capability_http_endpoint::__private::ExtractorRejection::Invocation(
                    ::lenso_capability_http_endpoint::EndpointHandleInvocationError::Domain(
                        error,
                    ),
                )) => return Ok(Err(error)),
                Err(::lenso_capability_http_endpoint::__private::ExtractorRejection::Invocation(
                    ::lenso_capability_http_endpoint::EndpointHandleInvocationError::Runtime(
                        error,
                    ),
                )) => return Err(error),
            };
        })
    });
    let mutable_context = handler
        .arguments
        .iter()
        .any(|argument| matches!(&argument.kind, ArgumentKind::Extractor(_)))
        .then(|| quote!(let mut context = context;));
    let arguments = handler
        .arguments
        .iter()
        .map(|argument| match &argument.kind {
            ArgumentKind::Context => quote!(context),
            ArgumentKind::Request => quote!(request),
            ArgumentKind::Extractor(binding) => quote!(#binding),
        });

    quote! {
        #route_id => {
            #(#middleware_steps)*
            #mutable_context
            #(#extractor_steps)*
            let _ = (&context, &request);
            ::lenso_capability_http_endpoint::__private::IntoEndpointResult::into_endpoint_result(
                provider.#method(#(#arguments),*).await,
            )
        }
    }
}

fn http_method(attribute: &Attribute) -> Option<&'static str> {
    let path = attribute.path();
    [
        ("get", "GET"),
        ("post", "POST"),
        ("put", "PUT"),
        ("patch", "PATCH"),
        ("delete", "DELETE"),
        ("head", "HEAD"),
        ("options", "OPTIONS"),
        ("query", "QUERY"),
    ]
    .into_iter()
    .find_map(|(attribute, method)| path.is_ident(attribute).then_some(method))
}

struct RouteArguments {
    route_id: LitStr,
    path: LitStr,
}

impl Parse for RouteArguments {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let arguments = Punctuated::<LitStr, Token![,]>::parse_terminated(input)?;
        if arguments.len() != 2 {
            return Err(Error::new(
                input.span(),
                "HTTP handler attributes require a route ID and path",
            ));
        }
        let mut arguments = arguments.into_iter();
        Ok(Self {
            route_id: arguments.next().expect("length was checked"),
            path: arguments.next().expect("length was checked"),
        })
    }
}

struct Route {
    method: LitStr,
    id: LitStr,
    path: LitStr,
    openapi: Option<LitStr>,
}

struct HandlerMetadata {
    route: Option<Route>,
    middlewares: Vec<Ident>,
    openapi: Option<LitStr>,
    contract: Option<OpenApiContractMetadata>,
}

struct Handler {
    route: Route,
    middlewares: Vec<Ident>,
    method: Ident,
    arguments: Vec<HandlerArgument>,
    contract: Option<EndpointContract>,
}

struct EndpointContract {
    path: Option<Type>,
    query: Option<Type>,
    body: Option<ContractBody>,
    success: Type,
    success_status: u16,
    problems: Vec<ContractProblem>,
}

struct ContractBody {
    ty: Type,
    required: bool,
}

struct RequestContractShape {
    path: Option<Type>,
    query: Option<Type>,
    body: Option<ContractBody>,
    problems: Vec<ContractProblem>,
}

struct ContractProblem {
    status: u16,
    code: LitStr,
}

impl ContractProblem {
    fn generated(status: u16, code: &str, span: proc_macro2::Span) -> Self {
        Self {
            status,
            code: LitStr::new(code, span),
        }
    }
}

struct HandlerArgument {
    ty: Type,
    kind: ArgumentKind,
}

enum ArgumentKind {
    Context,
    Request,
    Extractor(Ident),
}

#[cfg(test)]
mod tests {
    use super::expand_endpoint;
    use syn::parse_quote;

    #[test]
    fn expands_handler_attributes_into_the_static_route_table() {
        let expanded = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi({
                    summary: "Read an order",
                    tags: ["orders"],
                    responses: {
                        "200": { description: "Order" }
                    }
                })]
                async fn read(&self) {}
            }
        })
        .unwrap()
        .to_string();

        assert!(expanded.contains("HttpEndpoint"));
        assert!(expanded.contains("orders.read"));
        assert!(expanded.contains("GET"));
        assert!(expanded.contains("/orders/{order_id}"));
        assert!(expanded.contains("with_openapi"));
        assert!(!expanded.contains("# [get"));
        assert!(!expanded.contains("# [openapi"));
        assert!(expanded.contains("Read an order"));
        assert!(expanded.contains(r#"\"200\""#));
    }

    #[test]
    fn accepts_legacy_json_string_metadata() {
        let expanded = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi(r#"{"summary":"Read an order"}"#)]
                async fn read(&self) {}
            }
        })
        .unwrap()
        .to_string();

        assert!(expanded.contains("Read an order"));
    }

    #[test]
    fn rejects_an_operation_id_in_structured_metadata() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi({ operationId: "another.id" })]
                async fn read(&self) {}
            }
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("generated from the stable route ID")
        );
    }

    #[test]
    fn rejects_duplicate_structured_metadata_keys() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi({ summary: "Read", summary: "Read again" })]
                async fn read(&self) {}
            }
        })
        .unwrap_err();

        assert!(error.to_string().contains("duplicate OpenAPI object key"));
    }

    #[test]
    fn rejects_an_openapi_operation_id_that_can_drift_from_the_route_id() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi(r#"{"operationId":"another.id"}"#)]
                async fn read(&self) {}
            }
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("generated from the stable route ID")
        );
    }

    #[test]
    fn rejects_invalid_openapi_json() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi("not-json")]
                async fn read(&self) {}
            }
        })
        .unwrap_err();

        assert!(error.to_string().contains("not valid JSON"));
    }

    #[test]
    fn emits_a_strict_contract_factory_from_typed_handler_values() {
        let expanded = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[post("orders.create", "/orders/{order_id}")]
                #[openapi({ responses: { "201": { description: "Created" } } })]
                #[openapi_contract(
                    success = 201,
                    errors = [(422, "invalid_order")]
                )]
                async fn create(
                    &self,
                    Path(path): Path<OrderPath>,
                    QueryParams(query): QueryParams<CreateQuery>,
                    Json(order): Json<CreateOrder>,
                ) -> Result<(StatusCode, Json<CreatedOrder>), Problem> {
                    unimplemented!()
                }
            }
        })
        .unwrap()
        .to_string();

        assert!(expanded.contains("with_openapi_contract"));
        assert!(expanded.contains("OpenApiContract"));
        assert!(expanded.contains("path :: < OrderPath >"));
        assert!(expanded.contains("query :: < CreateQuery >"));
        assert!(expanded.contains("json_body :: < CreateOrder >"));
        assert!(expanded.contains("success_json :: < CreatedOrder > (201"));
        assert!(expanded.contains("invalid_order"));
    }

    #[test]
    fn strict_contract_requires_openapi_and_problem_enumeration() {
        let missing_openapi = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi_contract]
                async fn read(&self) -> Result<Json<Order>, Problem> {
                    unimplemented!()
                }
            }
        })
        .unwrap_err();
        assert!(
            missing_openapi
                .to_string()
                .contains("requires an OpenAPI Operation Object")
        );

        let missing_problem = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi({ responses: { "200": { description: "Order" } } })]
                #[openapi_contract]
                async fn read(&self) -> Result<Json<Order>, Problem> {
                    unimplemented!()
                }
            }
        })
        .unwrap_err();
        assert!(
            missing_problem
                .to_string()
                .contains("must enumerate errors")
        );
    }

    #[test]
    fn reserves_the_generated_contract_extension() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[openapi({ "x-lenso-contract": {} })]
                async fn read(&self) {}
            }
        })
        .unwrap_err();

        assert!(error.to_string().contains("reserved for generated"));
    }

    #[test]
    fn expands_middleware_and_typed_extractors_before_the_handler() {
        let expanded = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[middleware(authenticate)]
                #[get("orders.read", "/orders/{order_id}")]
                async fn read(
                    &self,
                    context: InvocationContext,
                    Path(path): Path<OrderPath>,
                ) {}
            }
        })
        .unwrap()
        .to_string();

        assert!(expanded.contains("provider . authenticate"));
        assert!(expanded.contains("into_result"));
        assert!(expanded.contains("FromRequest"));
        assert!(expanded.contains("provider . read (context , __lenso_extracted_2)"));
        assert!(!expanded.contains("# [middleware"));
    }

    #[test]
    fn applies_provider_middleware_before_route_middleware() {
        let expanded = expand_endpoint(parse_quote! {
            #[middleware(trace_all)]
            impl OrdersHttp {
                #[middleware(authorize_read)]
                #[get("orders.read", "/orders/{order_id}")]
                async fn read(&self) {}

                #[get("orders.list", "/orders")]
                async fn list(&self) {}
            }
        })
        .unwrap()
        .to_string();

        assert_eq!(expanded.matches("provider . trace_all").count(), 2);
        assert_eq!(expanded.matches("provider . authorize_read").count(), 1);
        assert!(
            expanded.find("provider . trace_all").unwrap()
                < expanded.find("provider . authorize_read").unwrap()
        );
        assert!(!expanded.contains("# [middleware"));
    }

    #[test]
    fn rejects_handlers_with_multiple_http_methods() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                #[get("orders.read", "/orders/{order_id}")]
                #[post("orders.read", "/orders/{order_id}")]
                async fn read(&self) {}
            }
        })
        .unwrap_err();

        assert!(error.to_string().contains("only one HTTP method"));
    }

    #[test]
    fn rejects_impls_without_handlers() {
        let error = expand_endpoint(parse_quote! {
            impl OrdersHttp {
                fn helper(&self) {}
            }
        })
        .unwrap_err();

        assert!(error.to_string().contains("at least one HTTP handler"));
    }
}
