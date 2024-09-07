#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;
use color_eyre::{
    eyre::{Result, WrapErr},
    Section,
};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm};
use github::Repo;
use include::RuleSet;
use indicatif::{ProgressBar, ProgressStyle};
use temp_dir::TempDir;
use tracing_subscriber::prelude::*;

#[macro_use]
extern crate tracing;

pub mod github;
pub mod gource;
pub mod include;

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    /// Your personal access token for GitHub.
    ///
    /// This token must have the `repo` scope.
    #[clap(short, long, env = "GITHUB_TOKEN")]
    pub token: String,
    /// The directory to store the cloned repos and gource logs.
    ///
    /// If left blank, a temporary directory will be created and removed after finishing.
    ///
    /// If you are going to be running this command multiple times, it is recommended to specify a directory
    /// to ensure work is not done multiple times needlessly.
    #[clap(short, long)]
    pub data_dir: Option<PathBuf>,
    /// Silently allow using a temporary data directory instead of prompting for confirmation.
    #[clap(short = 'y', long)]
    pub temp: bool,
    /// The path to output the resulting gource video.
    #[clap(short, long, default_value_os_t = PathBuf::from("./gource.mp4"))]
    pub output: PathBuf,
    /// Skip cloning/pulling repos and assume they are already present in the data directory.
    #[clap(long)]
    pub skip_clone: bool,
    /// Include any repos matching the given selectors.
    #[clap(short, long)]
    pub include: Vec<String>,
    /// Include any repos matching the given selectors from the given file.
    #[clap(short = 'f', long)]
    pub include_file: Option<PathBuf>,
    /// Extra arguments to pass to ffmpeg.
    ///
    /// The resulting command will look like `ffmpeg -r 60 -f image2pipe -c:v ppm -i - {ffmpeg_args} {output}`.
    #[clap(long, default_value = "-c:v libx264 -preset ultrafast -crf 1 -bf 0")]
    pub ffmpeg_args: String,
    /// Extra arguments to pass to gource.
    ///
    /// The resulting command will look like `gource {gource_args} -o - {data_dir}/sorted.txt`.
    ///
    /// Using `--hide root` is highly recommended.
    #[clap(
        long,
        default_value = "--hide root -a 1 -s 1 -c 4 --key --multi-sampling -1920x1080"
    )]
    pub gource_args: String,
}

#[derive(Debug)]
pub enum OutputDir {
    Temp(TempDir),
    Specified(PathBuf),
}

impl OutputDir {
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            OutputDir::Specified(path) => path,
            OutputDir::Temp(temp) => temp.path(),
        }
    }

    #[must_use]
    pub fn repos_dir(&self) -> PathBuf {
        self.path().join("repos")
    }

    #[must_use]
    pub fn repo_dir(&self, repo: &Repo) -> PathBuf {
        self.repos_dir().join(repo.full_name_path_friendly())
    }

    #[must_use]
    pub fn gource_dir(&self) -> PathBuf {
        self.path().join("gource")
    }

    #[must_use]
    pub fn gource_log(&self, repo: &Repo) -> PathBuf {
        self.gource_dir()
            .join(format!("{}.txt", repo.full_name_path_friendly()))
    }

    #[must_use]
    pub fn sorted_log(&self) -> PathBuf {
        self.path().join("sorted.txt")
    }
}

#[derive(Debug)]
pub struct Context {
    pub token: String,
    pub data_dir: OutputDir,
    pub output: PathBuf,
    pub skip_clone: bool,
    pub includes: Option<RuleSet>,
    pub ffmpeg_args: Vec<String>,
    pub gource_args: Vec<String>,
}

impl Context {
    pub fn from_cli(cli: Cli) -> Result<Self> {
        let data_dir = cli.data_dir.map_or_else(
            || -> Result<OutputDir> {
                if !cli.temp {
                    println!("{}: {}", style("WARNING").red().bright().bold(), style("No --data-dir specified!").dim());
                    println!("{}: {}\n", style("WARNING").red().bright().bold(), style("A temporary data directory will be created and removed after finishing. You probably don't want this.").dim());

                    let confirm = Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt("Are you sure you want to use a temporary data directory?")
                        .interact()
                        .wrap_err("failed to prompt for temporary data directory")?;

                    if !confirm {
                        println!(
                            "{}",
                            style("Refusing to use a temporary data directory.").red()
                        );
                        std::process::exit(1);
                    }
                }
                let temp = TempDir::new()
                    .wrap_err("failed to create a temporary directory")
                    .suggestion("use -d to specify a data directory")?;
                Ok(OutputDir::Temp(temp))
            },
            |dir| Ok(OutputDir::Specified(dir)),
        )?;

        let mut includes = None;

        if let Some(includes_file) = &cli.include_file {
            let includes_str = std::fs::read_to_string(includes_file).wrap_err_with(|| {
                format!("failed to read includes file {}", includes_file.display())
            })?;
            let includes_file = includes_str.parse::<RuleSet>().wrap_err_with(|| {
                format!("failed to parse includes file {}", includes_file.display())
            })?;
            includes = Some(includes_file);
        }

        if !cli.include.is_empty() {
            let includes_str = cli.include.join("\n");
            let includes_file = includes_str
                .parse::<RuleSet>()
                .wrap_err("failed to parse command line includes")?;
            if let Some(includes) = &mut includes {
                includes.merge(includes_file);
            } else {
                includes = Some(includes_file);
            }
        }

        let ffmpeg_args = cli
            .ffmpeg_args
            .split_whitespace()
            .map(ToString::to_string)
            .collect();
        let gource_args = cli
            .gource_args
            .split_whitespace()
            .map(ToString::to_string)
            .collect();

        let cx = Context {
            token: cli.token,
            data_dir,
            output: cli.output,
            skip_clone: cli.skip_clone,
            includes,
            ffmpeg_args,
            gource_args,
        };

        Ok(cx)
    }
}

const NUM_STEPS: usize = 5;

macro_rules! status {
    ($step_idx:literal, $icon:literal, $($args:tt)*) => {
        println!(
            "{} {} {}",
            ::console::style(
                format!("[{}/{}]", $step_idx, NUM_STEPS)
            ).bold().dim(),
            ::emojis::get_by_shortcode($icon).unwrap(),
            format!($($args)*)
        )
    };
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_target(false),
        )
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_error::ErrorLayer::default())
        .init();

    color_eyre::install()?;

    let cli = Cli::parse();
    trace!("parsed args: {cli:?}");

    let cx = Context::from_cli(cli)?;
    trace!("context: {cx:?}");

    let determinate_style = ProgressStyle::with_template(
        "{elapsed:.magenta.bold} {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .wrap_err("failed to create progress style")
    .unwrap()
    .progress_chars("▓▒░");

    let indeterminate_style = ProgressStyle::default_spinner()
        .template("{elapsed:.magenta.bold} {spinner:.green} {msg}")
        .wrap_err("failed to create progress style")
        .unwrap();

    status!(1, "mag", "Fetching repos from GitHub API...");

    let fetch_progress = ProgressBar::new(1);
    fetch_progress.set_style(indeterminate_style.clone());
    fetch_progress.enable_steady_tick(Duration::from_millis(200));

    let mut repos = github::list_repos(&cx, &fetch_progress).wrap_err("failed to list repos")?;
    let initial_len = repos.len();
    trace!("fetched {} repos: {repos:?}", initial_len);

    if let Some(includes) = &cx.includes {
        includes.apply(&mut repos);
    }

    trace!("filtered to {} repos: {repos:?}", repos.len());
    debug!("filtering removed {} repos", initial_len - repos.len());

    fetch_progress.finish();

    status!(
        2,
        "arrow_double_down",
        "Cloning repos...{}",
        if cx.skip_clone { " (skipped)" } else { "" }
    );

    if !cx.skip_clone {
        let clone_progress = ProgressBar::new(repos.len() as u64);
        clone_progress.set_style(determinate_style.clone());

        debug!("cloning/pulling {} repos", repos.len());

        for repo in &repos {
            clone_progress.set_message(repo.full_name());
            github::fetch_repo(&cx, repo)
                .wrap_err_with(|| format!("failed to fetch repo {}", repo.full_name()))?;
            clone_progress.inc(1);
        }

        clone_progress.finish();
    }

    status!(3, "factory", "Generating gource logs...");

    let gource_progress = ProgressBar::new(repos.len() as u64);
    gource_progress.set_style(determinate_style.clone());

    if !cx.data_dir.gource_dir().exists() {
        trace!(
            "creating gource log directory: {}",
            cx.data_dir.gource_dir().display()
        );
        std::fs::create_dir(cx.data_dir.gource_dir())
            .wrap_err("failed to create gource log directory")?;
    }

    debug!("generating gource logs for {} repos", repos.len());
    for repo in &repos {
        gource_progress.set_message(repo.full_name());
        gource::generate_gource_log(&cx, repo)
            .wrap_err_with(|| format!("failed to generate gource log for {}", repo.full_name()))?;
        gource_progress.inc(1);
    }

    gource_progress.finish();

    status!(4, "construction", "Combining and sorting logs...");

    // this step is too fast for a progress bar
    debug!("combining and sorting logs");
    gource::combine_and_sort_logs(&cx, &repos).wrap_err("failed to combine and sort logs")?;

    status!(5, "movie_camera", "Generating gource video...");

    let gource_progress = ProgressBar::new(1);
    gource_progress.set_style(indeterminate_style.clone());
    gource_progress.enable_steady_tick(Duration::from_millis(200));

    debug!("generating gource video");
    gource::generate_gource_video(&cx).wrap_err("failed to generate gource video")?;

    gource_progress.finish();

    println!(
        "      {} Done!",
        ::emojis::get_by_shortcode("tada").unwrap()
    );

    Ok(())
}
