use std::{fs, io::Write, path::PathBuf, process::exit, sync::Arc};

use anyhow::Context;
use clap::Parser;
use env_logger::fmt::Color;
use futures::future::join_all;
use gh::Repo;
use ignore::IgnoreFile;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::ignore::FilterResult;

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod check;
mod gh;
mod gource;
mod ignore;
mod qsv;

#[macro_use]
mod log_utils {
    /// Logs an error message regardless of the log level.
    macro_rules! critical {
        ($($arg:tt)*) => {
            if ::log::log_enabled!(::log::Level::Error) {
                error!($( $arg )*);
            } else {
                eprintln!($( $arg )*);
            }
        };
    }

    /// Logs an error message and exits the program.
    macro_rules! failure {
        ($($arg:tt)*) => {
            critical!( $( $arg )* );
            ::std::process::exit(1);
        };
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Parser)]
#[clap(version, author, about, long_about = None)]
pub struct Settings {
    #[clap(short = 't', long, help = "GitHub token", env = "GH_TOKEN")]
    pub token: String,
    #[clap(short = 'o', long, help = "Data folder")]
    pub data_folder: PathBuf,
    #[clap(short = 'd', long, help = "Create dump file", action = clap::ArgAction::SetTrue)]
    pub dump: Option<bool>,
    #[clap(
        short,
        long,
        help = "Use dump file instead of reading GitHub API",
        env = "USE_DUMP",
        action = clap::ArgAction::SetTrue
    )]
    pub use_dump: Option<bool>,
    #[clap(long, help = "Dump requests", action = clap::ArgAction::SetTrue)]
    pub dump_requests: Option<bool>,
    #[clap(short = 'i', long, help = "Ignore repos")]
    pub ignore: Option<Vec<String>>,
    #[clap(short = 'f', long, help = "Ignore file")]
    pub ignore_file: Option<PathBuf>,
    #[clap(short = 's', long, help = "Stop after n repos")]
    pub stop_after: Option<usize>,
    #[clap(long, help = "Only find and filter repos, no cloning/gourcing", action = clap::ArgAction::SetTrue)]
    pub dry_run: bool,
    #[clap(short = 'g', long, help = "Enable video generation", action = clap::ArgAction::SetTrue)]
    pub video: bool,
    #[clap(
        short = 'v',
        long,
        help = "Video filename",
        default_value = "gource.mp4"
    )]
    pub video_filename: Option<String>,
    #[clap(
        short = 'r',
        long,
        help = "Video resolution",
        default_value = "1920x1080"
    )]
    pub video_resolution: Option<String>,
    #[clap(
        short = 'p',
        long,
        help = "Gource options",
        default_value = "--hide root -a 1 -s 1 -c 4 --key --multi-sampling"
    )]
    pub gource_options: String,
}

impl Settings {
    pub fn repos_folder(&self) -> PathBuf {
        self.data_folder.join("repos")
    }

    pub fn repo_folder(&self, repo: &Repo) -> PathBuf {
        self.repos_folder().join(&repo.name)
    }

    pub fn gource_logs_folder(&self) -> PathBuf {
        self.data_folder.join("gource")
    }

    pub fn gource_log_file(&self, repo: &Repo) -> PathBuf {
        self.gource_logs_folder()
            .join(&repo.name)
            .with_extension("txt")
    }

    pub fn combined_log_file(&self) -> PathBuf {
        self.data_folder.join("combined.txt")
    }

    pub fn sorted_log_file(&self) -> PathBuf {
        self.data_folder.join("sorted.txt")
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    env_logger::builder()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} [{:<5}] - {}",
                buf.style()
                    .set_color(Color::Cyan)
                    .set_bold(true)
                    .value(record.file().unwrap_or("unknown")),
                buf.style()
                    .set_color(Color::Yellow)
                    .set_bold(true)
                    .value(record.line().unwrap_or(0)),
                buf.default_styled_level(record.level()),
                record.args(),
            )
        })
        .init();

    if let Err(err) = check::check_for_dependencies().await {
        critical!("Dependency check failed with the following errors:");
        err.iter()
            .enumerate()
            .for_each(|(x, e)| critical!("{}─ {}", if x == err.len() - 1 { "└" } else { "├" }, e));
        exit(1);
    }

    let settings = Settings::parse();
    let mut ignores = None;

    if let Some(ignore_file) = &settings.ignore_file {
        let ignores_str = match fs::read_to_string(ignore_file) {
            Ok(ignores_str) => ignores_str,
            Err(err) => {
                failure!("Failed to read ignore file: {err}");
            }
        };
        match ignores_str.parse::<IgnoreFile>() {
            Ok(parsed) => ignores = Some(parsed),
            Err(err) => {
                failure!("Failed to parse ignore file: {err}");
            }
        };
    }

    if ignores.is_none() {
        if let Some(ignore) = &settings.ignore {
            let ignores_str = ignore.join(" ");
            match ignores_str.parse::<IgnoreFile>() {
                Ok(parsed) => ignores = Some(parsed),
                Err(err) => {
                    failure!("Failed to parse ignores argument: {err}");
                }
            };
        }
    }

    if !settings.data_folder.exists() {
        if let Err(err) = tokio::fs::create_dir_all(&settings.data_folder).await {
            failure!("Failed to create data folder: {err}");
        }
    }

    if !settings.gource_logs_folder().exists() {
        if let Err(err) = tokio::fs::create_dir_all(&settings.gource_logs_folder()).await {
            failure!("Failed to create gource logs folder: {err}");
        }
    }

    if settings.video {
        if settings.video_filename.is_none() {
            failure!("Video filename is required for video generation");
        }
        if settings.video_resolution.is_none() {
            failure!("Video resolution is required for video generation");
        }
    }

    trace!(
        "gource options: {:?}",
        settings.gource_options.split(' ').collect::<Vec<_>>()
    );

    let mut repos = match settings.use_dump {
        Some(true) => {
            let dump_file = std::fs::File::open("repos.json").unwrap();
            serde_json::from_reader(dump_file).unwrap()
        }
        _ => gh::get_user_repos(&settings).await.unwrap(),
    };

    debug!("loaded {} repos", repos.len());

    if let Some(true) = settings.dump {
        let dump_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("repos.json")
            .unwrap();

        serde_json::to_writer_pretty(dump_file, &repos).unwrap();

        info!("dumped repos");
    }

    if let Some(ignores) = &ignores {
        repos.retain(|r| {
            let res = ignores.is_ignored(r);
            match res {
                FilterResult::Ignore(exclusion, filter) => {
                    debug!(
                        "ignoring repo {} because {} but {}",
                        r.full_name.as_ref().unwrap_or(&r.name),
                        exclusion.describe(),
                        filter.describe()
                    );
                }
                FilterResult::Keep(exclusion) => {
                    trace!(
                        "keeping repo {} because {}",
                        r.full_name.as_ref().unwrap_or(&r.name),
                        exclusion.describe()
                    );
                }
                FilterResult::Default => {
                    trace!(
                        "ignoring repo {}: no rules matched",
                        r.full_name.as_ref().unwrap_or(&r.name)
                    );
                }
            }
            res.keep()
        });
    }

    debug!("filtered to {} repos", repos.len());

    if let Some(stop_after) = settings.stop_after {
        repos.truncate(stop_after);
    }

    debug!("truncated to {} repos", repos.len());

    if settings.dry_run {
        debug!("{:?}", repos);
        info!("dry run, exiting");
        exit(0);
    }

    if settings.gource_logs_folder().exists() {
        if let Err(err) = tokio::fs::remove_dir_all(&settings.gource_logs_folder()).await {
            failure!("Failed to remove old gource logs folder: {err}");
        }
    }

    if let Err(err) = tokio::fs::create_dir_all(&settings.gource_logs_folder()).await {
        failure!("Failed to create gource logs folder: {err}");
    }

    let m = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .context("Failed to create progress bar style")
    .unwrap()
    .progress_chars("##-");

    let clone_progress = m.add(ProgressBar::new(repos.len() as u64));
    clone_progress.set_style(style.clone());
    let gource_progress = m.insert_after(&clone_progress, ProgressBar::new(repos.len() as u64));
    gource_progress.set_style(style.clone());

    clone_progress.set_message("Cloning");
    gource_progress.set_message("Gourcing");

    m.println("Starting...").unwrap();

    let shared_settings = Arc::new(settings.clone());
    let mut futures = Vec::new();

    for repo in repos.clone().into_iter() {
        let shared_settings = shared_settings.clone();
        let clone_progress = clone_progress.clone();
        let gource_progress = gource_progress.clone();
        futures.push(tokio::spawn(async move {
            if let Err(err) = gh::fetch_repo(&shared_settings, &repo).await {
                error!("Failed to fetch repo {}: {}", repo.name, err);
                return;
            }
            clone_progress.inc(1);
            if let Err(err) = gource::generate_gource_log(&shared_settings, &repo).await {
                error!(
                    "Failed to generate gource log for repo {}: {}",
                    repo.name, err
                );
            }
            gource_progress.inc(1);
        }));
    }

    join_all(futures).await;

    clone_progress.finish();
    gource_progress.finish();
    m.println("Done!").unwrap();
    m.clear().unwrap();

    if let Err(err) = gource::combine_gource_logs(
        &settings,
        &repos[..settings.stop_after.unwrap_or(repos.len())],
    )
    .await
    {
        failure!("Failed to combine gource logs: {err}");
    }

    if let Err(err) = qsv::sort_combined_logs(&settings).await {
        failure!("Failed to sort combined logs: {err}");
    }

    if let Err(err) = gource::execute_gource(&settings).await {
        failure!("Failed to execute gource: {err}");
    }
}
