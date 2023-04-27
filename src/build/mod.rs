use anyhow::anyhow;

use std::{
    collections::HashMap,
    ffi::OsStr,
    process::{ExitStatus, Stdio},
    sync::Arc,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command as TokioCommand,
};
use tokio_stream::StreamExt;

use tracing::log::*;

pub mod pipeline;

pub use pipeline::Pipeline;
pub type Pipelines = HashMap<String, Vec<Arc<Pipeline>>>;

#[tracing::instrument(skip())]
async fn run_command<Cmd>(
    command: Cmd,
    envs: Option<&HashMap<String, String>>,
) -> anyhow::Result<()>
where
    Cmd: AsRef<OsStr> + std::fmt::Debug,
{
    let mut cmd = TokioCommand::new("sh");

    cmd.arg("-c");
    cmd.arg(command);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if let Some(envs) = envs {
        cmd.envs(envs);
    }

    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stdout_reader = BufReader::new(stdout).lines();
    let mut stdout_reader_stream = tokio_stream::wrappers::LinesStream::new(stdout_reader);

    let stderr = child.stderr.take().unwrap();
    let stderr_reader = BufReader::new(stderr).lines();
    let mut stderr_reader_stream = tokio_stream::wrappers::LinesStream::new(stderr_reader);

    let handle: tokio::task::JoinHandle<Result<ExitStatus, std::io::Error>> =
        tokio::spawn(async move { child.wait().await });

    loop {
        tokio::select! {
            Some(Ok(line)) = stdout_reader_stream.next() => {
                info!("stdout: {line:?}");
            }
            Some(Ok(line)) = stderr_reader_stream.next() => {
                info!("stderr: {line:?}");
            }
            else => {
                break;
            }
        }
    }

    let status = handle.await??;

    if let Some(code) = status.code() {
        if code == 0 {
            Ok(())
        } else {
            Err(anyhow!("Process exited with status {code}"))
        }
    } else {
        Err(anyhow!("Process exited without a status code?"))
    }
}
