use gemini_rust::{ClientError, Model, client::Gemini};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Agent {
    client: Gemini,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MsgType {
    Scam,
    Phishing,
    UnsolicitedPromotion,
    OtherSpam,
    NotSpam,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct SpamCheckResult {
    pub msg_type: MsgType,
    pub reason: String,
}

/// Converts a standard JSON schema to Gemini's simplified schema format
/// Gemini doesn't support $schema, $defs, or $ref - this function resolves references and removes unsupported fields
fn convert_to_gemini_schema(mut schema: serde_json::Value) -> serde_json::Value {
    // Extract $defs for reference resolution
    let defs = schema.get("$defs").cloned();

    // Remove unsupported top-level fields
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("$schema");
        obj.remove("$defs");
        obj.remove("title");
    }

    // Recursively resolve $ref references
    resolve_refs(&mut schema, &defs);

    schema
}

/// Recursively resolves $ref references in the schema by inlining definitions
fn resolve_refs(value: &mut serde_json::Value, defs: &Option<serde_json::Value>) {
    match value {
        serde_json::Value::Object(map) => {
            // Check if this object contains a $ref
            if let Some(ref_path) = map.get("$ref").and_then(|v| v.as_str()) {
                // Extract the definition name from "#/$defs/DefinitionName"
                if let Some(def_name) = ref_path.strip_prefix("#/$defs/") {
                    if let Some(inner_defs) = defs {
                        if let Some(definition) = inner_defs.get(def_name) {
                            // Replace the entire object with the resolved definition
                            *value = definition.clone();
                            // Continue resolving in case the definition has more refs
                            // Use the outer 'defs' parameter which is already &Option<T>
                            resolve_refs(value, defs);
                            return;
                        }
                    }
                }
            }

            // Recursively process all values in the object
            for val in map.values_mut() {
                resolve_refs(val, defs);
            }
        }
        serde_json::Value::Array(arr) => {
            // Recursively process all items in the array
            for item in arr.iter_mut() {
                resolve_refs(item, defs);
            }
        }
        _ => {}
    }
}

impl Agent {
    pub fn new(api_key: String) -> Self {
        Self {
            // Failing to create a Gemini client is a fatal error
            client: Gemini::with_model(api_key, Model::Gemini3Flash)
                .expect("Failed to create a Gemini client"),
        }
    }

    pub async fn check_spam(&self, text: &str) -> Result<SpamCheckResult, ClientError> {
        // Convert standard JSON schema to Gemini's format
        let standard_schema = schema_for!(SpamCheckResult);
        let gemini_schema =
            convert_to_gemini_schema(serde_json::to_value(standard_schema).unwrap());

        let response = self
            .client
            .generate_content()
            .with_response_mime_type("application/json")
            .with_response_schema(gemini_schema)
            .with_system_prompt("Please moderate the following content and provide a decision.")
            .with_user_message(text)
            .execute()
            .await?;

        let response_text = response.text();

        // Parse the response, fail open if parsing fails
        match serde_json::from_str::<SpamCheckResult>(&response_text) {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::error!(
                    "Failed to parse Gemini response, failing open (not spam). Error: {}, Response: {}",
                    e,
                    response_text
                );
                // Fail open: assume not spam if we can't parse the response
                Ok(SpamCheckResult {
                    msg_type: MsgType::NotSpam,
                    reason: "Failed to parse AI response".to_string(),
                })
            }
        }
    }
}
