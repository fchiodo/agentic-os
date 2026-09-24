use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::engine::{EngineFuture, EngineRequest, OcrEngine, ProgressReporter};
use super::errors::{ConverterError, ConverterResult};
use super::protocol::{encode_request, parse_message, SidecarMessage, SidecarRequest};
use super::types::{
    ConversionProgress, EngineConversion, OCR_ENGINE_ID, OCR_SIDECAR_VERSION,
    SIDECAR_PROTOCOL_VERSION,
};

pub const KEEP_WARM_SECONDS: u64 = 5 * 60;
const STARTUP_TIMEOUT_SECONDS: u64 = 45;

enum ActorCommand {
    Convert {
        request: EngineRequest,
        progress: ProgressReporter,
        reply: oneshot::Sender<ConverterResult<EngineConversion>>,
    },
    Cancel {
        job_id: String,
        reply: oneshot::Sender<ConverterResult<()>>,
    },
    Shutdown {
        reply: oneshot::Sender<ConverterResult<()>>,
    },
}

struct RunningProcess {
    receiver: tauri::async_runtime::Receiver<CommandEvent>,
    child: CommandChild,
    process_group_id: Option<i32>,
}

#[derive(Clone)]
pub struct PaddleOcrEngine {
    sender: mpsc::UnboundedSender<ActorCommand>,
}

impl PaddleOcrEngine {
    pub fn new(app: AppHandle) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(actor_loop(app, receiver));
        Self { sender }
    }
}

impl OcrEngine for PaddleOcrEngine {
    fn id(&self) -> &'static str {
        OCR_ENGINE_ID
    }

    fn version(&self) -> String {
        OCR_SIDECAR_VERSION.to_string()
    }

    fn is_available(&self) -> bool {
        std::env::consts::OS == "macos" && std::env::consts::ARCH == "aarch64"
    }

    fn convert<'a>(
        &'a self,
        request: EngineRequest,
        progress: ProgressReporter,
    ) -> EngineFuture<'a, EngineConversion> {
        Box::pin(async move {
            let (reply, response) = oneshot::channel();
            self.sender
                .send(ActorCommand::Convert {
                    request,
                    progress,
                    reply,
                })
                .map_err(|_| {
                    ConverterError::new("SIDECAR_CRASHED", "Document AI is unavailable")
                })?;
            response.await.map_err(|_| {
                ConverterError::new("SIDECAR_CRASHED", "Document AI stopped unexpectedly")
            })?
        })
    }

    fn cancel<'a>(&'a self, job_id: &'a str) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            let (reply, response) = oneshot::channel();
            self.sender
                .send(ActorCommand::Cancel {
                    job_id: job_id.to_string(),
                    reply,
                })
                .map_err(|_| {
                    ConverterError::new("SIDECAR_CRASHED", "Document AI is unavailable")
                })?;
            response.await.map_err(|_| {
                ConverterError::new("SIDECAR_CRASHED", "Document AI stopped unexpectedly")
            })?
        })
    }

    fn shutdown<'a>(&'a self) -> EngineFuture<'a, ()> {
        Box::pin(async move {
            let (reply, response) = oneshot::channel();
            self.sender
                .send(ActorCommand::Shutdown { reply })
                .map_err(|_| {
                    ConverterError::new("SIDECAR_CRASHED", "Document AI is unavailable")
                })?;
            response.await.map_err(|_| {
                ConverterError::new("SIDECAR_CRASHED", "Document AI stopped unexpectedly")
            })?
        })
    }
}

async fn actor_loop(app: AppHandle, mut commands: mpsc::UnboundedReceiver<ActorCommand>) {
    let mut process: Option<RunningProcess> = None;
    loop {
        let command = if process.is_some() {
            match tokio::time::timeout(Duration::from_secs(KEEP_WARM_SECONDS), commands.recv())
                .await
            {
                Ok(command) => command,
                Err(_) => {
                    shutdown_process(&mut process).await;
                    continue;
                }
            }
        } else {
            commands.recv().await
        };
        let Some(command) = command else {
            shutdown_process(&mut process).await;
            return;
        };
        match command {
            ActorCommand::Convert {
                request,
                progress,
                reply,
            } => {
                let result = async {
                    ensure_started(&app, &mut process).await?;
                    run_conversion(&mut process, &mut commands, request, progress).await
                }
                .await;
                if result.is_err()
                    && !matches!(
                        result.as_ref().err().map(|e| e.code()),
                        Some("CONVERSION_CANCELLED")
                    )
                {
                    shutdown_process(&mut process).await;
                }
                let _ = reply.send(result);
            }
            ActorCommand::Cancel { reply, .. } => {
                shutdown_process(&mut process).await;
                let _ = reply.send(Ok(()));
            }
            ActorCommand::Shutdown { reply } => {
                shutdown_process(&mut process).await;
                let _ = reply.send(Ok(()));
            }
        }
    }
}

async fn ensure_started(
    app: &AppHandle,
    process: &mut Option<RunningProcess>,
) -> ConverterResult<()> {
    if process.is_some() {
        return Ok(());
    }
    let (receiver, child) = app
        .shell()
        .sidecar("ocr-sidecar")?
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .env("NO_PROXY", "*")
        .spawn()?;
    *process = Some(RunningProcess {
        receiver,
        child,
        process_group_id: None,
    });
    let request_id = Uuid::new_v4().to_string();
    let health = SidecarRequest::simple("health", &request_id);
    process
        .as_mut()
        .expect("process was just started")
        .child
        .write(&encode_request(&health)?)?;

    let event = tokio::time::timeout(Duration::from_secs(STARTUP_TIMEOUT_SECONDS), async {
        loop {
            let event = next_protocol_message(process).await?;
            match event {
                SidecarMessage::Health {
                    request_id: response_id,
                    health,
                } if response_id == request_id => {
                    return Ok(health);
                }
                SidecarMessage::Error { code, message, .. } => {
                    return Err(ConverterError::new(leak_error_code(code), message));
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| {
        ConverterError::new(
            "SIDECAR_STARTUP_TIMEOUT",
            "Document AI took too long to start",
        )
    })??;

    if event.sidecar_version != OCR_SIDECAR_VERSION
        || event.engine != OCR_ENGINE_ID
        || event.architecture != "arm64"
        || event.process_id == 0
        || event.process_group_id <= 1
    {
        return Err(ConverterError::new(
            "PROTOCOL_MISMATCH",
            "Document AI components are incompatible; run Repair",
        ));
    }
    process
        .as_mut()
        .expect("process is available after health check")
        .process_group_id = Some(event.process_group_id);
    log::info!(
        "document converter sidecar ready: version={}, protocol={}, engine_version={}, model={}",
        event.sidecar_version,
        SIDECAR_PROTOCOL_VERSION,
        event.engine_version,
        event.model_required
    );
    Ok(())
}

async fn run_conversion(
    process: &mut Option<RunningProcess>,
    commands: &mut mpsc::UnboundedReceiver<ActorCommand>,
    request: EngineRequest,
    progress: ProgressReporter,
) -> ConverterResult<EngineConversion> {
    let request_id = Uuid::new_v4().to_string();
    let input = request.input_path.to_string_lossy();
    let working = request.working_directory.to_string_lossy();
    let model = request.model_path.to_string_lossy();
    let sidecar_request = SidecarRequest {
        protocol_version: SIDECAR_PROTOCOL_VERSION,
        command: "convert",
        request_id: &request_id,
        job_id: Some(&request.job_id),
        input_path: Some(&input),
        working_directory: Some(&working),
        model_path: Some(&model),
        processing_mode: Some(&request.options.processing_mode),
        max_tokens: Some(request.options.max_tokens_per_page.clamp(256, 8192)),
    };
    process
        .as_mut()
        .ok_or_else(|| ConverterError::new("SIDECAR_CRASHED", "Document AI is not running"))?
        .child
        .write(&encode_request(&sidecar_request)?)?;

    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(ActorCommand::Cancel { job_id, reply }) if job_id == request.job_id => {
                        shutdown_process(process).await;
                        let _ = reply.send(Ok(()));
                        return Err(ConverterError::new("CONVERSION_CANCELLED", "Conversion was cancelled"));
                    }
                    Some(ActorCommand::Cancel { reply, .. }) => {
                        let _ = reply.send(Ok(()));
                    }
                    Some(ActorCommand::Shutdown { reply }) => {
                        shutdown_process(process).await;
                        let _ = reply.send(Ok(()));
                        return Err(ConverterError::new("CONVERSION_CANCELLED", "Conversion was cancelled"));
                    }
                    Some(ActorCommand::Convert { reply, .. }) => {
                        let _ = reply.send(Err(ConverterError::new("ENGINE_BUSY", "Document AI is processing another document")));
                    }
                    None => {
                        shutdown_process(process).await;
                        return Err(ConverterError::new("CONVERSION_CANCELLED", "Conversion was cancelled"));
                    }
                }
            }
            event = next_protocol_message(process) => {
                match event? {
                    SidecarMessage::Progress { request_id: response_id, job_id, stage, page, total_pages, label }
                        if response_id == request_id && job_id.as_deref() == Some(&request.job_id) => {
                        let percent = match (page, total_pages) {
                            (Some(page), Some(total)) if total > 0 => Some((page as f64 / total as f64) * 100.0),
                            _ => None,
                        };
                        progress(ConversionProgress {
                            job_id: request.job_id.clone(),
                            source_name: request.source_name.clone(),
                            status: "processing".to_string(),
                            stage,
                            label,
                            page,
                            total_pages,
                            percent,
                        });
                    }
                    SidecarMessage::Completed { request_id: response_id, job_id, result }
                        if response_id == request_id && job_id.as_deref() == Some(&request.job_id) => {
                        return Ok(result);
                    }
                    SidecarMessage::Error { request_id: response_id, code, message }
                        if response_id.as_deref().is_none() || response_id.as_deref() == Some(&request_id) => {
                        return Err(ConverterError::new(leak_error_code(code), message));
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn next_protocol_message(
    process: &mut Option<RunningProcess>,
) -> ConverterResult<SidecarMessage> {
    let running = process
        .as_mut()
        .ok_or_else(|| ConverterError::new("SIDECAR_CRASHED", "Document AI is not running"))?;
    loop {
        match running.receiver.recv().await {
            Some(CommandEvent::Stdout(line)) => return parse_message(&line),
            Some(CommandEvent::Stderr(line)) => {
                let diagnostic = String::from_utf8_lossy(&line);
                log::warn!(
                    "document converter sidecar: {}",
                    diagnostic.chars().take(800).collect::<String>()
                );
            }
            Some(CommandEvent::Error(error)) => {
                return Err(ConverterError::new("SIDECAR_CRASHED", error));
            }
            Some(CommandEvent::Terminated(payload)) => {
                return Err(ConverterError::new(
                    "SIDECAR_CRASHED",
                    format!("Document AI stopped unexpectedly (code {:?})", payload.code),
                ));
            }
            Some(_) => {}
            None => {
                return Err(ConverterError::new(
                    "SIDECAR_CRASHED",
                    "Document AI stopped unexpectedly",
                ));
            }
        }
    }
}

async fn shutdown_process(process: &mut Option<RunningProcess>) {
    let Some(mut running) = process.take() else {
        return;
    };
    let request_id = Uuid::new_v4().to_string();
    if let Ok(payload) = encode_request(&SidecarRequest::simple("shutdown", &request_id)) {
        let _ = running.child.write(&payload);
    }
    let terminated = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(event) = running.receiver.recv().await {
            if matches!(event, CommandEvent::Terminated(_)) {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    if !terminated {
        if let Some(process_group_id) = running.process_group_id {
            signal_process_group(process_group_id, libc::SIGTERM);
            let terminated_after_signal = tokio::time::timeout(Duration::from_secs(2), async {
                while let Some(event) = running.receiver.recv().await {
                    if matches!(event, CommandEvent::Terminated(_)) {
                        return true;
                    }
                }
                false
            })
            .await
            .unwrap_or(false);
            if terminated_after_signal {
                return;
            }
            signal_process_group(process_group_id, libc::SIGKILL);
        }
        let _ = running.child.kill();
    }
}

fn signal_process_group(process_group_id: i32, signal: i32) {
    if process_group_id <= 1 {
        return;
    }
    // The sidecar reports a private process group created with setsid(). A
    // negative pid targets the complete group, including MLX/Python workers.
    let result = unsafe { libc::kill(-process_group_id, signal) };
    if result != 0 {
        log::warn!(
            "could not signal Document AI process group {process_group_id}: {}",
            std::io::Error::last_os_error()
        );
    }
}

fn leak_error_code(code: String) -> &'static str {
    match code.as_str() {
        "MODEL_NOT_INSTALLED" => "MODEL_NOT_INSTALLED",
        "MODEL_CHECKSUM_FAILED" => "MODEL_CHECKSUM_FAILED",
        "UNSUPPORTED_FILE" => "UNSUPPORTED_FILE",
        "INVALID_PDF" => "INVALID_PDF",
        "ENCRYPTED_PDF" => "ENCRYPTED_PDF",
        "RESOURCE_LIMIT_EXCEEDED" => "RESOURCE_LIMIT_EXCEEDED",
        "PROTOCOL_MISMATCH" => "PROTOCOL_MISMATCH",
        "INVALID_REQUEST" => "SIDECAR_PROTOCOL_ERROR",
        _ => "OCR_ENGINE_FAILED",
    }
}
