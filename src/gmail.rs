use std::{
    env,
    path::{Path, PathBuf},
};

use google_gmail1::{
    Gmail,
    api::{
        Label, Message, MessagePart, MessagePartHeader, ModifyMessageRequest, ModifyThreadRequest,
    },
    hyper_rustls,
    hyper_util::{self, client::legacy::connect::HttpConnector},
    yup_oauth2,
};
use uuid::Uuid;

use crate::{
    contracts::{
        AttachmentDescriptor, DownloadAttachmentInput, DownloadedAttachment, Failure, FailureCode,
        GmailLabel, GmailMessage, GmailThread, IdInput, LabelInput, LabelsOutput,
        ListMessagesInput, MessagePage, MessageRef, ModifyLabelsInput, MutationResult,
        SummaryResult, UpdateLabelInput, require_non_empty, validate_modify,
    },
    summary::Summarizer,
};

type Connector = hyper_rustls::HttpsConnector<HttpConnector>;
const READ_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const LABEL_SCOPE: &str = "https://www.googleapis.com/auth/gmail.labels";
const MODIFY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";

#[derive(Clone)]
pub struct GmailClient {
    hub: Gmail<Connector>,
    summarizer: Summarizer,
    attachment_root: PathBuf,
}

impl GmailClient {
    pub async fn from_env(summarizer: Summarizer) -> anyhow::Result<Self> {
        let credentials = env_path("GMAIL_CLIENT_SECRET", ".secrets/client_secret.json");
        let token_cache = env_path("GMAIL_TOKEN_CACHE", "/data/token_cache.json");
        let attachment_root = env_path("ATTACHMENT_ROOT", "/attachments");
        tokio::fs::create_dir_all(&attachment_root).await?;
        let secret = yup_oauth2::read_application_secret(&credentials).await?;

        let auth_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_only()
            .enable_http2()
            .build();
        let auth_client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(auth_connector);
        let auth = yup_oauth2::InstalledFlowAuthenticator::with_client(
            secret,
            yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
            yup_oauth2::client::CustomHyperClientBuilder::from(auth_client),
        )
        .persist_tokens_to_disk(token_cache)
        .build()
        .await?;

        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http2()
            .build();
        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector);

        Ok(Self {
            hub: Gmail::new(client, auth),
            summarizer,
            attachment_root,
        })
    }

    #[cfg(test)]
    pub async fn for_mcp_smoke(
        summarizer: Summarizer,
        attachment_root: PathBuf,
    ) -> anyhow::Result<Self> {
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http2()
            .build();
        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector);
        let mut hub = Gmail::new(client, google_gmail1::common::NoToken);
        hub.base_url("http://127.0.0.1:9/".to_owned());
        Ok(Self {
            hub,
            summarizer,
            attachment_root,
        })
    }

    pub async fn list_messages(&self, input: ListMessagesInput) -> Result<MessagePage, Failure> {
        if let Some(max) = input.max_results
            && !(1..=100).contains(&max)
        {
            return Err(Failure::invalid(
                "max_results",
                "max_results must be between 1 and 100",
            ));
        }
        let mut call = self.hub.users().messages_list("me");
        if let Some(query) = input.query.as_deref() {
            call = call.q(query);
        }
        if let Some(page_token) = input.page_token.as_deref() {
            call = call.page_token(page_token);
        }
        call = call
            .include_spam_trash(input.include_spam_trash)
            .max_results(input.max_results.unwrap_or(25));
        for label in &input.label_ids {
            call = call.add_label_ids(label);
        }
        if input.inbox_only && !input.label_ids.iter().any(|label| label == "INBOX") {
            call = call.add_label_ids("INBOX");
        }
        let (_, response) = call
            .add_scope(READ_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        Ok(MessagePage {
            items: response
                .messages
                .unwrap_or_default()
                .into_iter()
                .map(message_ref)
                .collect(),
            next_page_token: response.next_page_token,
            result_size_estimate: response.result_size_estimate,
        })
    }

    pub async fn get_message(&self, input: IdInput) -> Result<GmailMessage, Failure> {
        require_non_empty(&input.id, "id")?;
        let (_, message) = self
            .hub
            .users()
            .messages_get("me", &input.id)
            .format("full")
            .add_scope(READ_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        self.map_message(message).await
    }

    pub async fn get_thread(&self, input: IdInput) -> Result<GmailThread, Failure> {
        require_non_empty(&input.id, "id")?;
        let (_, thread) = self
            .hub
            .users()
            .threads_get("me", &input.id)
            .format("full")
            .add_scope(READ_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        let mut messages = Vec::new();
        for message in thread.messages.unwrap_or_default() {
            messages.push(self.map_message(message).await?);
        }
        let summary_inputs = messages
            .iter()
            .map(|message| (message.content.clone(), message.summary.clone()))
            .collect::<Vec<_>>();
        let summary = self.summarizer.summarize_thread(&summary_inputs).await;
        Ok(GmailThread {
            id: thread.id.unwrap_or(input.id),
            messages,
            summary,
        })
    }

    pub async fn list_labels(&self) -> Result<LabelsOutput, Failure> {
        let (_, response) = self
            .hub
            .users()
            .labels_list("me")
            .add_scope(LABEL_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        Ok(LabelsOutput {
            items: response
                .labels
                .unwrap_or_default()
                .into_iter()
                .map(label_out)
                .collect(),
        })
    }

    pub async fn create_label(&self, input: LabelInput) -> Result<GmailLabel, Failure> {
        validate_label(&input.name)?;
        let (_, label) = self
            .hub
            .users()
            .labels_create(label_request(input), "me")
            .add_scope(LABEL_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        Ok(label_out(label))
    }

    pub async fn update_label(&self, input: UpdateLabelInput) -> Result<GmailLabel, Failure> {
        require_non_empty(&input.id, "id")?;
        validate_label(&input.name)?;
        let request = Label {
            name: Some(input.name),
            message_list_visibility: input.message_list_visibility,
            label_list_visibility: input.label_list_visibility,
            ..Default::default()
        };
        let (_, label) = self
            .hub
            .users()
            .labels_update(request, "me", &input.id)
            .add_scope(LABEL_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        Ok(label_out(label))
    }

    pub async fn delete_label(&self, input: IdInput) -> Result<MutationResult, Failure> {
        require_non_empty(&input.id, "id")?;
        self.hub
            .users()
            .labels_delete("me", &input.id)
            .add_scope(LABEL_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        Ok(MutationResult {
            id: input.id,
            thread_id: None,
            label_ids: Vec::new(),
        })
    }

    pub async fn modify_message_labels(
        &self,
        input: ModifyLabelsInput,
    ) -> Result<MutationResult, Failure> {
        validate_modify(&input)?;
        let request = ModifyMessageRequest {
            add_label_ids: Some(input.add_label_ids),
            remove_label_ids: Some(input.remove_label_ids),
        };
        let (_, message) = self
            .hub
            .users()
            .messages_modify(request, "me", &input.id)
            .add_scope(MODIFY_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        Ok(mutation_out(message))
    }

    pub async fn modify_thread_labels(
        &self,
        input: ModifyLabelsInput,
    ) -> Result<MutationResult, Failure> {
        validate_modify(&input)?;
        let request = ModifyThreadRequest {
            add_label_ids: Some(input.add_label_ids),
            remove_label_ids: Some(input.remove_label_ids),
        };
        let (_, thread) = self
            .hub
            .users()
            .threads_modify(request, "me", &input.id)
            .add_scope(MODIFY_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        let labels = thread
            .messages
            .unwrap_or_default()
            .into_iter()
            .flat_map(|message| message.label_ids.unwrap_or_default())
            .collect();
        Ok(MutationResult {
            id: thread.id.unwrap_or(input.id),
            thread_id: None,
            label_ids: labels,
        })
    }

    pub async fn archive_message(&self, input: IdInput) -> Result<MutationResult, Failure> {
        self.modify_message_labels(ModifyLabelsInput {
            id: input.id,
            add_label_ids: Vec::new(),
            remove_label_ids: vec!["INBOX".into()],
        })
        .await
    }

    pub async fn mark_message_spam(&self, input: IdInput) -> Result<MutationResult, Failure> {
        self.modify_message_labels(ModifyLabelsInput {
            id: input.id,
            add_label_ids: vec!["SPAM".into()],
            remove_label_ids: Vec::new(),
        })
        .await
    }

    pub async fn download_attachment(
        &self,
        input: DownloadAttachmentInput,
    ) -> Result<DownloadedAttachment, Failure> {
        require_non_empty(&input.message_id, "message_id")?;
        require_non_empty(&input.attachment_id, "attachment_id")?;
        let (_, attachment) = self
            .hub
            .users()
            .messages_attachments_get("me", &input.message_id, &input.attachment_id)
            .add_scope(READ_SCOPE)
            .doit()
            .await
            .map_err(gmail_error)?;
        let bytes = attachment.data.ok_or_else(|| {
            Failure::new(
                FailureCode::NotFound,
                "attachment data was not found",
                false,
            )
        })?;
        let filename = format!("{}.bin", Uuid::new_v4());
        let path = safe_attachment_path(&self.attachment_root, &filename)?;
        tokio::fs::write(&path, &bytes).await.map_err(|_| {
            Failure::new(
                FailureCode::StorageError,
                "attachment could not be written",
                true,
            )
        })?;
        Ok(DownloadedAttachment {
            message_id: input.message_id,
            attachment_id: input.attachment_id,
            filename,
            path: path.to_string_lossy().into_owned(),
            size: bytes.len(),
        })
    }

    async fn map_message(&self, message: Message) -> Result<GmailMessage, Failure> {
        let payload = message.payload.as_ref();
        let headers = payload
            .and_then(|payload| payload.headers.clone())
            .unwrap_or_default();
        let (plain, html, attachments) =
            payload.map_or_else(|| (String::new(), String::new(), Vec::new()), extract_parts);
        let content = if !plain.trim().is_empty() {
            plain
        } else if !html.trim().is_empty() {
            html2text::from_read(html.as_bytes(), 120).unwrap_or_default()
        } else {
            String::new()
        };
        let summary: SummaryResult = self.summarizer.summarize(&content).await;
        Ok(message_out(
            message,
            &headers,
            content,
            attachments,
            summary,
        ))
    }
}

fn message_out(
    message: Message,
    headers: &[MessagePartHeader],
    content: String,
    attachments: Vec<AttachmentDescriptor>,
    summary: SummaryResult,
) -> GmailMessage {
    GmailMessage {
        id: message.id.unwrap_or_default(),
        thread_id: message.thread_id,
        label_ids: message.label_ids.unwrap_or_default(),
        snippet: message.snippet,
        internal_date: message.internal_date,
        from: header(headers, "From"),
        to: header(headers, "To"),
        cc: header(headers, "Cc"),
        subject: header(headers, "Subject"),
        date: header(headers, "Date"),
        content,
        attachments,
        summary,
    }
}

fn extract_parts(part: &MessagePart) -> (String, String, Vec<AttachmentDescriptor>) {
    fn visit(
        part: &MessagePart,
        plain: &mut Vec<String>,
        html: &mut Vec<String>,
        attachments: &mut Vec<AttachmentDescriptor>,
    ) {
        let body = part.body.as_ref();
        if let Some(id) = body.and_then(|body| body.attachment_id.clone()) {
            attachments.push(AttachmentDescriptor {
                id,
                filename: part.filename.clone().filter(|value| !value.is_empty()),
                mime_type: part.mime_type.clone(),
                size: body.and_then(|body| body.size),
            });
        }
        if let Some(data) = body.and_then(|body| body.data.as_deref())
            && let Ok(text) = std::str::from_utf8(data)
        {
            match part.mime_type.as_deref() {
                Some("text/plain") => plain.push(text.to_owned()),
                Some("text/html") => html.push(text.to_owned()),
                _ => {}
            }
        }
        for child in part.parts.as_deref().unwrap_or_default() {
            visit(child, plain, html, attachments);
        }
    }
    let (mut plain, mut html, mut attachments) = (Vec::new(), Vec::new(), Vec::new());
    visit(part, &mut plain, &mut html, &mut attachments);
    (plain.join("\n"), html.join("\n"), attachments)
}

fn safe_attachment_path(root: &Path, filename: &str) -> Result<PathBuf, Failure> {
    if filename.contains(['/', '\\']) || filename == "." || filename == ".." {
        return Err(Failure::invalid(
            "filename",
            "invalid generated attachment filename",
        ));
    }
    Ok(root.join(filename))
}

fn label_request(input: LabelInput) -> Label {
    Label {
        name: Some(input.name),
        message_list_visibility: input.message_list_visibility,
        label_list_visibility: input.label_list_visibility,
        ..Default::default()
    }
}
fn validate_label(name: &str) -> Result<(), Failure> {
    require_non_empty(name, "name")
}
fn message_ref(message: Message) -> MessageRef {
    MessageRef {
        id: message.id.unwrap_or_default(),
        thread_id: message.thread_id,
    }
}
fn mutation_out(message: Message) -> MutationResult {
    MutationResult {
        id: message.id.unwrap_or_default(),
        thread_id: message.thread_id,
        label_ids: message.label_ids.unwrap_or_default(),
    }
}
fn label_out(label: Label) -> GmailLabel {
    GmailLabel {
        id: label.id.unwrap_or_default(),
        name: label.name.unwrap_or_default(),
        kind: label.type_,
        message_list_visibility: label.message_list_visibility,
        label_list_visibility: label.label_list_visibility,
    }
}
fn header(headers: &[MessagePartHeader], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|header| {
            header
                .name
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        })
        .and_then(|header| header.value.clone())
}
fn gmail_error(_error: google_gmail1::Error) -> Failure {
    Failure::new(FailureCode::GmailError, "Gmail operation failed", true)
}
fn env_path(name: &str, default: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| default.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_gmail1::api::MessagePartBody;

    #[test]
    fn nested_plain_text_and_attachments_are_extracted() {
        let part = MessagePart {
            parts: Some(vec![
                MessagePart {
                    mime_type: Some("text/html".into()),
                    body: Some(MessagePartBody {
                        data: Some(b"<p>html</p>".to_vec()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                MessagePart {
                    mime_type: Some("text/plain".into()),
                    body: Some(MessagePartBody {
                        data: Some(b"plain".to_vec()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                MessagePart {
                    filename: Some("report.pdf".into()),
                    mime_type: Some("application/pdf".into()),
                    body: Some(MessagePartBody {
                        attachment_id: Some("a1".into()),
                        size: Some(12),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        let (plain, html, attachments) = extract_parts(&part);
        assert_eq!(plain, "plain");
        assert_eq!(html, "<p>html</p>");
        assert_eq!(attachments[0].id, "a1");
    }

    #[test]
    fn attachment_path_rejects_traversal() {
        assert!(safe_attachment_path(Path::new("/tmp/attachments"), "../secret").is_err());
        assert!(safe_attachment_path(Path::new("/tmp/attachments"), "generated.bin").is_ok());
    }

    #[test]
    fn unavailable_summary_preserves_message_content() {
        let message = Message {
            id: Some("message-id".into()),
            ..Default::default()
        };
        let summary = SummaryResult {
            status: crate::contracts::SummaryStatus::Unavailable,
            text: None,
            model: "gpt-6-luna".into(),
            failure_code: Some(FailureCode::SummaryError),
        };
        let output = message_out(
            message,
            &[],
            "important message body".into(),
            Vec::new(),
            summary,
        );
        assert_eq!(output.content, "important message body");
        assert_eq!(
            output.summary.status,
            crate::contracts::SummaryStatus::Unavailable
        );
    }
}
