use std::collections::BTreeMap;

use rmcp::model::{ErrorCode, ErrorData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    InvalidRequest,
    Unauthorized,
    NotFound,
    Conflict,
    GmailError,
    StorageError,
    SummaryError,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Failure {
    pub code: FailureCode,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, String>,
}

impl Failure {
    pub fn new(code: FailureCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            context: BTreeMap::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if self.context.len() < 8 {
            self.context
                .insert(key.into(), value.into().chars().take(200).collect());
        }
        self
    }

    pub fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Self::new(FailureCode::InvalidRequest, message, false).with_context("field", field)
    }

    pub fn into_mcp(self) -> ErrorData {
        let code = if self.code == FailureCode::InvalidRequest {
            ErrorCode::INVALID_PARAMS
        } else {
            ErrorCode::INTERNAL_ERROR
        };
        let message = self.message.clone();
        ErrorData::new(code, message, serde_json::to_value(self).ok())
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListMessagesInput {
    pub query: Option<String>,
    #[serde(default)]
    pub label_ids: Vec<String>,
    #[serde(default)]
    pub inbox_only: bool,
    pub max_results: Option<u32>,
    pub page_token: Option<String>,
    #[serde(default)]
    pub include_spam_trash: bool,
    #[serde(default)]
    pub summaries_only: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MessageRef {
    pub id: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MessagePage {
    pub items: Vec<MessageRef>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MessageSummaryPage {
    pub items: Vec<GmailMessageSummary>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ListMessagesOutput {
    References(MessagePage),
    Summaries(MessageSummaryPage),
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct IdInput {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadInput {
    pub id: String,
    #[serde(default)]
    pub summaries_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AttachmentDescriptor {
    pub id: String,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SummaryStatus {
    Generated,
    Cached,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SummaryResult {
    pub status: SummaryStatus,
    pub text: Option<String>,
    pub model: String,
    pub failure_code: Option<FailureCode>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: Option<String>,
    pub label_ids: Vec<String>,
    pub snippet: Option<String>,
    pub internal_date: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub cc: Option<String>,
    pub subject: Option<String>,
    pub date: Option<String>,
    pub content: String,
    pub attachments: Vec<AttachmentDescriptor>,
    pub summary: SummaryResult,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GmailThread {
    pub id: String,
    pub messages: Vec<GmailMessage>,
    pub summary: SummaryResult,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GmailMessageSummary {
    pub id: String,
    pub thread_id: Option<String>,
    pub summary: SummaryResult,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GmailThreadSummary {
    pub id: String,
    pub messages: Vec<GmailMessageSummary>,
    pub summary: SummaryResult,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GmailMessageOutput {
    Full(Box<GmailMessage>),
    Summary(GmailMessageSummary),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GmailThreadOutput {
    Full(GmailThread),
    Summary(GmailThreadSummary),
}

impl GmailMessage {
    pub fn into_summary(self) -> GmailMessageSummary {
        GmailMessageSummary {
            id: self.id,
            thread_id: self.thread_id,
            summary: self.summary,
        }
    }
}

impl GmailThread {
    pub fn into_summary(self) -> GmailThreadSummary {
        GmailThreadSummary {
            id: self.id,
            messages: self
                .messages
                .into_iter()
                .map(GmailMessage::into_summary)
                .collect(),
            summary: self.summary,
        }
    }
}

#[cfg(test)]
mod summary_page_tests {
    use super::*;

    #[test]
    fn summary_page_serialization_contains_only_summary_contract_fields() {
        let output = ListMessagesOutput::Summaries(MessageSummaryPage {
            items: vec![GmailMessageSummary {
                id: "message-id".into(),
                thread_id: Some("thread-id".into()),
                summary: SummaryResult {
                    status: SummaryStatus::Generated,
                    text: Some("safe summary".into()),
                    model: "model".into(),
                    failure_code: None,
                },
            }],
            next_page_token: Some("next".into()),
            result_size_estimate: Some(2),
        });
        let value = serde_json::to_value(output).unwrap();
        let item = &value["items"][0];
        assert_eq!(item["id"], "message-id");
        assert_eq!(item["thread_id"], "thread-id");
        assert!(item.get("summary").is_some());
        for excluded in [
            "content",
            "from",
            "to",
            "cc",
            "subject",
            "date",
            "snippet",
            "label_ids",
            "attachments",
        ] {
            assert!(item.get(excluded).is_none(), "unexpected field {excluded}");
        }
        assert_eq!(value["next_page_token"], "next");
        assert_eq!(value["result_size_estimate"], 2);
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LabelInput {
    pub name: String,
    pub message_list_visibility: Option<String>,
    pub label_list_visibility: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct UpdateLabelInput {
    pub id: String,
    pub name: String,
    pub message_list_visibility: Option<String>,
    pub label_list_visibility: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
    pub message_list_visibility: Option<String>,
    pub label_list_visibility: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LabelsOutput {
    pub items: Vec<GmailLabel>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ModifyLabelsInput {
    pub id: String,
    #[serde(default)]
    pub add_label_ids: Vec<String>,
    #[serde(default)]
    pub remove_label_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MutationResult {
    pub id: String,
    pub thread_id: Option<String>,
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DownloadAttachmentInput {
    pub message_id: String,
    pub attachment_id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DownloadedAttachment {
    pub message_id: String,
    pub attachment_id: String,
    pub filename: String,
    pub path: String,
    pub size: usize,
}

pub fn require_non_empty(value: &str, field: &'static str) -> Result<(), Failure> {
    if value.trim().is_empty() {
        Err(Failure::invalid(
            field,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(())
    }
}

pub fn validate_modify(input: &ModifyLabelsInput) -> Result<(), Failure> {
    require_non_empty(&input.id, "id")?;
    if input.add_label_ids.is_empty() && input.remove_label_ids.is_empty() {
        return Err(Failure::invalid(
            "add_label_ids",
            "at least one label change is required",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> SummaryResult {
        SummaryResult {
            status: SummaryStatus::Generated,
            text: Some("Summary".into()),
            model: "model".into(),
            failure_code: None,
        }
    }

    fn message() -> GmailMessage {
        GmailMessage {
            id: "message-id".into(),
            thread_id: Some("thread-id".into()),
            label_ids: vec!["INBOX".into()],
            snippet: Some("Snippet".into()),
            internal_date: Some(1),
            from: Some("sender@example.com".into()),
            to: Some("recipient@example.com".into()),
            cc: None,
            subject: Some("Subject".into()),
            date: Some("Date".into()),
            content: "Message content".into(),
            attachments: vec![AttachmentDescriptor {
                id: "attachment-id".into(),
                filename: Some("file.txt".into()),
                mime_type: Some("text/plain".into()),
                size: Some(12),
            }],
            summary: summary(),
        }
    }

    #[test]
    fn failure_serializes_as_stable_public_shape() {
        let value = serde_json::to_value(
            Failure::invalid("id", "id must not be empty").with_context("operation", "get_message"),
        )
        .unwrap();
        assert_eq!(value["code"], "invalid_request");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["context"]["field"], "id");
    }

    #[test]
    fn label_modification_requires_id_and_at_least_one_change() {
        let empty_id = ModifyLabelsInput {
            id: " ".into(),
            add_label_ids: vec!["STARRED".into()],
            remove_label_ids: Vec::new(),
        };
        assert_eq!(
            validate_modify(&empty_id).unwrap_err().context["field"],
            "id"
        );

        let no_changes = ModifyLabelsInput {
            id: "message-id".into(),
            add_label_ids: Vec::new(),
            remove_label_ids: Vec::new(),
        };
        assert_eq!(
            validate_modify(&no_changes).unwrap_err().context["field"],
            "add_label_ids"
        );
    }

    #[test]
    fn read_input_defaults_to_full_output() {
        let input: ReadInput =
            serde_json::from_value(serde_json::json!({ "id": "message-id" })).unwrap();

        assert!(!input.summaries_only);
    }

    #[test]
    fn message_summary_output_contains_only_summary_contract_fields() {
        let value =
            serde_json::to_value(GmailMessageOutput::Summary(message().into_summary())).unwrap();
        let object = value.as_object().unwrap();

        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            ["id", "summary", "thread_id"]
        );
        for field in [
            "content",
            "from",
            "to",
            "cc",
            "subject",
            "date",
            "internal_date",
            "snippet",
            "label_ids",
            "attachments",
        ] {
            assert!(!object.contains_key(field), "unexpected field {field}");
        }
    }

    #[test]
    fn thread_summary_output_contains_only_summary_contract_fields() {
        let thread = GmailThread {
            id: "thread-id".into(),
            messages: vec![message()],
            summary: summary(),
        };
        let value =
            serde_json::to_value(GmailThreadOutput::Summary(thread.into_summary())).unwrap();
        let object = value.as_object().unwrap();
        let message = value["messages"][0].as_object().unwrap();

        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            ["id", "messages", "summary"]
        );
        assert_eq!(
            message.keys().map(String::as_str).collect::<Vec<_>>(),
            ["id", "summary", "thread_id"]
        );
        for field in [
            "content",
            "from",
            "to",
            "cc",
            "subject",
            "date",
            "internal_date",
            "snippet",
            "label_ids",
            "attachments",
        ] {
            assert!(!message.contains_key(field), "unexpected field {field}");
        }
    }
}
