//! An ignore file format which describes a set of repos to ignore.
//!
//! The format is a selector, followed by the name of the value in the given selector.
//!
//! The selector can be one of:
//! - `*`: matches all repos
//! - `owner`: the owner of the repo
//! - `name`: the name of the repo
//! - `full_name`: the full name of the repo, which is the owner and name separated by a slash
//! - `is_fork`: whether the repo is a fork
//!
//! The value is a string which is matched against the value of the selector.
//!
//! Examples:
//! - `*:*`
//! - `name:rust`
//! - `!owner:rust-lang`
//! - `full_name:rust-lang/rust`
//! - `is_fork:true`

use std::{fmt::Display, str::FromStr};

use thiserror::Error;

use crate::gh::Repo;

#[derive(Debug, Error)]
pub enum IgnoreFileErrorKind {
    #[error("Invalid selector: {0:?}")]
    InvalidSelector(Option<String>),
    #[error("Value must be a bool: {0}")]
    InvalidBool(String),
    #[error("Selector has no value: {0}")]
    MissingValue(String),
}

#[derive(Debug, Error)]
pub struct IgnoreFileError {
    pub kind: IgnoreFileErrorKind,
    pub line: usize,
}

impl Display for IgnoreFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.kind)
    }
}

impl From<(usize, IgnoreFileErrorKind)> for IgnoreFileError {
    fn from((line, kind): (usize, IgnoreFileErrorKind)) -> Self {
        Self { kind, line }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IgnoreFile {
    pub(crate) entries: Vec<IgnoreEntry>,
    pub(crate) inverted_entries: Vec<IgnoreEntry>,
}

impl FromStr for IgnoreFile {
    type Err = IgnoreFileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ignore_file = Self::new();

        for (x, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            let mut parts = line.splitn(2, ':');

            let mut selector_part = parts.next();

            let inverted = selector_part.map_or(false, |sel| sel.starts_with('!'));

            if inverted {
                selector_part = selector_part.map(|sel| &sel[1..]);
            }

            let selector = match selector_part {
                Some("*") => IgnoreSelector::All,
                Some("owner") => IgnoreSelector::Owner,
                Some("name") => IgnoreSelector::Name,
                Some("full_name") => IgnoreSelector::FullName,
                Some("is_fork") => IgnoreSelector::IsFork,
                part => {
                    return Err((
                        x,
                        IgnoreFileErrorKind::InvalidSelector(part.map(|s| s.to_string())),
                    )
                        .into());
                }
            };

            let value = match parts.next() {
                Some(v) => v,
                None => {
                    return Err((x, IgnoreFileErrorKind::MissingValue(line.to_string())).into());
                }
            };

            if matches!(selector, IgnoreSelector::IsFork) && value != "true" && value != "false" {
                return Err((x, IgnoreFileErrorKind::InvalidBool(value.to_string())).into());
            }

            let entry = IgnoreEntry::new(selector, value);
            if inverted {
                ignore_file.inverted_entries.push(entry);
                continue;
            } else {
                ignore_file.entries.push(entry)
            }
        }

        Ok(ignore_file)
    }
}

pub enum FilterResult<'a> {
    Keep(&'a IgnoreEntry),
    /// The repo should not be ignored.
    Ignore(&'a IgnoreEntry, &'a IgnoreEntry),
    /// The repo should be included.
    Default,
}

impl<'a> FilterResult<'a> {
    pub fn keep(&self) -> bool {
        matches!(self, Self::Keep(_))
    }
}

impl IgnoreFile {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            inverted_entries: Vec::new(),
        }
    }

    /// Returns the entry which matches the given repo, if any.
    ///
    /// If the repo matches but an exclusion also matches, then `None` is returned, and the repo is not ignored.
    pub fn is_ignored(&self, repo: &Repo) -> FilterResult<'_> {
        let Some(exclusion) = self.inverted_entries.iter().find(|entry| entry.matches(repo)) else {
            return FilterResult::Default;
        };
        let Some(filter) = self.entries.iter().find(|entry| entry.matches(repo)) else {
            return FilterResult::Keep(exclusion);
        };
        FilterResult::Ignore(exclusion, filter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreEntry {
    pub(crate) selector: IgnoreSelector,
    pub(crate) value: String,
}

impl IgnoreEntry {
    pub fn describe(&self) -> String {
        let sel = match self.selector {
            IgnoreSelector::All => return "* is enabled".into(),
            IgnoreSelector::Owner => "owner",
            IgnoreSelector::Name => "name",
            IgnoreSelector::FullName => "full_name",
            IgnoreSelector::IsFork => return "repository is a fork".into(),
        };

        format!("{} is {:?}", sel, self.value)
    }
}

impl IgnoreEntry {
    pub fn new(selector: IgnoreSelector, value: impl ToString) -> Self {
        Self {
            selector,
            value: value.to_string(),
        }
    }

    pub fn matches(&self, repo: &Repo) -> bool {
        match self.selector {
            IgnoreSelector::All => true,
            IgnoreSelector::Owner => repo.owner.login == self.value,
            IgnoreSelector::Name => repo.name == self.value,
            IgnoreSelector::FullName => matches!(repo.full_name, Some(ref v) if v == &self.value),
            IgnoreSelector::IsFork => repo.fork.to_string() == self.value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreSelector {
    All,
    Owner,
    Name,
    FullName,
    IsFork,
}
