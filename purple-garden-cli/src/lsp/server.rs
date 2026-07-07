use std::{collections::HashMap, path::PathBuf};

use lsp_server::{
    Connection, ErrorCode, ExtractError, Message, Notification, Request, RequestId, Response,
    ResponseError,
};
use lsp_types::{
    CodeActionProviderCapability, CompletionOptions, CompletionResponse, DiagnosticOptions,
    FullDocumentDiagnosticReport, HoverProviderCapability, InitializeParams, OneOf,
    PublishDiagnosticsParams, SaveOptions, ServerCapabilities, TextDocumentSyncKind,
    TextDocumentSyncOptions, Uri,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    },
    request::{Completion, DocumentDiagnosticRequest, GotoDefinition, HoverRequest},
};

use super::analysis::DocumentState;
use super::source::apply_content_changes;

type LspResult<T> = Result<T, Box<dyn std::error::Error>>;

struct Server {
    connection: Connection,
    documents: OpenDocuments,
    shutdown_requested: bool,
}

#[derive(Default)]
struct OpenDocuments {
    states: HashMap<String, DocumentState>,
}

macro_rules! lsp_log {
    ($literal:literal) => {
        eprintln!("[purple-garden]: {}", $literal)
    };
    ($fmt:literal, $($arg:tt)*) => {
        eprintln!(concat!("[purple-garden]: ", $fmt), $($arg)*)
    };
}

pub(super) fn run() -> LspResult<()> {
    lsp_log!("starting language server");
    let (connection, threads) = Connection::stdio();
    let capabilities = serde_json::to_value(server_capabilities())?;

    let init_params = match connection.initialize(capabilities) {
        Ok(params) => params,
        Err(err) => {
            if err.channel_is_disconnected() {
                threads.join().map_err(|_| "failed to join lsp threads")?;
            }
            return Err(err.into());
        }
    };

    Server::new(connection).run(init_params)?;
    threads.join().map_err(|_| "failed to join lsp threads")?;
    lsp_log!("shutting down language server");
    Ok(())
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(completion_trigger_characters()),
            ..Default::default()
        }),
        diagnostic_provider: Some(lsp_types::DiagnosticServerCapabilities::Options(
            DiagnosticOptions {
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..Default::default()
            },
        )),
        definition_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(
                    SaveOptions {
                        include_text: Some(true),
                    },
                )),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
}

// Some clients only request completions automatically for advertised trigger characters.
// Keep this in sync with completion::is_completion_char plus import-string completions.
fn completion_trigger_characters() -> Vec<String> {
    ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain(['_', '/', '.', '"'])
        .map(|ch| ch.to_string())
        .collect()
}

impl Server {
    fn new(connection: Connection) -> Self {
        Self {
            connection,
            documents: OpenDocuments::default(),
            shutdown_requested: false,
        }
    }

    fn run(mut self, params: serde_json::Value) -> LspResult<()> {
        let _: InitializeParams = serde_json::from_value(params)?;
        lsp_log!("starting event loop");

        while let Ok(msg) = self.connection.receiver.recv() {
            self.handle_message(msg)?;
            if self.shutdown_requested {
                break;
            }
        }

        Ok(())
    }

    fn handle_message(&mut self, msg: Message) -> LspResult<()> {
        match msg {
            Message::Request(req) => self.handle_request(req),
            Message::Response(_) => Ok(()),
            Message::Notification(not) => self.handle_notification(not),
        }
    }

    // Request handlers should stay thin: parse params, read the current document snapshot,
    // then serialize the feature-specific response.
    fn handle_request(&mut self, req: Request) -> LspResult<()> {
        if self.connection.handle_shutdown(&req)? {
            self.shutdown_requested = true;
            return Ok(());
        }

        match req.method.as_str() {
            "textDocument/completion" => self.completion(req),
            "textDocument/hover" => self.hover(req),
            "textDocument/definition" => self.definition(req),
            "textDocument/diagnostic" => self.diagnostic(req),
            "textDocument/codeAction" => self.code_action(req),
            _ => send_error(
                &self.connection,
                req.id,
                ErrorCode::MethodNotFound,
                format!("unsupported method '{}'", req.method),
            ),
        }
    }

    fn code_action(&self, req: Request) -> LspResult<()> {
        let Some((id, params)) =
            request_params::<lsp_types::request::CodeActionRequest>(&self.connection, req)?
        else {
            return Ok(());
        };
        let actions = self
            .documents
            .get(&params.text_document.uri)
            .map(|state| state.code_actions(params.text_document.uri, params.range));
        send_response(&self.connection, id, actions)
    }

    fn completion(&self, req: Request) -> LspResult<()> {
        let Some((id, params)) = request_params::<Completion>(&self.connection, req)? else {
            return Ok(());
        };
        let pos = params.text_document_position;
        let completions = self
            .documents
            .get(&pos.text_document.uri)
            .map(|state| state.completions_at(pos.position))
            .unwrap_or_default();
        send_response(
            &self.connection,
            id,
            Some(CompletionResponse::Array(completions)),
        )
    }

    fn hover(&self, req: Request) -> LspResult<()> {
        let Some((id, params)) = request_params::<HoverRequest>(&self.connection, req)? else {
            return Ok(());
        };
        let pos = params.text_document_position_params;
        let hover = self
            .documents
            .get(&pos.text_document.uri)
            .and_then(|state| state.hover_at(pos.position));
        send_response(&self.connection, id, hover)
    }

    fn definition(&self, req: Request) -> LspResult<()> {
        let Some((id, params)) = request_params::<GotoDefinition>(&self.connection, req)? else {
            return Ok(());
        };
        let pos = params.text_document_position_params;
        let definition = self
            .documents
            .get(&pos.text_document.uri)
            .and_then(|state| state.definition_at(pos.text_document.uri, pos.position));
        send_response(&self.connection, id, definition)
    }

    fn diagnostic(&self, req: Request) -> LspResult<()> {
        let Some((id, params)) =
            request_params::<DocumentDiagnosticRequest>(&self.connection, req)?
        else {
            return Ok(());
        };
        let diagnostics = self
            .documents
            .get(&params.text_document.uri)
            .map(DocumentState::diagnostics)
            .unwrap_or_default();
        send_response(
            &self.connection,
            id,
            FullDocumentDiagnosticReport {
                result_id: None,
                items: diagnostics,
            },
        )
    }

    // Notifications are the only protocol messages that mutate the open-document map.
    fn handle_notification(&mut self, not: Notification) -> LspResult<()> {
        match not.method.as_str() {
            "textDocument/didOpen" => self.did_open(not),
            "textDocument/didChange" => self.did_change(not),
            "textDocument/didSave" => self.did_save(not),
            "textDocument/didClose" => self.did_close(not),
            "$/cancelRequest" => Ok(()),
            _ => {
                lsp_log!("unsupported notification '{}'", not.method);
                Ok(())
            }
        }
    }

    fn did_open(&mut self, not: Notification) -> LspResult<()> {
        let Some(params) = notification_params::<DidOpenTextDocument>(not) else {
            return Ok(());
        };
        self.update_document(params.text_document.uri, params.text_document.text)
    }

    fn did_change(&mut self, not: Notification) -> LspResult<()> {
        let Some(params) = notification_params::<DidChangeTextDocument>(not) else {
            return Ok(());
        };
        let uri = params.text_document.uri;
        let diagnostics = self.documents.change(uri.clone(), params.content_changes);
        publish_diagnostics(&self.connection, uri, diagnostics)
    }

    fn did_save(&mut self, not: Notification) -> LspResult<()> {
        let Some(params) = notification_params::<DidSaveTextDocument>(not) else {
            return Ok(());
        };
        if let Some(text) = params.text {
            self.update_document(params.text_document.uri, text)?;
        }
        Ok(())
    }

    fn did_close(&mut self, not: Notification) -> LspResult<()> {
        let Some(params) = notification_params::<DidCloseTextDocument>(not) else {
            return Ok(());
        };
        let uri = params.text_document.uri;
        self.documents.close(&uri);
        publish_diagnostics(&self.connection, uri, Vec::new())
    }

    fn update_document(&mut self, uri: Uri, text: String) -> LspResult<()> {
        let diagnostics = self.documents.update(uri.clone(), text);
        publish_diagnostics(&self.connection, uri, diagnostics)
    }
}

impl OpenDocuments {
    fn get(&self, uri: &Uri) -> Option<&DocumentState> {
        self.states.get(&document_key(uri))
    }

    // Re-analyze on every document update. This keeps feature handlers read-only and simple.
    fn update(&mut self, uri: Uri, text: String) -> Vec<lsp_types::Diagnostic> {
        let path = path_from_uri(&uri);
        let state = DocumentState::analyze(path, text);
        let diagnostics = state.diagnostics();
        self.states.insert(document_key(&uri), state);
        diagnostics
    }

    // LSP incremental edits are position-based; convert them into a full source snapshot and
    // reuse the same update path as open/save.
    fn change(
        &mut self,
        uri: Uri,
        changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
    ) -> Vec<lsp_types::Diagnostic> {
        let mut text = self
            .get(&uri)
            .map_or_else(String::new, |state| state.text().to_owned());
        apply_content_changes(&mut text, changes);
        self.update(uri, text)
    }

    fn close(&mut self, uri: &Uri) {
        self.states.remove(&document_key(uri));
    }
}

fn document_key(uri: &Uri) -> String {
    uri.to_string()
}

fn path_from_uri(uri: &Uri) -> Option<PathBuf> {
    let raw = uri.to_string();
    let path = raw.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(path)?))
}

fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = *bytes.get(i + 1)?;
            let lo = *bytes.get(i + 2)?;
            decoded.push(hex(hi)? * 16 + hex(lo)?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn publish_diagnostics(
    connection: &Connection,
    uri: Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> LspResult<()> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".to_owned(),
            params,
        )))?;
    Ok(())
}

fn send_response<T: serde::Serialize>(
    connection: &Connection,
    id: RequestId,
    result: T,
) -> LspResult<()> {
    let resp = Response {
        id,
        result: Some(serde_json::to_value(result)?),
        error: None,
    };
    connection.sender.send(Message::Response(resp))?;
    Ok(())
}

fn send_request_error(
    connection: &Connection,
    id: RequestId,
    err: ExtractError<Request>,
) -> LspResult<()> {
    send_error(connection, id, ErrorCode::InvalidParams, err.to_string())
}

fn request_params<R>(
    connection: &Connection,
    req: Request,
) -> LspResult<Option<(RequestId, R::Params)>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    let id = req.id.clone();
    match cast::<R>(req) {
        Ok(params) => Ok(Some(params)),
        Err(err) => {
            send_request_error(connection, id, err)?;
            Ok(None)
        }
    }
}

fn notification_params<N>(not: Notification) -> Option<N::Params>
where
    N: lsp_types::notification::Notification,
    N::Params: serde::de::DeserializeOwned,
{
    match cast_noti::<N>(not) {
        Ok(params) => Some(params),
        Err(err) => {
            lsp_log!("failed to parse notification: {}", err);
            None
        }
    }
}

fn send_error(
    connection: &Connection,
    id: RequestId,
    code: ErrorCode,
    message: String,
) -> LspResult<()> {
    let resp = Response {
        id,
        result: None,
        error: Some(ResponseError {
            code: code as i32,
            message,
            data: None,
        }),
    };
    connection.sender.send(Message::Response(resp))?;
    Ok(())
}

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}

fn cast_noti<N>(not: Notification) -> Result<N::Params, ExtractError<Notification>>
where
    N: lsp_types::notification::Notification,
    N::Params: serde::de::DeserializeOwned,
{
    not.extract(N::METHOD)
}
