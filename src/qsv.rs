use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::{gh::Repo, Settings};

/// This function is roughly equivalent to the following shell script:
///
/// ```sh
/// # combine all logs
/// cat $LOGS_DIR/*.log |
/// # sort the logs by their first column (replaces pipes with commas)
/// qsv sort -d \| -n -s 1 |
/// # format the logs (change the commas back to pipes, gource expects pipes)
/// qsv fmt -t \| -o $SORTED_LOG
/// ```
pub async fn combine_and_sort_logs(settings: &Settings, repos: &[Repo]) -> Result<()> {
    debug!("combining gource logs");
    let mut combined = String::new();

    for repo in repos {
        let repo_log = tokio::fs::read_to_string(settings.gource_log_file(repo))
            .await
            .context("failed to read gource log file")?;
        combined.push_str(&repo_log);
    }

    let sorted_log_path = settings.sorted_log_file();
    if sorted_log_path.exists() {
        tokio::fs::remove_file(&sorted_log_path)
            .await
            .context("failed to remove old sorted log file")?;
    }

    let mut sort_cmd = Command::new("qsv");

    sort_cmd
        .arg("sort")
        .arg("-d")
        .arg("|")
        .arg("-n")
        .arg("-s")
        .arg("1");

    sort_cmd
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .stdin(Stdio::piped());

    let mut sort_child = sort_cmd.spawn().context("failed to spawn sort command")?;

    let mut sort_stdin = sort_child.stdin.take().unwrap();
    let mut sort_stdout = sort_child.stdout.take().unwrap();

    let mut fmt_cmd = Command::new("qsv");

    fmt_cmd
        .arg("fmt")
        .arg("-t")
        .arg("|")
        .arg("-o")
        .arg(settings.sorted_log_file());

    fmt_cmd.stderr(Stdio::piped()).stdin(Stdio::piped());

    let mut fmt_child = fmt_cmd.spawn().context("failed to spawn fmt command")?;

    let mut fmt_stdin = fmt_child.stdin.take().unwrap();

    let stdin_task = tokio::task::spawn(async move {
        debug!("writing combined log to sort: {} bytes", combined.len());
        sort_stdin
            .write_all(combined.as_bytes())
            .await
            .context("failed to write combined log to sort")
    });

    tokio::io::copy(&mut sort_stdout, &mut fmt_stdin)
        .await
        .context("failed to copy sort output to fmt input")?;

    stdin_task.await??;

    debug!("waiting for sort to finish");
    let sort_status = sort_child
        .wait()
        .await
        .context("failed to wait for sort to finish")?;

    // send EOF to fmt
    drop(fmt_stdin);

    if !sort_status.success() {
        let mut stderr = String::new();
        sort_child
            .stderr
            .unwrap()
            .read_to_string(&mut stderr)
            .await
            .context("failed to read sort stderr")?;
        return Err(anyhow!("Failed to sort combined log file: {}", stderr));
    }

    debug!("waiting for fmt to finish");
    let fmt_status = fmt_child
        .wait()
        .await
        .context("failed to wait for fmt to finish")?;

    if !fmt_status.success() {
        let mut stderr = String::new();
        fmt_child
            .stderr
            .unwrap()
            .read_to_string(&mut stderr)
            .await
            .context("failed to read fmt stderr")?;
        return Err(anyhow!("Failed to format combined log file: {}", stderr));
    }

    Ok(())
}
