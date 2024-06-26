use std::cmp::max;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    ExecuteCommandOptions, ExecuteCommandParams, InitializeParams, InitializeResult,
    InitializedParams, MessageType, Position, Range, SaveOptions, ServerCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TextEdit, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

use crate::operations::{Complete, Document, Fix, Instruct, Optimize, Suggest};

#[derive(Clone, Copy, Debug, PartialEq)]
enum AiCodeAction {
    Instruct,
    Document,
    Fix,
    Optimize,
    Suggest,
    FillInMiddle,
    Test,
}

impl AiCodeAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Instruct => "Acai - Instruct",
            Self::Document => "Acai - Document",
            Self::Fix => "Acai - Fix",
            Self::Optimize => "Acai - Optimize",
            Self::Suggest => "Acai - Suggest",
            Self::FillInMiddle => "Acai - Fill in middle",
            Self::Test => "Acai - Test",
        }
    }

    /// Returns the identifier of the command.
    const fn identifier(self) -> &'static str {
        match self {
            Self::Instruct => "ai.instruct",
            Self::Document => "ai.document",
            Self::Fix => "ai.fix",
            Self::Optimize => "ai.optimize",
            Self::Suggest => "ai.suggest",
            Self::FillInMiddle => "ai.fillInMiddle",
            Self::Test => "ai.test",
        }
    }

    /// Returns all the commands that the server currently supports.
    const fn all() -> [Self; 7] {
        [
            Self::Instruct,
            Self::Document,
            Self::Fix,
            Self::Optimize,
            Self::Suggest,
            Self::FillInMiddle,
            Self::Test,
        ]
    }
}

impl FromStr for AiCodeAction {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> anyhow::Result<Self, Self::Err> {
        Ok(match name {
            "ai.instruct" => Self::Instruct,
            "ai.document" => Self::Document,
            "ai.fix" => Self::Fix,
            "ai.optimize" => Self::Optimize,
            "ai.suggest" => Self::Suggest,
            "ai.fillInMiddle" => Self::FillInMiddle,
            "ai.test" => Self::Test,
            _ => return Err(anyhow::anyhow!("Invalid command `{name}`")),
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct CodeActionData {
    id: String,
    document_uri: Url,
    range: Range,
}

#[derive(Debug)]
struct State {
    sources: HashMap<Url, String>,
}

impl State {
    fn new() -> Self {
        Self {
            sources: HashMap::new(),
        }
    }

    fn insert_source(&mut self, document: &TextDocumentItem) {
        if !self.sources.contains_key(&document.uri) {
            self.sources
                .insert(document.uri.clone(), document.text.clone());
        }
    }

    fn update_source(&mut self, document: &TextDocumentIdentifier, text: Option<String>) {
        if let Some(text) = text {
            self.sources.insert(document.uri.clone(), text);
        }
    }

    fn reload_source(
        &mut self,
        document: &VersionedTextDocumentIdentifier,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) {
        if let Some(src) = self.sources.get(&document.uri) {
            let mut source = src.to_owned();
            for change in changes {
                if (change.range, change.range_length) == (None, None) {
                    source = change.text;
                } else if let Some(range) = change.range {
                    let mut lines: Vec<&str> = source.lines().collect();
                    let new_lines: Vec<&str> = change.text.lines().collect();
                    let start = usize::try_from(range.start.line).unwrap();
                    let end = usize::try_from(range.end.line).unwrap();
                    lines.splice(start..end, new_lines);
                    source = lines.join("\n");
                }
            }
            self.sources.insert(document.uri.clone(), source);
        } else {
            panic!("attempted to reload source that does not exist");
        }
    }

    fn get_source_range(&self, document_uri: &Url, range: &Range) -> Option<String> {
        self.sources.get(document_uri).and_then(|src| {
            let source = src.to_owned();
            let lines: Vec<&str> = source.lines().collect();
            let start = usize::try_from(range.start.line).unwrap();
            let end = usize::try_from(range.end.line).unwrap();
            let range_lines = lines.get(start..end);

            range_lines.map(|target_lines| target_lines.join("\n"))
        })
    }
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    state: Arc<Mutex<State>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(Mutex::new(State::new())),
        }
    }

    async fn on_code_action(&self, params: CodeActionParams) -> CodeActionResponse {
        self.client
            .log_message(MessageType::INFO, "on code action")
            .await;

        let text_doc = params.text_document;
        let document_uri = text_doc.uri;
        let range = params.range;
        // let diagnostics = params.context.diagnostics;
        // let error_id_to_ranges = build_error_id_to_ranges(diagnostics);

        let mut response = CodeActionResponse::new();

        let code_actions = AiCodeAction::all();

        for code_action in &code_actions {
            let action = CodeAction {
                title: code_action.label().to_string(),
                command: None,
                diagnostics: None,
                edit: None,
                disabled: None,
                kind: Some(CodeActionKind::QUICKFIX),
                is_preferred: Some(true),
                data: Some(serde_json::json!(CodeActionData {
                    id: code_action.identifier().to_string(),
                    document_uri: document_uri.clone(),
                    range,
                })),
            };
            response.push(CodeActionOrCommand::from(action));
        }

        response
    }

    async fn on_code_action_resolve(&self, params: CodeAction) -> CodeAction {
        let mut new_params = params.clone();

        let data = params.data;

        let code_action_data = data.map_or_else(
            || None,
            |json_obj| {
                let result: core::result::Result<CodeActionData, serde_json::Error> =
                    serde_json::from_value::<CodeActionData>(json_obj);
                Some(result)
            },
        );

        let args = if let Some(some_cad) = code_action_data {
            match some_cad {
                Ok(cad) => {
                    self.client
                        .log_message(MessageType::INFO, format!("Range {:#?}", &cad.range))
                        .await;

                    let context = self
                        .state
                        .lock()
                        .await
                        .get_source_range(&cad.document_uri, &cad.range);

                    Some((cad.document_uri.clone(), cad.range, context, cad.id))
                }
                Err(err) => {
                    self.client.log_message(MessageType::ERROR, err).await;
                    None
                }
            }
        } else {
            None
        };

        if let Some(arg) = args {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Executing {}", params.title.as_str()),
                )
                .await;

            let document_uri = arg.0;
            let range = arg.1;
            let context = arg.2;
            let id = arg.3;

            self.client
                .log_message(MessageType::INFO, format!("Context {context:?}"))
                .await;

            let response = execute_operation(id, context).await;

            if let Some(str_edit) = response {
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

                let edits = changes.entry(document_uri).or_default();

                let edit = TextEdit {
                    range,
                    new_text: str_edit,
                };

                edits.push(edit);

                let edit = Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                });

                new_params.edit = edit;
            }
        }

        new_params
    }
}

async fn execute_operation(op_title: String, context: Option<String>) -> Option<String> {
    let code_action = AiCodeAction::from_str(op_title.as_str()).unwrap();

    if matches!(code_action, AiCodeAction::Test) {
        return None::<String>;
    }

    if matches!(code_action, AiCodeAction::FillInMiddle) {
        let response = Complete {
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            prompt: None,
            context,
        }
        .send()
        .await;

        return if let Ok(Some(response_msg)) = response {
            Some(response_msg)
        } else {
            None
        };
    }

    let result = match code_action {
        AiCodeAction::Instruct => Some(
            Instruct {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            }
            .send()
            .await,
        ),
        AiCodeAction::Document => Some(
            Document {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            }
            .send()
            .await,
        ),
        AiCodeAction::Fix => Some(
            Fix {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            }
            .send()
            .await,
        ),
        AiCodeAction::Optimize => Some(
            Optimize {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            }
            .send()
            .await,
        ),
        AiCodeAction::Suggest => Some(
            Suggest {
                model: None,
                temperature: None,
                max_tokens: None,
                top_p: None,
                prompt: None,
                context,
            }
            .send()
            .await,
        ),
        _ => None,
    };

    result.and_then(|response| response.map_or(None, |result| result.map(|msg| msg.content)))
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Initializing {:?}", params.root_uri.unwrap().path()),
            )
            .await;

        // Text Document Sync Configuration
        let text_document_sync = TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                include_text: Some(true),
            })),
            ..TextDocumentSyncOptions::default()
        });

        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(text_document_sync),
                // completion_provider: Some(CompletionOptions {
                //     resolve_provider: Some(true),
                //     trigger_characters: Some(vec![".".to_owned(), ":".to_owned()]),
                //     work_done_progress_options: WorkDoneProgressOptions::default(),
                //     all_commit_characters: None,
                //     ..Default::default()
                // }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["codingassistant/instruct".to_owned()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        resolve_provider: Some(true),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                // Some(CodeActionProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(MessageType::INFO, "workspace folders changed!")
            .await;
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.client
            .log_message(MessageType::INFO, "configuration changed!")
            .await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.client
            .log_message(MessageType::INFO, "watched files have changed!")
            .await;
    }

    async fn execute_command(&self, _: ExecuteCommandParams) -> Result<Option<Value>> {
        self.client
            .log_message(MessageType::INFO, "command executed!")
            .await;

        match self.client.apply_edit(WorkspaceEdit::default()).await {
            Ok(res) if res.applied => self.client.log_message(MessageType::INFO, "applied").await,
            Ok(_) => self.client.log_message(MessageType::INFO, "rejected").await,
            Err(err) => self.client.log_message(MessageType::ERROR, err).await,
        }

        Ok(None)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file opened! {}", params.text_document.uri),
            )
            .await;

        self.state.lock().await.insert_source(&params.text_document);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file changed! {}", params.text_document.uri),
            )
            .await;

        // reload_source(&self.state, &params.text_document, params.content_changes).await;
    }

    // Test
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("file saved! {}", params.text_document.uri),
            )
            .await;

        // Update source
        self.state
            .lock()
            .await
            .update_source(&params.text_document, params.text.clone());

        self.client
            .log_message(
                MessageType::INFO,
                format!("file saved on server! {:#?}", params.text),
            )
            .await;
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.client
            .log_message(MessageType::INFO, "code action!")
            .await;

        Ok(Some(self.on_code_action(params).await))
    }

    async fn code_action_resolve(&self, params: CodeAction) -> Result<CodeAction> {
        self.client
            .log_message(MessageType::INFO, "code action resolve!")
            .await;

        Ok(self.on_code_action_resolve(params).await)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.client
            .log_message(MessageType::INFO, "completion")
            .await;

        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(MessageType::INFO, uri.clone())
            .await;

        let range = Range {
            start: Position {
                line: max(position.line - 3, 0),
                character: 0,
            },
            end: position,
        };

        let context = self.state.lock().await.get_source_range(&uri, &range);

        self.client
            .log_message(MessageType::INFO, context.clone().unwrap())
            .await;

        let op = Complete {
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            prompt: None,
            context,
        };

        let response = op.send().await;

        let msg = if let Ok(Some(response_msg)) = response {
            Some(response_msg)
        } else {
            None
        };

        self.client
            .log_message(MessageType::INFO, msg.clone().unwrap())
            .await;

        msg.map_or(Ok(None), |msg| {
            Ok(Some(CompletionResponse::Array(vec![
                CompletionItem::new_simple(msg.clone(), msg),
            ])))
        })
    }
}
