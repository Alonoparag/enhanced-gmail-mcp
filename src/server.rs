use rmcp::{
    Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ErrorData, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};

use crate::{
    contracts::{
        DownloadAttachmentInput, DownloadedAttachment, GmailLabel, GmailMessageOutput,
        GmailThreadOutput, IdInput, LabelInput, LabelsOutput, ListMessagesInput, MessagePage,
        ModifyLabelsInput, MutationResult, ReadInput, UpdateLabelInput,
    },
    gmail::GmailClient,
};

#[derive(Clone)]
pub struct GmailMcpServer {
    gmail: GmailClient,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl GmailMcpServer {
    pub fn new(gmail: GmailClient) -> Self {
        Self {
            gmail,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List Gmail messages using query, label, inbox, and pagination filters")]
    async fn list_messages(
        &self,
        Parameters(input): Parameters<ListMessagesInput>,
    ) -> Result<Json<MessagePage>, ErrorData> {
        self.gmail
            .list_messages(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(
        description = "Read one Gmail message, optionally returning only identifiers and summary state"
    )]
    async fn get_message(
        &self,
        Parameters(input): Parameters<ReadInput>,
    ) -> Result<Json<GmailMessageOutput>, ErrorData> {
        self.gmail
            .get_message(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(
        description = "Read a Gmail thread, optionally returning only identifiers and summary states"
    )]
    async fn get_thread(
        &self,
        Parameters(input): Parameters<ReadInput>,
    ) -> Result<Json<GmailThreadOutput>, ErrorData> {
        self.gmail
            .get_thread(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "List Gmail labels")]
    async fn list_labels(&self) -> Result<Json<LabelsOutput>, ErrorData> {
        self.gmail
            .list_labels()
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Create a Gmail label")]
    async fn create_label(
        &self,
        Parameters(input): Parameters<LabelInput>,
    ) -> Result<Json<GmailLabel>, ErrorData> {
        self.gmail
            .create_label(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Replace a Gmail label's mutable fields")]
    async fn update_label(
        &self,
        Parameters(input): Parameters<UpdateLabelInput>,
    ) -> Result<Json<GmailLabel>, ErrorData> {
        self.gmail
            .update_label(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Delete a Gmail user label")]
    async fn delete_label(
        &self,
        Parameters(input): Parameters<IdInput>,
    ) -> Result<Json<MutationResult>, ErrorData> {
        self.gmail
            .delete_label(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Add and remove labels on one Gmail message")]
    async fn modify_message_labels(
        &self,
        Parameters(input): Parameters<ModifyLabelsInput>,
    ) -> Result<Json<MutationResult>, ErrorData> {
        self.gmail
            .modify_message_labels(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Add and remove labels on one Gmail thread")]
    async fn modify_thread_labels(
        &self,
        Parameters(input): Parameters<ModifyLabelsInput>,
    ) -> Result<Json<MutationResult>, ErrorData> {
        self.gmail
            .modify_thread_labels(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Archive a Gmail message by removing its INBOX label")]
    async fn archive_message(
        &self,
        Parameters(input): Parameters<IdInput>,
    ) -> Result<Json<MutationResult>, ErrorData> {
        self.gmail
            .archive_message(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Mark a Gmail message as spam")]
    async fn mark_message_spam(
        &self,
        Parameters(input): Parameters<IdInput>,
    ) -> Result<Json<MutationResult>, ErrorData> {
        self.gmail
            .mark_message_spam(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }

    #[tool(description = "Download a Gmail attachment into the server-owned attachment root")]
    async fn download_attachment(
        &self,
        Parameters(input): Parameters<DownloadAttachmentInput>,
    ) -> Result<Json<DownloadedAttachment>, ErrorData> {
        self.gmail
            .download_attachment(input)
            .await
            .map(Json)
            .map_err(|error| error.into_mcp())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GmailMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions("Read and organize the authenticated Gmail mailbox. Message sending and persistent Gmail filter management are outside this server's scope.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{configuration::ConfigurationState, summary::Summarizer};
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientConfig},
    };

    #[test]
    fn discovery_exposes_all_v1_tools_with_object_schemas() {
        let tools = GmailMcpServer::tool_router().list_all();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "archive_message",
                "create_label",
                "delete_label",
                "download_attachment",
                "get_message",
                "get_thread",
                "list_labels",
                "list_messages",
                "mark_message_spam",
                "modify_message_labels",
                "modify_thread_labels",
                "update_label",
            ]
        );
        assert!(tools.iter().all(|tool| {
            tool.input_schema
                .get("type")
                .and_then(|value| value.as_str())
                == Some("object")
        }));
        for name in ["get_message", "get_thread"] {
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            let summaries_only = &tool.input_schema["properties"]["summaries_only"];
            assert_eq!(summaries_only["type"], "boolean");
            assert_eq!(summaries_only["default"], false);
            assert!(
                !tool.input_schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|field| field == "summaries_only")
            );
        }
    }

    #[tokio::test]
    async fn direct_mcp_smoke_initializes_discovers_and_routes_every_tool() {
        let temp = tempfile::tempdir().unwrap();
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            temp.path().join("smoke.db").display()
        );
        let configuration = ConfigurationState::new(&database_url, "user".into(), "pass".into())
            .await
            .unwrap();
        let groq_secret = temp.path().join("groq");
        tokio::fs::write(&groq_secret, "unused-smoke-key")
            .await
            .unwrap();
        let summarizer = Summarizer::from_secret_files(
            &temp.path().join("missing-openai"),
            Some((&groq_secret, "unused-smoke-model".into())),
            configuration.pool(),
        )
        .await
        .unwrap();
        let gmail = GmailClient::for_mcp_smoke(summarizer, temp.path().join("attachments"))
            .await
            .unwrap();

        let (client_to_server, server_from_client) = tokio::io::duplex(64 * 1024);
        let (server_to_client, client_from_server) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            GmailMcpServer::new(gmail)
                .serve((server_from_client, server_to_client))
                .await
                .unwrap()
                .waiting()
                .await
        });
        let client = rmcp::serve_client(
            ClientConfig::default(),
            (client_from_server, client_to_server),
        )
        .await
        .unwrap();

        let tools = client.list_tools(None).await.unwrap().tools;
        assert_eq!(tools.len(), 12);
        for name in ["get_message", "get_thread"] {
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            let summaries_only = &tool.input_schema["properties"]["summaries_only"];
            assert_eq!(summaries_only["type"], "boolean");
            assert_eq!(summaries_only["default"], false);
        }
        for tool in tools {
            let result = client
                .call_tool(
                    CallToolRequestParams::new(tool.name.to_string())
                        .with_arguments(serde_json::Map::new()),
                )
                .await;
            assert!(
                result.is_err() || result.unwrap().is_error == Some(true),
                "{} unexpectedly succeeded",
                tool.name
            );
        }

        client.cancel().await.unwrap();
        server.await.unwrap().unwrap();
    }
}
