use anyhow::{Context as _, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const OPENAPI_PATH: &str = "contracts/openapi/app-api.v1.yaml";
const TYPES_PATH: &str = "packages/ts-sdk/src/generated/types.ts";
const CLIENT_PATH: &str = "packages/ts-sdk/src/generated/client.ts";

pub fn generate() -> anyhow::Result<()> {
    write_file(TYPES_PATH, &generated_types_source()?)?;
    write_file(CLIENT_PATH, &generated_client_source()?)?;
    Ok(())
}

pub fn generated_types_source() -> anyhow::Result<String> {
    let document = read_openapi()?;
    let schemas = document
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .context("OpenAPI document is missing components.schemas")?;

    let mut rendered = String::from(GENERATED_HEADER);
    for (schema_name, schema) in schemas.iter().collect::<BTreeMap<_, _>>() {
        let required = required_fields(schema);
        rendered.push_str(&format!(
            "export type {schema_name} = {};\n\n",
            type_from_schema(schema, &required)?
        ));
    }

    Ok(rendered)
}

pub fn generated_client_source() -> anyhow::Result<String> {
    let document = read_openapi()?;
    assert_json_operation(
        &document,
        "/paths/~1v1~1identity~1users/post",
        "identity_create_user",
        "CreateUserRequest",
        "CreateUserResponseEnvelope",
    )?;
    assert_json_operation(
        &document,
        "/paths/~1v1~1auth~1password~1register/post",
        "auth_password_register",
        "PasswordRegisterRequest",
        "PasswordSessionResponseEnvelope",
    )?;
    assert_json_operation(
        &document,
        "/paths/~1v1~1auth~1password~1login/post",
        "auth_password_login",
        "PasswordLoginRequest",
        "PasswordSessionResponseEnvelope",
    )?;

    Ok(format!(
        "{GENERATED_HEADER}import type {{ CreateUserRequest, CreateUserResponse, ErrorResponse, PasswordLoginRequest, PasswordRegisterRequest, PasswordSessionResponse }} from './types.js';\n\n{}",
        CLIENT_BODY
    ))
}

fn read_openapi() -> anyhow::Result<Value> {
    let path = repo_root().join(OPENAPI_PATH);
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&source).context("OpenAPI contract should parse as YAML")
}

fn write_file(path: impl AsRef<Path>, contents: &str) -> anyhow::Result<()> {
    let path = repo_root().join(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("generator crate should be inside tools/generate-ts-sdk")
        .to_path_buf()
}

fn assert_json_operation(
    document: &Value,
    operation_pointer: &str,
    expected_operation_id: &str,
    request_type: &str,
    response_type: &str,
) -> anyhow::Result<()> {
    let operation = document
        .pointer(operation_pointer)
        .with_context(|| format!("OpenAPI document is missing {operation_pointer}"))?;
    let operation_id = operation
        .get("operationId")
        .and_then(Value::as_str)
        .with_context(|| format!("{operation_pointer} is missing operationId"))?;
    if operation_id != expected_operation_id {
        bail!("unexpected operationId for {operation_pointer}: {operation_id}");
    }

    let request_ref = operation
        .pointer("/requestBody/content/application~1json/schema/$ref")
        .and_then(Value::as_str);
    let expected_request_ref = format!("#/components/schemas/{request_type}");
    if request_ref != Some(expected_request_ref.as_str()) {
        bail!("{operation_pointer} request body must reference {request_type}");
    }

    let success_ref = operation
        .pointer("/responses/200/content/application~1json/schema/$ref")
        .and_then(Value::as_str);
    let expected_success_ref = format!("#/components/schemas/{response_type}");
    if success_ref != Some(expected_success_ref.as_str()) {
        bail!("{operation_pointer} response must reference {response_type}");
    }

    Ok(())
}

fn required_fields(schema: &Value) -> BTreeSet<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn type_from_schema(schema: &Value, required: &BTreeSet<String>) -> anyhow::Result<String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return Ok(type_name_from_ref(reference).to_owned());
    }

    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        let parts = one_of
            .iter()
            .map(|branch| {
                let required = required_fields(branch);
                type_from_schema(branch, &required)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        return Ok(union_type(parts));
    }

    match schema.get("type") {
        Some(Value::String(schema_type)) => match schema_type.as_str() {
            "object" => object_type(schema, required),
            "array" => {
                let item_type = schema
                    .get("items")
                    .map(|items| type_from_schema(items, &BTreeSet::new()))
                    .transpose()?
                    .unwrap_or_else(|| "unknown".to_owned());
                Ok(format!("Array<{item_type}>"))
            }
            "string" => Ok("string".to_owned()),
            "integer" | "number" => Ok("number".to_owned()),
            "boolean" => Ok("boolean".to_owned()),
            "null" => Ok("null".to_owned()),
            _ => Ok("unknown".to_owned()),
        },
        Some(Value::Array(types)) => {
            let mut parts = Vec::new();
            for schema_type in types {
                let Some(schema_type) = schema_type.as_str() else {
                    continue;
                };
                parts.push(type_from_schema(
                    &serde_json::json!({ "type": schema_type }),
                    &BTreeSet::new(),
                )?);
            }
            Ok(union_type(parts))
        }
        _ => Ok("unknown".to_owned()),
    }
}

fn union_type(parts: Vec<String>) -> String {
    let mut seen = BTreeSet::new();
    let mut rendered = Vec::new();
    let mut includes_null = false;

    for part in parts {
        if part == "null" {
            includes_null = true;
            continue;
        }
        if seen.insert(part.clone()) {
            rendered.push(part);
        }
    }

    if includes_null {
        rendered.push("null".to_owned());
    }

    if rendered.is_empty() {
        "unknown".to_owned()
    } else {
        rendered.join(" | ")
    }
}

fn object_type(schema: &Value, required: &BTreeSet<String>) -> anyhow::Result<String> {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Ok("Record<string, unknown>".to_owned());
    };

    let sorted = properties.iter().collect::<BTreeMap<_, _>>();
    let mut fields = String::new();
    fields.push_str("{\n");
    for (name, property) in sorted {
        let optional = if required.contains(name.as_str()) {
            ""
        } else {
            "?"
        };
        fields.push_str(&format!(
            "  {name}{optional}: {};\n",
            type_from_schema(property, &BTreeSet::new())?
        ));
    }
    fields.push('}');
    Ok(fields)
}

fn type_name_from_ref(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}

const GENERATED_HEADER: &str = "/* eslint-disable */\n// Generated from contracts/openapi/app-api.v1.yaml. Do not edit by hand.\n\n";

const CLIENT_BODY: &str = r#"export type LensoClientOptions = {
  baseUrl: string;
  fetch?: typeof fetch;
  headers?: HeadersInit | (() => HeadersInit | Promise<HeadersInit>);
};

export class LensoApiError extends Error {
  readonly status: number;
  readonly response: ErrorResponse;

  constructor(status: number, response: ErrorResponse) {
    super(response.error.message);
    this.name = 'LensoApiError';
    this.status = status;
    this.response = response;
  }
}

export type CreateUserResponseEnvelope = {
  data: CreateUserResponse;
};

export type PasswordSessionResponseEnvelope = {
  data: PasswordSessionResponse;
};

export class GeneratedLensoClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly headers?: LensoClientOptions['headers'];

  constructor(options: LensoClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.fetchImpl = options.fetch ?? fetch;
    this.headers = options.headers;
  }

  async createUser(input: CreateUserRequest): Promise<CreateUserResponse> {
    const body = await this.postJson<CreateUserResponseEnvelope>(
      '/v1/identity/users',
      input
    );
    return body.data;
  }

  async authPasswordRegister(
    input: PasswordRegisterRequest
  ): Promise<PasswordSessionResponse> {
    const body = await this.postJson<PasswordSessionResponseEnvelope>(
      '/v1/auth/password/register',
      input
    );
    return body.data;
  }

  async authPasswordLogin(
    input: PasswordLoginRequest
  ): Promise<PasswordSessionResponse> {
    const body = await this.postJson<PasswordSessionResponseEnvelope>(
      '/v1/auth/password/login',
      input
    );
    return body.data;
  }

  private async postJson<T>(path: string, input: unknown): Promise<T> {
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...(await this.resolveHeaders()),
      },
      body: JSON.stringify(input),
    });

    const body = await response.json();
    if (!response.ok) {
      throw new LensoApiError(response.status, body as ErrorResponse);
    }

    return body as T;
  }

  private async resolveHeaders(): Promise<HeadersInit> {
    if (!this.headers) {
      return {};
    }

    return typeof this.headers === 'function' ? await this.headers() : this.headers;
  }
}
"#;
