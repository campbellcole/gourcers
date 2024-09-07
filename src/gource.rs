use std::{
    fs::File,
    io::Write,
    process::{Command, Stdio},
};

use color_eyre::eyre::{bail, Result, WrapErr};
use lazy_regex::{lazy_regex, Lazy, Regex};

use crate::{github::Repo, Context};

static REPLACE_REGEX: Lazy<Regex> = lazy_regex!(r"(.*\|.{1}\|)(.*)");
static DEQUOTE_REGEX: Lazy<Regex> = lazy_regex!(r#"['"`]"#);

#[instrument(skip(cx))]
pub fn generate_gource_log(cx: &Context, repo: &Repo) -> Result<()> {
    let repo_dir = cx.data_dir.repo_dir(repo);

    let mut cmd = Command::new("gource");

    cmd.arg("--output-custom-log").arg("-").arg(&repo_dir);

    trace!(command = ?cmd, repo = %repo.name, "running gource");

    let output = cmd.output().wrap_err("failed to generate gource log")?;

    if !output.status.success() {
        bail!("gource failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let gource_log = String::from_utf8(output.stdout).wrap_err("gource log was not valid utf-8")?;

    let substitution = format!("$1/{}$2", repo.name);
    let gource_log = REPLACE_REGEX.replace_all(&gource_log, &substitution);
    let gource_log = diacritics::remove_diacritics(&gource_log);
    let gource_log = DEQUOTE_REGEX.replace_all(&gource_log, "");

    let gource_log_path = cx.data_dir.gource_log(repo);
    let mut gource_log_file =
        File::create(gource_log_path).wrap_err("failed to create gource log file")?;

    gource_log_file
        .write_all(gource_log.as_bytes())
        .wrap_err("failed to write gource log")?;

    Ok(())
}

pub fn combine_and_sort_logs(cx: &Context, repos: &Vec<Repo>) -> Result<()> {
    let mut combined = String::new();

    trace!("reading gource logs into memory");
    for repo in repos {
        let gource_log_path = cx.data_dir.gource_log(repo);
        let gource_log = std::fs::read_to_string(gource_log_path)
            .wrap_err_with(|| format!("failed to read gource log for {}", repo.full_name()))?;

        combined.push_str(&gource_log);
    }

    trace!("sorting combined logs");
    let mut lines = combined.lines().collect::<Vec<_>>();

    lines.sort_by(|a, b| {
        let a = a.split('|').next().unwrap();
        let b = b.split('|').next().unwrap();
        a.cmp(b)
    });

    let sorted_path = cx.data_dir.sorted_log();
    trace!(sorted_path = ?sorted_path, "writing sorted log to disk");

    let mut sorted_file = File::create(sorted_path).wrap_err("failed to create sorted log file")?;

    for line in lines {
        writeln!(sorted_file, "{line}").wrap_err("failed to write sorted log")?;
    }

    Ok(())
}

pub fn generate_gource_video(cx: &Context) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");

    cmd.args([
        "-y",
        "-r",
        "60",
        "-f",
        "image2pipe",
        "-c:v",
        "ppm",
        "-i",
        "-",
    ])
    .args(&cx.ffmpeg_args)
    .arg(&cx.output);

    cmd.stdin(Stdio::piped()).stderr(Stdio::piped());

    trace!(command = ?cmd, "spawning ffmpeg");

    let mut ffmpeg = cmd.spawn().wrap_err("failed to spawn ffmpeg")?;

    let mut cmd = Command::new("gource");

    cmd.args(&cx.gource_args)
        .args(["-o", "-"])
        .arg(&cx.data_dir.sorted_log());

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    trace!(command = ?cmd, "spawning gource");

    let mut gource = cmd.spawn().wrap_err("failed to spawn gource")?;

    let mut ffmpeg_in = ffmpeg.stdin.take().unwrap();
    let mut gource_out = gource.stdout.take().unwrap();

    trace!("piping gource output to ffmpeg input");
    std::io::copy(&mut gource_out, &mut ffmpeg_in)
        .wrap_err("failed to copy gource output to ffmpeg input")?;

    trace!("waiting for gource to finish");
    let gource_output = gource.wait_with_output().wrap_err("gource failed")?;

    trace!("sending EOF to ffmpeg input");
    // EOF ffmpeg
    drop(ffmpeg_in);

    if !gource_output.status.success() {
        bail!(
            "gource failed: {}",
            String::from_utf8_lossy(&gource_output.stderr)
        );
    }

    trace!("waiting for ffmpeg to finish");
    let ffmpeg_output = ffmpeg.wait_with_output().wrap_err("ffmpeg failed")?;

    if !ffmpeg_output.status.success() {
        bail!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&ffmpeg_output.stderr)
        );
    }

    Ok(())
}
