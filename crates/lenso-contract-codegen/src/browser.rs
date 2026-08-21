use std::{fmt::Write as _, path::Path};

use super::{
    CodegenError, contract_ir, load_descriptor, pascal_case, quote_string, snake_case,
    typescript_property_name,
};

const BROWSER_RUNTIME: &str = r#"const isRecord = (value) => typeof value === "object" && value !== null;
function validatePortableJson(value) {
  if (typeof value === "number") {
    if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) throw new Error("wire JSON contains an unsafe number");
    return;
  }
  if (Array.isArray(value)) { for (const item of value) validatePortableJson(item); return; }
  if (isRecord(value)) { for (const item of Object.values(value)) validatePortableJson(item); }
}
function decimalInRange(value, signed) {
  if (!(signed ? /^-?(0|[1-9][0-9]*)$/ : /^(0|[1-9][0-9]*)$/).test(value)) return false;
  try {
    const number = BigInt(value);
    return signed
      ? number >= -9223372036854775808n && number <= 9223372036854775807n
      : number >= 0n && number <= 18446744073709551615n;
  } catch { return false; }
}
function isCanonicalBase64(value) {
  if (value === "") return true;
  if (value.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) return false;
  try { return btoa(atob(value)) === value; } catch { return false; }
}
function isRfc3339(value) {
  const match = /^(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:[Zz]|([+-])(\d{2}):(\d{2}))$/.exec(value);
  if (!match) return false;
  const [, year, month, day, hour, minute, second, , offsetHour = "0", offsetMinute = "0"] = match;
  const numericYear = Number(year), numericMonth = Number(month);
  const leap = numericYear % 4 === 0 && (numericYear % 100 !== 0 || numericYear % 400 === 0);
  const days = [0, 31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][numericMonth] ?? 0;
  return numericMonth >= 1 && numericMonth <= 12 && Number(day) >= 1 && Number(day) <= days
    && Number(hour) <= 23 && Number(minute) <= 59 && Number(second) <= 60
    && Number(offsetHour) <= 23 && Number(offsetMinute) <= 59;
}
function isIso8601Duration(value) {
  let index = value.startsWith("-") ? 1 : 0;
  if (value[index] !== "P") return false;
  index += 1;
  let inTime = false, sawComponent = false, sawTime = false;
  while (index < value.length) {
    if (value[index] === "T") { if (inTime || index + 1 === value.length) return false; inTime = true; index += 1; continue; }
    const start = index;
    let separatorSeen = false;
    while (index < value.length && (/[0-9]/.test(value[index]) || (!separatorSeen && value[index] === "."))) {
      separatorSeen ||= value[index] === ".";
      index += 1;
    }
    if (index === start || index >= value.length) return false;
    const unit = value[index];
    if (!(inTime ? ["H", "M", "S"] : ["Y", "M", "W", "D"]).includes(unit)) return false;
    if (inTime) sawTime = true;
    sawComponent = true;
    index += 1;
  }
  return sawComponent && (!inTime || sawTime);
}
function matchesFormat(value, format) {
  if (!format) return true;
  if (format === "int64") return decimalInRange(value, true);
  if (format === "uint64") return decimalInRange(value, false);
  if (format === "byte") return isCanonicalBase64(value);
  if (format === "date-time") return isRfc3339(value);
  if (format === "duration") return isIso8601Duration(value);
  return false;
}
function matchesSchema(value, schema) {
  if (Array.isArray(schema.oneOf) && schema.oneOf.filter((candidate) => matchesSchema(value, candidate)).length !== 1) return false;
  if (Array.isArray(schema.anyOf) && !schema.anyOf.some((candidate) => matchesSchema(value, candidate))) return false;
  if (Object.hasOwn(schema, "const") && value !== schema.const) return false;
  if (Array.isArray(schema.enum) && !schema.enum.includes(value)) return false;
  if (Array.isArray(schema.type)) return schema.type.some((type) => matchesSchema(value, { ...schema, type }));
  if (schema.type === "null") return value === null;
  if (schema.type === "string") {
    if (typeof value !== "string" || !matchesFormat(value, schema.format)) return false;
    const length = Array.from(value).length;
    if (schema.minLength !== undefined && length < schema.minLength) return false;
    if (schema.maxLength !== undefined && length > schema.maxLength) return false;
  }
  if (schema.type === "boolean" && typeof value !== "boolean") return false;
  if (schema.type === "number" && (typeof value !== "number" || !Number.isFinite(value))) return false;
  if (schema.type === "integer" && !Number.isSafeInteger(value)) return false;
  if (["number", "integer"].includes(schema.type)) {
    if (schema.minimum !== undefined && value < schema.minimum) return false;
    if (schema.maximum !== undefined && value > schema.maximum) return false;
    if (schema.exclusiveMinimum !== undefined && value <= schema.exclusiveMinimum) return false;
    if (schema.exclusiveMaximum !== undefined && value >= schema.exclusiveMaximum) return false;
  }
  if (schema.type === "array") {
    if (!Array.isArray(value)) return false;
    if (schema.minItems !== undefined && value.length < schema.minItems) return false;
    if (schema.maxItems !== undefined && value.length > schema.maxItems) return false;
    return !schema.items || value.every((item) => matchesSchema(item, schema.items));
  }
  if (schema.type === "object") {
    if (!isRecord(value) || Array.isArray(value)) return false;
    if ((schema.required ?? []).some((key) => !Object.hasOwn(value, key))) return false;
    const properties = schema.properties ?? {};
    for (const [key, item] of Object.entries(value)) {
      if (Object.hasOwn(properties, key)) { if (!matchesSchema(item, properties[key])) return false; }
      else if (schema.additionalProperties === false) return false;
      else if (isRecord(schema.additionalProperties) && !matchesSchema(item, schema.additionalProperties)) return false;
    }
  }
  return true;
}
function validateSchema(value, schema, label) {
  validatePortableJson(value);
  if (!matchesSchema(value, schema)) throw new Error(`invalid ${label} payload`);
}
function validateDomainError(value, schema) {
  if (matchesSchema(value, schema)) return;
  if (typeof value === "string") return;
  if (isRecord(value) && typeof value.code === "string") return;
  throw new Error("invalid Domain Error payload");
}
function validateResult(result, responseSchema, errorSchema) {
  if (!isRecord(result) || typeof result.ok !== "boolean") throw new Error("invalid Capability result envelope");
  if (result.ok) {
    if (!Object.hasOwn(result, "value")) throw new Error("successful Capability result is missing value");
    validateSchema(result.value, responseSchema, "response");
    return;
  }
  if (!isRecord(result.error) || !["domain", "runtime"].includes(result.error.kind)) throw new Error("invalid invocation error envelope");
  if (result.error.kind === "domain") validateDomainError(result.error.error, errorSchema);
  else if (!isRecord(result.error.error) || typeof result.error.error.kind !== "string") throw new Error("invalid Runtime Failure payload");
}
"#;

/// Generates an executable browser request client from the same validated
/// Descriptor IR used by the checked-in Rust and TypeScript bindings.
pub fn generate_browser_request_client(path: &Path) -> Result<String, CodegenError> {
    let descriptor = load_descriptor(path)?;
    let contract = contract_ir(&descriptor);
    let client_name = contract
        .capability_id
        .split('@')
        .next()
        .and_then(|id| id.rsplit('.').next())
        .map_or_else(|| "Capability".to_owned(), pascal_case);
    let mut methods = String::new();
    for operation in contract
        .operations
        .iter()
        .filter(|operation| operation.interaction == "request")
    {
        let descriptor_operation = descriptor
            .operations
            .iter()
            .find(|candidate| candidate.name == operation.name)
            .expect("Contract IR operations come from the Descriptor");
        let method = typescript_property_name(&snake_case(&operation.name));
        let route = format!(
            "/api/capabilities/{}/{}",
            contract.capability_id, operation.name
        );
        writeln!(
            methods,
            "    async {method}(request, token) {{\n      validateSchema(request, {request_schema}, \"request\");\n      const response = await fetchTransport({}, {{\n        method: \"POST\",\n        headers: {{ \"authorization\": `Bearer ${{token}}`, \"content-type\": \"application/json\" }},\n        body: JSON.stringify(request),\n      }});\n      const result = await response.json();\n      validatePortableJson(result);\n      validateResult(result, {response_schema}, {error_schema});\n      return result;\n    }},",
            quote_string(&route),
            request_schema = descriptor_operation.request_schema,
            response_schema = descriptor_operation.response_schema,
            error_schema = descriptor_operation.domain_error_schema,
        )
        .expect("writing to a String cannot fail");
    }
    let mut output = format!(
        "// @generated by lenso-contract-codegen from {}; do not edit.\n{BROWSER_RUNTIME}",
        contract.capability_id
    );
    write!(
        output,
        "export function create{client_name}Client(fetchTransport = fetch) {{\n  return {{\n{methods}  }};\n}}\n"
    )
    .expect("writing to a String cannot fail");
    Ok(output)
}
