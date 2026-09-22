//! Source-first Rust authoring macros for Lenso Capability contracts.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Fields, FnArg, GenericArgument, ItemTrait, LitBool, LitInt,
    LitStr, PathArguments, ReturnType, Token, TraitItem, Type, parse_macro_input,
};

struct CapabilityArguments {
    id: LitStr,
    major: LitInt,
    version: LitStr,
    portable: LitBool,
    cross_lane_transfer: LitBool,
}

impl syn::parse::Parse for CapabilityArguments {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut id = None;
        let mut major = None;
        let mut version = None;
        let mut portable = None;
        let mut cross_lane_transfer = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "id" => id = Some(input.parse()?),
                "major" => major = Some(input.parse()?),
                "version" => version = Some(input.parse()?),
                "portable" => portable = Some(input.parse()?),
                "cross_lane_transfer" => cross_lane_transfer = Some(input.parse()?),
                _ => return Err(syn::Error::new(key.span(), "unknown Capability argument")),
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            id: id.ok_or_else(|| input.error("missing `id`"))?,
            major: major.ok_or_else(|| input.error("missing `major`"))?,
            version: version.ok_or_else(|| input.error("missing `version`"))?,
            portable: portable.ok_or_else(|| input.error("missing `portable`"))?,
            cross_lane_transfer: cross_lane_transfer
                .ok_or_else(|| input.error("missing `cross_lane_transfer`"))?,
        })
    }
}

/// Derives one deterministic Capability snapshot from an annotated trait.
#[proc_macro_attribute]
pub fn capability(arguments: TokenStream, item: TokenStream) -> TokenStream {
    let arguments = parse_macro_input!(arguments as CapabilityArguments);
    let mut contract = parse_macro_input!(item as ItemTrait);
    expand_capability(&arguments, &mut contract)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_capability(
    arguments: &CapabilityArguments,
    contract: &mut ItemTrait,
) -> syn::Result<proc_macro2::TokenStream> {
    if arguments.id.value().contains('@') {
        return Err(syn::Error::new_spanned(
            &arguments.id,
            "Capability `id` excludes the major; declare it with `major`",
        ));
    }
    let id = &arguments.id;
    let major = arguments.major.base10_parse::<u64>()?;
    let version = &arguments.version;
    let portable = &arguments.portable;
    let cross_lane_transfer = &arguments.cross_lane_transfer;
    let mut operations = Vec::new();

    for item in &mut contract.items {
        let TraitItem::Fn(method) = item else {
            continue;
        };
        let operation = take_operation_arguments(&mut method.attrs)?;
        let operation_name = operation
            .as_ref()
            .and_then(|operation| operation.name.as_ref())
            .map_or_else(|| method.sig.ident.to_string(), LitStr::value);
        let request = request_type(method)?;
        let (interaction, response, domain_error) = operation_result_types(method)?;
        if let Some(declared_interaction) = operation
            .as_ref()
            .and_then(|operation| operation.interaction.as_ref())
            && declared_interaction.value() != interaction
        {
            return Err(syn::Error::new_spanned(
                declared_interaction,
                format!(
                    "Operation interaction is inferred as `{interaction}` from its return type"
                ),
            ));
        }
        operations.push(quote! {
            ::lenso_contract_authoring::OperationSnapshot {
                name: #operation_name.to_owned(),
                interaction: #interaction.to_owned(),
                request_schema: ::lenso_contract_authoring::schema_for::<#request>(),
                response_schema: ::lenso_contract_authoring::schema_for::<#response>(),
                domain_error_schema: <#domain_error as ::lenso_contract_authoring::DomainErrorSchema>::domain_error_schema(),
            }
        });
    }
    if operations.is_empty() {
        return Err(syn::Error::new_spanned(
            &contract.ident,
            "a Capability trait must declare at least one Operation",
        ));
    }

    Ok(quote! {
        #[allow(async_fn_in_trait)]
        #contract

        #[doc(hidden)]
        pub fn __lenso_capability_snapshot() -> ::lenso_contract_authoring::CapabilitySnapshot {
            ::lenso_contract_authoring::CapabilitySnapshot {
                capability_id: format!("{}@{}", #id, #major),
                version: #version.to_owned(),
                portable: #portable,
                cross_lane_transfer: #cross_lane_transfer,
                operations: vec![#(#operations),*],
            }
        }
    })
}

struct OperationArguments {
    name: Option<LitStr>,
    interaction: Option<LitStr>,
}

impl syn::parse::Parse for OperationArguments {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut name = None;
        let mut interaction = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "name" => name = Some(input.parse()?),
                "interaction" => interaction = Some(input.parse()?),
                _ => return Err(syn::Error::new(key.span(), "unknown Operation argument")),
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self { name, interaction })
    }
}

fn take_operation_arguments(
    attributes: &mut Vec<Attribute>,
) -> syn::Result<Option<OperationArguments>> {
    let Some(index) = attributes.iter().position(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "operation")
    }) else {
        return Ok(None);
    };
    let attribute = attributes.remove(index);
    attribute.parse_args().map(Some)
}

fn request_type(method: &syn::TraitItemFn) -> syn::Result<&Type> {
    if method.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "a Capability Operation must be `async`",
        ));
    }
    let mut arguments = method.sig.inputs.iter();
    let receiver = arguments.next();
    let context = arguments.next();
    let request = arguments.next();
    if arguments.next().is_some()
        || !matches!(receiver, Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() && receiver.mutability.is_none())
        || !matches!(context, Some(FnArg::Typed(context)) if is_context_type(&context.ty))
    {
        return Err(syn::Error::new_spanned(
            &method.sig.inputs,
            "a Capability Operation must accept `&self`, `Ctx<'_>`, and one request value",
        ));
    }
    let Some(FnArg::Typed(request)) = request else {
        return Err(syn::Error::new_spanned(
            &method.sig.inputs,
            "a Capability Operation must accept one request value",
        ));
    };
    Ok(&request.ty)
}

fn is_context_type(ty: &Type) -> bool {
    let Type::Path(ty) = ty else {
        return false;
    };
    ty.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Ctx")
}

fn operation_result_types(method: &syn::TraitItemFn) -> syn::Result<(&'static str, &Type, &Type)> {
    let ReturnType::Type(_, output) = &method.sig.output else {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "a Capability Operation must return `Result<Response, DomainError>` or `Stream<Message, DomainError>`",
        ));
    };
    let Type::Path(result) = output.as_ref() else {
        return Err(syn::Error::new_spanned(
            output,
            "expected `Result<Response, DomainError>` or `Stream<Message, DomainError>`",
        ));
    };
    let Some(segment) = result.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            output,
            "expected `Result<Response, DomainError>` or `Stream<Message, DomainError>`",
        ));
    };
    let interaction = match segment.ident.to_string().as_str() {
        "Result" => "request",
        "Stream" => "stream",
        _ => {
            return Err(syn::Error::new_spanned(
                output,
                "expected `Result<Response, DomainError>` or `Stream<Message, DomainError>`",
            ));
        }
    };
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            output,
            "expected `Result<Response, DomainError>` or `Stream<Message, DomainError>`",
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
    match types.as_slice() {
        [response, domain_error] => Ok((interaction, *response, *domain_error)),
        _ => Err(syn::Error::new_spanned(
            output,
            "expected exactly two response and Domain Error type arguments",
        )),
    }
}

/// Derives the portable open Domain Error union used by a Capability Operation.
#[proc_macro_derive(DomainError, attributes(lenso))]
pub fn domain_error(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    expand_domain_error(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_domain_error(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Enum(errors) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "DomainError requires an enum",
        ));
    };
    let name = &input.ident;
    let mut variants = Vec::new();
    for variant in &errors.variants {
        let code = error_code(&variant.attrs, &variant.ident)?;
        match &variant.fields {
            Fields::Unit => variants.push(quote! {
                ::lenso_contract_authoring::unit_error_schema(#code)
            }),
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let payload = &fields.unnamed[0].ty;
                variants.push(quote! {
                    ::lenso_contract_authoring::structured_error_schema::<#payload>(#code)
                });
            }
            Fields::Named(fields) if fields.named.len() == 1 => {
                let field = fields.named.first().expect("one field");
                if field.ident.as_ref().is_none_or(|ident| ident != "payload") {
                    return Err(syn::Error::new_spanned(
                        fields,
                        "a structured Domain Error field must be named `payload`",
                    ));
                }
                let payload = &field.ty;
                variants.push(quote! {
                    ::lenso_contract_authoring::structured_error_schema::<#payload>(#code)
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    &variant.fields,
                    "a Domain Error variant is unit or carries exactly one payload type",
                ));
            }
        }
    }
    Ok(quote! {
        impl ::lenso_contract_authoring::DomainErrorSchema for #name {
            fn domain_error_schema() -> ::lenso_contract_authoring::__private::serde_json::Value {
                ::lenso_contract_authoring::domain_error_union(vec![#(#variants),*])
            }
        }
    })
}

fn error_code(attributes: &[Attribute], ident: &syn::Ident) -> syn::Result<String> {
    for attribute in attributes {
        if attribute.path().is_ident("lenso") {
            let mut code = None;
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("code") {
                    code = Some(meta.value()?.parse::<LitStr>()?.value());
                    Ok(())
                } else {
                    Err(meta.error("unknown Domain Error argument"))
                }
            })?;
            if let Some(code) = code {
                return Ok(code);
            }
        }
    }
    Ok(to_snake_case(&ident.to_string()))
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use syn::{ItemTrait, parse_quote};

    use super::{CapabilityArguments, expand_capability};

    fn arguments() -> CapabilityArguments {
        syn::parse_str(
            r#"id = "example.test", major = 1, version = "1.0.0", portable = true, cross_lane_transfer = false"#,
        )
        .unwrap()
    }

    #[test]
    fn rejects_non_async_operations() {
        let mut contract: ItemTrait = parse_quote! {
            trait Test {
                fn run(&self, context: lenso::Ctx<'_>, request: Input) -> Result<Output, Error>;
            }
        };
        let error = expand_capability(&arguments(), &mut contract).unwrap_err();
        assert!(error.to_string().contains("must be `async`"));
    }

    #[test]
    fn rejects_an_interaction_that_disagrees_with_the_return_type() {
        let mut contract: ItemTrait = parse_quote! {
            trait Test {
                #[lenso::operation(interaction = "request")]
                async fn run(
                    &self,
                    context: lenso::Ctx<'_>,
                    request: Input,
                ) -> lenso::Stream<Output, Error>;
            }
        };
        let error = expand_capability(&arguments(), &mut contract).unwrap_err();
        assert!(error.to_string().contains("inferred as `stream`"));
    }
}
