use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use super::errors::ConverterResult;
use super::types::{ConversionOptions, ConversionProgress, EngineConversion};

pub type ProgressReporter = Arc<dyn Fn(ConversionProgress) + Send + Sync>;
pub type EngineFuture<'a, T> = Pin<Box<dyn Future<Output = ConverterResult<T>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub struct EngineRequest {
    pub job_id: String,
    pub source_name: String,
    pub input_path: PathBuf,
    pub working_directory: PathBuf,
    pub model_path: PathBuf,
    pub options: ConversionOptions,
}

pub trait OcrEngine: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> String;
    fn is_available(&self) -> bool;
    fn convert<'a>(
        &'a self,
        request: EngineRequest,
        progress: ProgressReporter,
    ) -> EngineFuture<'a, EngineConversion>;
    fn cancel<'a>(&'a self, job_id: &'a str) -> EngineFuture<'a, ()>;
    fn shutdown<'a>(&'a self) -> EngineFuture<'a, ()>;
}
