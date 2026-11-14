use crate::Action;

/// Optional public annotations; execution still uses native types, validation and Access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "camelCase"))]
pub struct ActionMetadata {
    /// Public operation documentation, excluding credentials and environment values.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub description: Option<String>,
    /// Optional external input schema. Native inputs need not be JSON-serializable.
    #[cfg(feature = "json")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// Optional external output schema; this annotation does not perform validation.
    #[cfg(feature = "json")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Declared capability names, not grants. None is unknown; an empty list declares none.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub capabilities: Option<Vec<String>>,
    /// Declared effects, never evidence that effects actually occurred.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub effects: Option<Vec<String>>,
}

/// Optional module annotations, independent of runtime resources and operation handles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ModuleMetadata {
    /// Public module documentation.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub description: Option<String>,
    /// Author-declared module version, not a compatibility guarantee.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub version: Option<String>,
    /// Optional declared capability names; these do not authorize calls.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub capabilities: Option<Vec<String>>,
    /// Optional declared effects; empty and unknown remain distinct.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub effects: Option<Vec<String>>,
}

/// A single operation description shared by HTTP, Console and Agent consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionDescription {
    /// The native Action's existing identifier.
    pub id: String,
    /// None means undeclared; discovery never fabricates schemas or permission requirements.
    pub metadata: Option<ActionMetadata>,
}

/// Reads static declarations without validating, authorizing or executing an Action.
pub fn describe_action<R, A: Action<R>>(action: &A) -> ActionDescription {
    ActionDescription {
        id: action.id().to_owned(),
        metadata: action.metadata(),
    }
}

#[cfg(feature = "json")]
pub(crate) fn public_input_path(
    path: impl IntoIterator<Item = crate::InputSegment>,
    mut schema: Option<&serde_json::Value>,
) -> Vec<crate::InputSegment> {
    let mut public = Vec::new();
    for segment in path {
        schema = match &segment {
            crate::InputSegment::Field(field) => schema
                .and_then(|s| s.get("properties"))
                .and_then(|s| s.get(field)),
            crate::InputSegment::Index(_) => schema.and_then(|s| s.get("items")),
        };
        if !schema.is_some_and(|s| s.is_object() || s.is_boolean()) {
            break;
        }
        public.push(segment);
    }
    public
}
