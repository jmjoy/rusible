//! Controller-side runtime for executing Rusible tasks locally or over SSH.

use crate::meta::TaskSpec;
use std::{future::Future, path::PathBuf};
use tokio::fs;

/// Executes tasks on a controller target.
pub trait Runnable {
    /// Report type returned for a specific task details payload.
    type Output<D>;
    type InitError;
    /// Error type returned for a specific task details payload.
    type RunError<D>;

    /// Reads a `rusible-exec` binary from disk and prepares it for later task
    /// execution.
    fn init_with_path<P>(
        &mut self, exec_path: P,
    ) -> impl Future<Output = Result<(), Self::InitError>> + Send
    where
        Self: Send,
        Self::InitError: From<std::io::Error>,
        P: Into<PathBuf>,
    {
        let exec_path = exec_path.into();

        async move {
            let exec_bytes = fs::read(&exec_path).await?;
            self.init(&exec_bytes).await
        }
    }

    /// Prepares the provided `rusible-exec` binary bytes for later task
    /// execution.
    fn init(
        &mut self, exec_bytes: &[u8],
    ) -> impl Future<Output = Result<(), Self::InitError>> + Send
    where
        Self: Send;

    /// Serializes a task, executes it, and returns the structured result.
    fn run<T>(
        &mut self, task: T,
    ) -> impl Future<Output = Result<Self::Output<T::Details>, Self::RunError<T::Details>>> + Send
    where
        Self: Send,
        T: TaskSpec + Send;
}
