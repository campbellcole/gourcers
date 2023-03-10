use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use lazy_regex::{lazy_regex, Lazy, Regex};
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::{gh::Repo, Settings};

pub static REPLACE_REGEX: Lazy<Regex> = lazy_regex!(r"(.*\|.{1}\|)(.*)");
pub static DEQUOTE_REGEX: Lazy<Regex> = lazy_regex!(r#"['"`]"#);

pub async fn generate_gource_log(settings: &Settings, repo: &Repo) -> Result<()> {
    debug!("generating gource log for repo {}", repo.name);
    let repo_folder = settings.repo_folder(repo);

    let mut cmd = Command::new("gource");

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    cmd.arg("--output-custom-log").arg("-").arg(&repo_folder);

    let output = cmd
        .output()
        .await
        .context("failed to execute gource command")?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to generate gource log for repo {}: {}",
            repo.name,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let gource_log = String::from_utf8_lossy(&output.stdout);

    let substitution = format!("$1/{}$2", &repo.name);
    // add repo name to each line
    let gource_log = REPLACE_REGEX.replace_all(&gource_log, substitution.as_str());
    // remove diacritics
    let gource_log = diacritics::remove_diacritics(&gource_log);
    // remove quotes
    let gource_log = DEQUOTE_REGEX.replace_all(&gource_log, "");

    let gource_log_path = settings.gource_log_file(repo);
    let mut gource_log_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&gource_log_path)
        .await
        .context("failed to open gource log file")?;
    gource_log_file
        .write_all(gource_log.as_bytes())
        .await
        .context("failed to write gource log file")?;

    Ok(())
}

pub async fn combine_gource_logs(settings: &Settings, repos: &[Repo]) -> Result<()> {
    debug!("combining gource logs");
    let mut gource_log = String::new();

    for repo in repos {
        let repo_gource_log = tokio::fs::read_to_string(settings.gource_log_file(repo))
            .await
            .context("failed to read gource log file")?;
        gource_log.push_str(&repo_gource_log);
    }

    let gource_log_path = settings.combined_log_file();
    if gource_log_path.exists() {
        tokio::fs::remove_file(&gource_log_path)
            .await
            .context("failed to remove old combined gource log file")?;
    }
    let mut gource_log_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&gource_log_path)
        .await
        .context("failed to open gource log file")?;
    gource_log_file
        .write_all(gource_log.as_bytes())
        .await
        .context("failed to write gource log file")?;

    Ok(())
}

pub async fn execute_gource(settings: &Settings) -> Result<()> {
    let gource_args = settings.gource_options.trim().split(' ');

    if settings.video {
        debug!("spinning up ffmpeg");
        let mut cmd = Command::new("ffmpeg");

        // overwrite existing file
        cmd.arg("-y");

        // set input framerate
        cmd.arg("-r").arg("60");

        // set input format + codec
        cmd.arg("-f").arg("image2pipe").arg("-c:v").arg("ppm");

        // set input
        cmd.arg("-i").arg("-");

        // set output codec
        cmd.arg("-c:v").arg("libx264");

        // set output encoding preset
        cmd.arg("-preset").arg("ultrafast");

        // disable reduction factor
        cmd.arg("-crf").arg("1");

        // disable B-frames
        cmd.arg("-bf").arg("0");

        // set output filename
        // SAFETY: we check that the filename is set before calling this function
        cmd.arg(settings.video_filename.as_ref().unwrap());

        cmd.stdin(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().context("failed to execute ffmpeg command")?;

        debug!("executing gource");
        let mut cmd = Command::new("gource");

        cmd.args(gource_args);

        cmd.arg(format!("-{}", settings.video_resolution.as_ref().unwrap()));

        cmd.arg("-o").arg("-");

        cmd.arg(settings.sorted_log_file());

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut gource = cmd.spawn().context("failed to execute gource command")?;

        let mut ffmpeg_stdin = child.stdin.take().context("failed to get ffmpeg stdin")?;

        let mut ffmpeg_stderr = child.stderr.take().context("failed to get ffmpeg stderr")?;

        let mut gource_stdout = gource
            .stdout
            .take()
            .context("failed to get gource stdout")?;

        let mut gource_stderr = gource
            .stderr
            .take()
            .context("failed to get gource stderr")?;

        tokio::io::copy(&mut gource_stdout, &mut ffmpeg_stdin)
            .await
            .context("failed to copy gource output to ffmpeg input")?;

        debug!("waiting for gource to finish");
        let gource_status = gource
            .wait()
            .await
            .context("failed to wait for gource to finish")?;

        // send EOF to ffmpeg
        drop(ffmpeg_stdin);

        if !gource_status.success() {
            let mut stderr = String::new();
            gource_stderr
                .read_to_string(&mut stderr)
                .await
                .context("failed to read gource stderr")?;
            return Err(anyhow!("Failed to generate video: {}", stderr));
        }

        debug!("waiting for ffmpeg to finish");
        let ffmpeg_status = child
            .wait()
            .await
            .context("failed to wait for ffmpeg to finish")?;

        if !ffmpeg_status.success() {
            let mut stderr = String::new();
            ffmpeg_stderr
                .read_to_string(&mut stderr)
                .await
                .context("failed to read ffmpeg stderr")?;
            return Err(anyhow!("Failed to generate video: {}", stderr));
        }
    } else {
        debug!("executing gource");
        let mut cmd = Command::new("gource");

        cmd.args(gource_args);

        cmd.arg(settings.sorted_log_file());

        let _ = cmd
            .status()
            .await
            .context("failed to execute gource command")?;
    }

    Ok(())
}
