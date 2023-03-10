use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum DependencyError {
    #[error("ffmpeg not found")]
    FfmpegNotFound,
    #[error("gource not found")]
    GourceNotFound,
    #[error("git not found")]
    GitNotFound,
    #[error("qsv not found")]
    QsvNotFound,
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("nonzero exit code: {0:?}")]
    NonzeroExitCode(Option<i32>),
}

pub async fn check_for_dependencies() -> Result<(), Vec<DependencyError>> {
    match vec![
        check_for_cmd("git", "--version", DependencyError::GitNotFound).await,
        check_for_cmd("gource", "-h", DependencyError::GourceNotFound).await,
        check_for_cmd("ffmpeg", "-version", DependencyError::FfmpegNotFound).await,
        check_for_cmd("qsv", "--version", DependencyError::QsvNotFound).await,
    ]
    .drain(..)
    .filter_map(Result::err)
    .collect::<Vec<_>>()
    {
        errs if !errs.is_empty() => Err(errs),
        _ => Ok(()),
    }
}

async fn check_for_cmd(bin: &str, arg: &str, err: DependencyError) -> Result<(), DependencyError> {
    let mut cmd = Command::new(bin);
    cmd.arg(arg);
    let output = match cmd.output().await {
        Ok(o) => o,
        Err(io_err) => match io_err.kind() {
            std::io::ErrorKind::NotFound => return Err(err),
            _ => return Err(io_err.into()),
        },
    };
    if !output.status.success() {
        return Err(DependencyError::NonzeroExitCode(output.status.code()));
    }
    Ok(())
}
