//! Rust `Debug` rendering for schema fields marked as sensitive.

pub(super) fn render<'a>(
    name: &str,
    fields: impl IntoIterator<Item = (&'a str, String, bool)>,
) -> Option<String> {
    let fields = fields.into_iter().collect::<Vec<_>>();
    if !fields.iter().any(|(_, _, sensitive)| *sensitive) {
        return None;
    }
    let rendered = fields
        .into_iter()
        .map(|(wire_name, rust_name, sensitive)| {
            let wire_name = serde_json::to_string(wire_name).expect("field name is serializable");
            if sensitive {
                format!("            .field({wire_name}, &\"<redacted>\")")
            } else {
                format!("            .field({wire_name}, &self.{rust_name})")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let name_literal = serde_json::to_string(name).expect("type name is serializable");
    Some(format!(
        "\nimpl fmt::Debug for {name} {{\n    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {{\n        formatter\n            .debug_struct({name_literal})\n{rendered}\n            .finish()\n    }}\n}}\n"
    ))
}
