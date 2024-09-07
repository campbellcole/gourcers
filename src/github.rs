use std::process::{Command, Stdio};

use color_eyre::eyre::{bail, Result, WrapErr};
use indicatif::ProgressBar;
use reqwest::{
    blocking::{Client, Request},
    header::HeaderMap,
    Method,
};
use serde::Deserialize;
use tap::Tap;

use crate::Context;

#[derive(Debug, Deserialize)]
pub struct Repo {
    pub name: String,
    pub full_name: Option<String>,
    pub ssh_url: String,
    pub owner: Owner,
    pub fork: bool,
    pub private: bool,
}

impl Repo {
    #[must_use]
    pub fn full_name(&self) -> String {
        self.full_name
            .clone()
            .unwrap_or_else(|| format!("{}/{}", self.owner.login, self.name))
    }

    #[must_use]
    pub fn full_name_path_friendly(&self) -> String {
        self.full_name().replace('/', "__")
    }
}

#[derive(Debug, Deserialize)]
pub struct Owner {
    pub login: String,
}

pub(crate) fn list_repos(cx: &Context, progress: &ProgressBar) -> Result<Vec<Repo>> {
    let mut headers = HeaderMap::new();

    headers.append(
        "Authorization",
        format!("Bearer {}", &cx.token)
            .parse()
            .wrap_err("failed to parse token into header")?,
    );
    headers.append("User-Agent", "gourcers-ng".parse().unwrap());
    headers.append("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
    headers.append("Accept", "application/vnd.github+json".parse().unwrap());

    trace!("headers: {:?}", headers);

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .wrap_err("failed to build reqwest client")?;

    let mut repos = Vec::new();
    let mut page = 1;

    loop {
        debug!(page = page, "fetching page of repos");
        progress.set_message(format!("Fetching page {page}"));

        let request = Request::new(
            Method::GET,
            format!("https://api.github.com/user/repos?per_page=100&page={page}")
                .parse()
                .unwrap(),
        );

        let response = client
            .execute(request)
            .wrap_err("failed to execute request")?;

        trace!("response: {:?}", response);

        let response = response.error_for_status().wrap_err("request failed")?;

        let page_repos: Vec<Repo> = response.json().wrap_err("failed to parse response")?;

        trace!(len = page_repos.len(), page = page, "fetched page of repos");

        if page_repos.is_empty() {
            break;
        }

        repos.extend(page_repos);
        page += 1;
    }

    Ok(repos)
}

/// Clone or pull the given repo into the repos directory.
pub(crate) fn fetch_repo(cx: &Context, repo: &Repo) -> Result<()> {
    let repo_dir = cx.data_dir.repo_dir(repo);

    let mut cmd = Command::new("git");

    cmd.stderr(Stdio::piped()).stdout(Stdio::piped());

    if repo_dir.exists() {
        let output = cmd
            .arg("pull")
            .current_dir(&repo_dir)
            .tap(|cmd| {
                trace!(command = ?cmd, repo = %repo.name, "running git pull");
            })
            .output()
            .wrap_err("failed to run git pull")?;

        if !output.status.success() {
            bail!(
                "git pull failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    } else {
        let output = cmd
            .arg("clone")
            .arg(&repo.ssh_url)
            .arg(&repo_dir)
            .tap(|cmd| {
                trace!(command = ?cmd, repo = %repo.name, "running git clone");
            })
            .output()
            .wrap_err("failed to run git clone")?;

        if !output.status.success() {
            bail!(
                "git clone failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    }

    Ok(())
}
