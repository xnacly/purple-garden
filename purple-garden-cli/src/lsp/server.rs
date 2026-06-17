use std::collections::HashMap;

use lsp_server::{
    Connection, ErrorCode, ExtractError, Message, Notification, Request, RequestId, Response,
    ResponseError,
};
use lsp_types::{
    CodeActionProviderCapability, CompletionOptions, CompletionResponse, DiagnosticOptions,
    FullDocumentDiagnosticReport, HoverProviderCapability, InitializeParams, SaveOptions,
    ServerCapabilities, TextDocumentSyncKind, TextDocumentSyncOptions,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    },
    request::{Completion, DocumentDiagnosticRequest, HoverRequest},
};

use super::analysis::DocumentState;

type LspResult<T> = Result<T, Box<dyn std::error::Error>>;

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

    event_loop(connection, init_params)?;
    threads.join().map_err(|_| "failed to join lsp threads")?;
    lsp_log!("shutting down language server");
    Ok(())
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![".".to_owned()]),
            ..Default::default()
        }),
        diagnostic_provider: Some(lsp_types::DiagnosticServerCapabilities::Options(
            DiagnosticOptions {
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..Default::default()
            },
        )),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
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

fn event_loop(connection: Connection, params: serde_json::Value) -> LspResult<()> {
    let _: InitializeParams = serde_json::from_value(params)?;
    let mut documents = HashMap::<String, DocumentState>::new();
    lsp_log!("starting event loop");

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                handle_request(&connection, &documents, req)?;
            }
            Message::Response(_) => {}
            Message::Notification(not) => handle_notification(&mut documents, not),
        }
    }

    Ok(())
}

fn handle_request(
    connection: &Connection,
    documents: &HashMap<String, DocumentState>,
    req: Request,
) -> LspResult<()> {
    match req.method.as_str() {
        "textDocument/completion" => handle_completion(connection, documents, req),
        "textDocument/hover" => handle_hover(connection, documents, req),
        "textDocument/diagnostic" => handle_diagnostic(connection, documents, req),
        "textDocument/codeAction" => handle_code_action(connection, documents, req),
        _ => send_error(
            connection,
            req.id,
            ErrorCode::MethodNotFound,
            format!("unsupported method '{}'", req.method),
        ),
    }
}

fn handle_code_action(
    connection: &Connection,
    documents: &HashMap<String, DocumentState>,
    req: Request,
) -> LspResult<()> {
    let id = req.id.clone();
    let (id, params) = match cast::<lsp_types::request::CodeActionRequest>(req) {
        Ok(v) => v,
        Err(err) => return send_request_error(connection, id, err),
    };
    let actions = documents
        .get(&params.text_document.uri.to_string())
        .map(|state| state.code_actions(params.text_document.uri, params.range));
    send_response(connection, id, actions)
}

fn handle_completion(
    connection: &Connection,
    documents: &HashMap<String, DocumentState>,
    req: Request,
) -> LspResult<()> {
    let id = req.id.clone();
    let (id, params) = match cast::<Completion>(req) {
        Ok(v) => v,
        Err(err) => return send_request_error(connection, id, err),
    };
    let pos = params.text_document_position;
    let completions = documents
        .get(&pos.text_document.uri.to_string())
        .map(|state| state.completions_at(pos.position))
        .unwrap_or_default();
    send_response(connection, id, Some(CompletionResponse::Array(completions)))
}

fn handle_hover(
    connection: &Connection,
    documents: &HashMap<String, DocumentState>,
    req: Request,
) -> LspResult<()> {
    let id = req.id.clone();
    let (id, params) = match cast::<HoverRequest>(req) {
        Ok(v) => v,
        Err(err) => return send_request_error(connection, id, err),
    };
    let pos = params.text_document_position_params;
    let hover = documents
        .get(&pos.text_document.uri.to_string())
        .and_then(|state| state.hover_at(pos.position));
    send_response(connection, id, hover)
}

fn handle_diagnostic(
    connection: &Connection,
    documents: &HashMap<String, DocumentState>,
    req: Request,
) -> LspResult<()> {
    let id = req.id.clone();
    let (id, params) = match cast::<DocumentDiagnosticRequest>(req) {
        Ok(v) => v,
        Err(err) => return send_request_error(connection, id, err),
    };
    let diagnostics = documents
        .get(&params.text_document.uri.to_string())
        .map(DocumentState::diagnostics)
        .unwrap_or_default();
    send_response(
        connection,
        id,
        FullDocumentDiagnosticReport {
            result_id: None,
            items: diagnostics,
        },
    )
}

fn handle_notification(documents: &mut HashMap<String, DocumentState>, not: Notification) {
    match not.method.as_str() {
        "textDocument/didOpen" => match cast_noti::<DidOpenTextDocument>(not) {
            Ok(params) => {
                let uri = params.text_document.uri.to_string();
                documents.insert(uri, DocumentState::analyze(params.text_document.text));
            }
            Err(err) => lsp_log!("failed to parse notification: {}", err),
        },
        "textDocument/didChange" => match cast_noti::<DidChangeTextDocument>(not) {
            Ok(params) => {
                if let Some(change) = params.content_changes.into_iter().next() {
                    let uri = params.text_document.uri.to_string();
                    documents.insert(uri, DocumentState::analyze(change.text));
                }
            }
            Err(err) => lsp_log!("failed to parse notification: {}", err),
        },
        "textDocument/didSave" => match cast_noti::<DidSaveTextDocument>(not) {
            Ok(params) => {
                if let Some(text) = params.text {
                    let uri = params.text_document.uri.to_string();
                    documents.insert(uri, DocumentState::analyze(text));
                }
            }
            Err(err) => lsp_log!("failed to parse notification: {}", err),
        },
        "textDocument/didClose" => match cast_noti::<DidCloseTextDocument>(not) {
            Ok(params) => {
                documents.remove(&params.text_document.uri.to_string());
            }
            Err(err) => lsp_log!("failed to parse notification: {}", err),
        },
        "$/cancelRequest" => {}
        _ => lsp_log!("unsupported notification '{}'", not.method),
    }
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
