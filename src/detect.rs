use gemini_rust::{ClientError, Model, client::Gemini};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use teloxide::types::Message;

#[derive(Clone)]
pub struct Agent {
    client: Gemini,
}

const SYSTEM_PROMPT: &str = "Content moderator for Telegram groups. Classify messages into categories. Context provided when available helps reduce false positives. Users may swear or trigger keywords normally. Avoid false positives.";

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MsgType {
    Scam,
    Phishing,
    NotSuitableForWork,
    UnsolicitedPromotion,
    OtherSpam,
    NotSpam,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
/// # Result indicating the type of message
pub struct SpamCheckResult {
    /// # Message type
    /// Scam: Messages attempting to defraud users
    /// Phishing: Messages explicitly trying to steal credentials or personal information
    /// NotSuitableForWork: Message that is explicit or sexually provoking
    /// UnsolicitedPromotion: Unwanted advertising or promotional content
    /// OtherSpam: Other annoying messages
    /// NotSpam: Legitimate message
    pub msg_type: MsgType,
}

/// Converts a standard JSON schema to Gemini's simplified schema format
/// Gemini doesn't support $schema, $defs, or $ref - this function resolves references and removes unsupported fields
fn convert_to_gemini_schema(mut schema: serde_json::Value) -> serde_json::Value {
    let defs = schema.get("$defs").cloned();

    if let Some(obj) = schema.as_object_mut() {
        obj.remove("$schema");
        obj.remove("$defs");
    }

    resolve_refs(&mut schema, &defs);

    schema
}

/// Recursively resolves $ref references in the schema by inlining definitions
fn resolve_refs(value: &mut serde_json::Value, defs: &Option<serde_json::Value>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(ref_path) = map.get("$ref").and_then(|v| v.as_str())
                && let Some(def_name) = ref_path.strip_prefix("#/$defs/")
                && let Some(inner_defs) = defs
                && let Some(definition) = inner_defs.get(def_name)
            {
                // Remove the $ref field
                map.remove("$ref");

                // Merge definition fields into the current object
                if let Some(def_obj) = definition.as_object() {
                    for (key, val) in def_obj {
                        map.insert(key.clone(), val.clone());
                    }
                }

                // Continue resolving refs in the merged object
                resolve_refs(value, defs);
                return;
            }

            // If $ref is not directly present, check each individual fields.
            for val in map.values_mut() {
                resolve_refs(val, defs);
            }
        }
        serde_json::Value::Array(arr) => {
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
            client: Gemini::with_model(api_key, Model::Gemini3Flash)
                .expect("Failed to create a Gemini client"),
        }
    }

    /// Helper function to get a consistent sender identifier from a message
    fn get_sender_id(message: &Message) -> String {
        message
            .from
            .as_ref()
            .map(|u| format!("User{}", u.id))
            .unwrap_or_else(|| "Unknown sender".to_string())
    }

    pub async fn check_spam(
        &self,
        message: &Message,
        context: &[Message],
    ) -> Result<SpamCheckResult, ClientError> {
        // Convert standard JSON schema to Gemini's format
        let standard_schema = schema_for!(SpamCheckResult);
        let gemini_schema =
            convert_to_gemini_schema(serde_json::to_value(standard_schema).unwrap());

        // Extract text from current message
        let current_text = message.text().unwrap_or("");

        // Build the prompt with context if available
        let prompt = if context.is_empty() {
            // No context, just send the current message
            current_text.to_string()
        } else {
            // Build context section
            let mut prompt_parts = vec!["History:".to_string()];

            for ctx_msg in context {
                let sender = Self::get_sender_id(ctx_msg);

                if let Some(text) = ctx_msg.text() {
                    prompt_parts.push(format!("- {}: {}", sender, text));
                }
            }

            // Add current message
            let current_sender = Self::get_sender_id(message);

            prompt_parts.push(format!("\nAnalyze:\n{}: {}", current_sender, current_text));

            prompt_parts.join("\n")
        };

        let response = self
            .client
            .generate_content()
            .with_response_mime_type("application/json")
            .with_response_schema(gemini_schema)
            .with_system_prompt(SYSTEM_PROMPT)
            .with_user_message(&prompt)
            .execute()
            .await?;

        let response_text = response.text();

        match serde_json::from_str::<SpamCheckResult>(&response_text) {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::error!(
                    "Failed to parse Gemini response, failing open (not spam). Error: {}, Response: {}",
                    e,
                    response_text
                );
                Ok(SpamCheckResult {
                    msg_type: MsgType::NotSpam,
                })
            }
        }
    }
}
