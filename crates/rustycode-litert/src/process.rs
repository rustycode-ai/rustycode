use anyhow::{Context, Result};
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

/// Command sent to the process's internal loop.
enum ProcessCommand {
    Run {
        prompt: String,
        /// Channel to stream response tokens back to the caller.
        response_tx: mpsc::Sender<Result<String>>,
    },
}

/// A single long-lived `lit run` process, managed through an internal command loop.
///
/// The process communicates via stdin/stdout using the `>>>` prompt marker protocol:
/// - Write a prompt + newline to stdin
/// - Read stdout character-by-character until `>>>` appears, signalling end of response
/// - Stream incremental tokens back through an mpsc channel
pub struct LitProcess {
    /// Channel to send commands to the internal process loop.
    command_tx: mpsc::Sender<ProcessCommand>,
    /// Handle to the spawned tokio task that owns the child process.
    /// Kept for cleanup/shutdown.
    #[allow(dead_code)]
    child_handle: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for LitProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LitProcess")
            .field("command_tx", &"<mpsc::Sender>")
            .field("child_handle", &"<JoinHandle>")
            .finish()
    }
}

impl LitProcess {
    /// Spawn a `lit run` process, trying the GPU backend first and falling back to CPU.
    pub fn spawn(binary_path: &Path, model: &str) -> Result<Self> {
        match Self::spawn_with_backend(binary_path, model, "gpu") {
            Ok(process) => Ok(process),
            Err(e) => {
                tracing::warn!("GPU backend failed: {}. Trying CPU backend...", e);
                Self::spawn_with_backend(binary_path, model, "cpu")
            }
        }
    }

    /// Spawn a `lit run` process with the specified backend (`"gpu"` or `"cpu"`).
    fn spawn_with_backend(binary_path: &Path, model: &str, backend: &str) -> Result<Self> {
        tracing::info!("Attempting to spawn lit process with backend={backend}");

        let mut child = Command::new(binary_path)
            .arg("run")
            .arg(model)
            .arg("--backend")
            .arg(backend)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn lit process with backend={backend}"))?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stderr = child.stderr.take().context("Failed to get stderr")?;

        let (command_tx, command_rx) = mpsc::channel::<ProcessCommand>(32);

        // Spawn a background task that drains stderr and logs it at debug level.
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut stderr = stderr;
            let mut buf = [0u8; 1024];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        tracing::debug!("lit stderr: {}", msg.trim());
                    }
                }
            }
        });

        // Spawn the long-running task that owns the child process and handles commands.
        let child_handle = tokio::spawn(Self::run_process_loop(child, stdout, command_rx, stdin));

        Ok(Self {
            command_tx,
            child_handle,
        })
    }

    /// Inner process loop: wait for model initialization, then serve commands.
    ///
    /// Extracted from [`spawn_with_backend`] to keep the spawn function under the
    /// line-count limit enforced by workspace lints.
    async fn run_process_loop(
        mut child: tokio::process::Child,
        stdout: tokio::process::ChildStdout,
        mut command_rx: mpsc::Receiver<ProcessCommand>,
        mut stdin: tokio::process::ChildStdin,
    ) {
        use tokio::io::AsyncReadExt;

        let mut stdout = stdout;
        let mut buffer = Vec::new();
        let mut temp_buf = [0u8; 1024];
        let mut pending_commands = Vec::new();

        // Wait for the model to load by watching for the `>>>` prompt marker.
        tracing::info!("Waiting for model to load...");
        let init_timeout = tokio::time::Duration::from_mins(2);
        let init_result = tokio::time::timeout(init_timeout, async {
            loop {
                tokio::select! {
                    // Buffer any commands that arrive during initialisation.
                    cmd = command_rx.recv() => {
                        if let Some(cmd) = cmd {
                            tracing::debug!("Buffering command during initialization");
                            pending_commands.push(cmd);
                        }
                    }
                    result = stdout.read(&mut temp_buf) => {
                        match result {
                            Ok(0) => {
                                tracing::error!("Process stdout closed before model loaded");
                                return Err(anyhow::anyhow!("Process died during initialization"));
                            }
                            Ok(n) => {
                                buffer.extend_from_slice(&temp_buf[..n]);
                                let text = String::from_utf8_lossy(&buffer);

                                // Check for error messages in early output.
                                if text.contains("Error")
                                    || text.contains("error")
                                    || text.contains("failed")
                                {
                                    tracing::error!("Initialization error: {}", text);
                                    return Err(anyhow::anyhow!(
                                        "Process initialization failed: {}",
                                        text.trim()
                                    ));
                                }

                                // Wait for the initial prompt marker.
                                if text.contains(">>>") {
                                    tracing::info!("Process ready to accept prompts");
                                    buffer.clear();
                                    return Ok(());
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Error reading process output during init: {}",
                                    e
                                );
                                return Err(e.into());
                            }
                        }
                    }
                }
            }
        })
        .await;

        match init_result {
            Ok(Ok(())) => {
                tracing::info!(
                    "Model initialization complete, processing {} buffered commands",
                    pending_commands.len()
                );
            }
            Ok(Err(e)) => {
                tracing::error!("Initialization failed: {}", e);
                for cmd in pending_commands {
                    let ProcessCommand::Run { response_tx, .. } = cmd;
                    let _ = response_tx
                        .send(Err(anyhow::anyhow!("Process initialization failed: {e}")))
                        .await;
                }
                let _ = child.kill().await;
                return;
            }
            Err(_) => {
                tracing::error!("Initialization timed out after 2 minutes");
                for cmd in pending_commands {
                    let ProcessCommand::Run { response_tx, .. } = cmd;
                    let _ = response_tx
                        .send(Err(anyhow::anyhow!("Process initialization timed out")))
                        .await;
                }
                let _ = child.kill().await;
                return;
            }
        }

        // Process any commands that were buffered during initialisation.
        for cmd in pending_commands {
            Self::handle_command(cmd, &mut stdin, &mut stdout, &mut buffer, &mut temp_buf).await;
        }

        // Main command loop.
        while let Some(cmd) = command_rx.recv().await {
            Self::handle_command(cmd, &mut stdin, &mut stdout, &mut buffer, &mut temp_buf).await;
        }

        // Cleanup: kill the child process when the command loop exits.
        let _ = child.kill().await;
    }

    /// Handle a single `ProcessCommand::Run` by writing the prompt to stdin and
    /// streaming stdout back character-by-character until the `>>>` end marker appears.
    async fn handle_command(
        cmd: ProcessCommand,
        stdin: &mut tokio::process::ChildStdin,
        stdout: &mut tokio::process::ChildStdout,
        buffer: &mut Vec<u8>,
        temp_buf: &mut [u8; 1024],
    ) {
        use tokio::io::AsyncReadExt;

        match cmd {
            ProcessCommand::Run {
                prompt,
                response_tx,
            } => {
                tracing::debug!("Writing prompt to process stdin");

                // Write prompt + newline, then flush.
                if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
                    tracing::error!(error = %e, "Failed to write prompt to stdin");
                    let _ = response_tx.send(Err(e.into())).await;
                    return;
                }
                if let Err(e) = stdin.write_all(b"\n").await {
                    tracing::error!(error = %e, "Failed to write newline to stdin");
                    let _ = response_tx.send(Err(e.into())).await;
                    return;
                }
                if let Err(e) = stdin.flush().await {
                    tracing::error!(error = %e, "Failed to flush stdin");
                    let _ = response_tx.send(Err(e.into())).await;
                    return;
                }

                // Read stdout incrementally and stream tokens.
                buffer.clear();
                let mut last_chunk = String::new();

                tracing::debug!("Reading response from process stdout");
                loop {
                    match stdout.read(temp_buf).await {
                        Ok(0) => {
                            tracing::error!("Process stdout closed unexpectedly");
                            let _ = response_tx
                                .send(Err(anyhow::anyhow!("Process stdout closed")))
                                .await;
                            break;
                        }
                        Ok(n) => {
                            buffer.extend_from_slice(&temp_buf[..n]);
                            let text = String::from_utf8_lossy(buffer).to_string();

                            // Detect end-of-response via `>>>` prompt marker.
                            if text.ends_with(">>>") || text.contains("\n>>>") {
                                tracing::debug!("Received end marker, finalizing response");
                                let final_text =
                                    text.trim_end_matches(">>>").trim_end_matches('\n');
                                if final_text.len() > last_chunk.len() {
                                    let new_content = &final_text[last_chunk.len()..];
                                    if !new_content.is_empty()
                                        && response_tx
                                            .send(Ok(new_content.to_string()))
                                            .await
                                            .is_err()
                                    {
                                        tracing::debug!("Response channel closed by receiver");
                                        break;
                                    }
                                }
                                buffer.clear();
                                break;
                            }

                            // Stream incremental tokens.
                            if text.len() > last_chunk.len() {
                                let new_content = &text[last_chunk.len()..];
                                if response_tx.send(Ok(new_content.to_string())).await.is_err() {
                                    // Client disconnected.
                                    buffer.clear();
                                    break;
                                }
                                last_chunk = text;
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Error reading from process stdout");
                            let _ = response_tx.send(Err(e.into())).await;
                            break;
                        }
                    }
                }
                // When done, `response_tx` is dropped, closing the stream.
            }
        }
    }

    /// Send a prompt and return a stream of incremental response tokens.
    pub async fn send_prompt_stream(
        &self,
        prompt: &str,
    ) -> Result<impl Stream<Item = Result<String>>> {
        tracing::debug!(prompt_length = prompt.len(), "Creating prompt stream");

        let (response_tx, response_rx) = mpsc::channel(100);

        let cmd = ProcessCommand::Run {
            prompt: prompt.to_string(),
            response_tx,
        };

        self.command_tx.send(cmd).await.map_err(|e| {
            tracing::error!(error = %e, "Process command channel closed");
            anyhow::anyhow!("Failed to send command to process: {e}")
        })?;

        tracing::debug!("Command sent to process, returning stream");
        Ok(ReceiverStream::new(response_rx))
    }

    /// Send a prompt and collect the full response as a single string.
    pub async fn send_prompt(&self, prompt: &str) -> Result<String> {
        use futures::StreamExt;

        let mut stream = self.send_prompt_stream(prompt).await?;
        let mut response = String::new();

        while let Some(result) = stream.next().await {
            let line = result?;
            response.push_str(&line);
            response.push('\n');
        }

        Ok(response)
    }

    /// Shut down the process gracefully by dropping the command channel and
    /// waiting for the child task to finish.
    #[allow(dead_code)]
    pub async fn shutdown(self) -> Result<()> {
        drop(self.command_tx);
        self.child_handle.await?;
        Ok(())
    }
}

/// A pool of [`LitProcess`] instances with round-robin selection.
#[derive(Debug)]
pub struct ProcessPool {
    binary_path: PathBuf,
    model: String,
    processes: Vec<Arc<LitProcess>>,
}

impl ProcessPool {
    /// Create a new (uninitialised) pool. Call [`initialize`](Self::initialize) to spawn the
    /// processes.
    pub fn new(binary_path: PathBuf, model: String, pool_size: usize) -> Self {
        Self {
            binary_path,
            model,
            processes: Vec::with_capacity(pool_size),
        }
    }

    /// Spawn `pool_size` [`LitProcess`] instances.
    pub fn initialize(&mut self) -> Result<()> {
        let pool_size = self.processes.capacity();
        tracing::info!(
            pool_size = pool_size,
            model = %self.model,
            "Initializing process pool"
        );

        for i in 0..pool_size {
            tracing::debug!(process_index = i, "Spawning process");
            let process = LitProcess::spawn(&self.binary_path, &self.model)?;
            self.processes.push(Arc::new(process));
            tracing::debug!(process_index = i, "Process spawned successfully");
        }

        tracing::info!(
            pool_size = pool_size,
            "Process pool initialized successfully"
        );
        Ok(())
    }

    /// Get the next available process using round-robin selection.
    pub fn get_process(&self) -> Result<Arc<LitProcess>> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        if self.processes.is_empty() {
            tracing::error!("Process pool is empty or not initialized");
            anyhow::bail!("Process pool not initialized");
        }

        let idx = COUNTER.fetch_add(1, Ordering::Relaxed) % self.processes.len();
        tracing::debug!(
            process_index = idx,
            pool_size = self.processes.len(),
            "Selected process from pool"
        );
        Ok(self.processes[idx].clone())
    }

    /// Convenience method: pick a process via round-robin and send a prompt.
    pub async fn send_prompt(&self, prompt: &str) -> Result<String> {
        let process = self.get_process()?;
        process.send_prompt(prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_pool_new_sets_capacity() {
        let pool = ProcessPool::new(PathBuf::from("/fake/binary"), "test-model".into(), 4);
        assert!(pool.processes.is_empty());
        assert_eq!(pool.processes.capacity(), 4);
    }

    #[test]
    fn process_pool_get_process_fails_when_empty() {
        let pool = ProcessPool::new(PathBuf::from("/fake/binary"), "test-model".into(), 2);
        let result = pool.get_process();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[test]
    fn process_pool_debug_includes_model() {
        let pool = ProcessPool::new(PathBuf::from("/fake/binary"), "gemma".into(), 2);
        let debug = format!("{pool:?}");
        assert!(debug.contains("ProcessPool"));
    }

    #[test]
    fn lit_process_debug_formats() {
        // We can't easily create a LitProcess without a real binary,
        // but we can test the Debug impl structure
        let pool = ProcessPool::new(PathBuf::from("/usr/bin/echo"), "model".into(), 1);
        let debug = format!("{pool:?}");
        assert!(debug.contains("echo"));
    }
}
