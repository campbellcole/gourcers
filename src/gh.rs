use anyhow::{anyhow, Context, Result};
use reqwest::{header::HeaderMap, Client, Method};
use tokio::{fs::write, process::Command};

use crate::Settings;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    pub full_name: Option<String>,
    pub ssh_url: String,
    pub owner: Owner,
    pub fork: bool,
}

pub async fn get_user_repos(settings: &Settings) -> Result<Vec<Repo>> {
    let mut headers = HeaderMap::new();
    headers.append(
        "Authorization",
        format!("Bearer {}", &settings.token)
            .parse()
            .context("failed to parse auth header")?,
    );
    headers.append("User-Agent", "gourcers".parse().unwrap());
    headers.append("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
    headers.append("Accept", "application/vnd.github+json".parse().unwrap());

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to build client")?;

    let mut repos = Vec::new();
    let mut page = 1;

    loop {
        let req = reqwest::Request::new(
            Method::GET,
            format!("https://api.github.com/user/repos?per_page=100&page={page}")
                .parse()
                .unwrap(),
        );
        let res = client
            .execute(req)
            .await
            .context("failed to execute github api request")?;
        let res_str = res
            .text()
            .await
            .context("failed to read github api response")?;

        if matches!(settings.dump_requests, Some(true)) {
            write(format!("api_page{}.json", page), &res_str)
                .await
                .context(format!("failed to dump request for page {page}"))?;
        }

        let page_repos = serde_json::from_str::<Vec<Repo>>(&res_str)
            .context("failed to parse github api response")?;

        if page_repos.is_empty() {
            break;
        }

        repos.extend(page_repos);
        page += 1;
    }

    Ok(repos)
}

/// Clones a repo if it does not exist, and pulls if it does.
pub async fn fetch_repo(settings: &Settings, repo: &Repo) -> Result<()> {
    let repo_folder = settings.repo_folder(repo);
    let mut cmd = Command::new("git");

    if repo_folder.try_exists()? {
        debug!("repo exists, pulling: {}", repo.name);

        cmd.arg("pull").arg("origin").current_dir(&repo_folder);
    } else {
        debug!("repo does not exist, cloning: {}", repo.name);

        cmd.arg("clone").arg(&repo.ssh_url).arg(&repo_folder);
    }

    let output = cmd
        .output()
        .await
        .context("failed to execute git command")?;

    if !matches!(output.status.code(), Some(0)) {
        return Err(anyhow!(
            "git clone/pull failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}
