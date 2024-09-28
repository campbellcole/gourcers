//! A file format which describes a set of rules which decide which repos to include and ignore.
//!
//! The format is a selector, followed by the name of the value in the given selector.
//!
//! The selector can be one of:
//! - `*`: matches all repos
//! - `owner`: the owner of the repo
//! - `name`: the name of the repo
//! - `full_name`: the full name of the repo, which is the owner and name separated by a slash
//! - `is_fork`: whether the repo is a fork
//! - `public`: whether the repo is public
//!
//! The value is a string which is matched against the value of the selector.
//!
//! Examples:
//! - `*:*`
//! - `name:rust`
//! - `!owner:rust-lang`
//! - `full_name:rust-lang/rust`
//! - `is_fork:true`
//! - `public:false`

use std::{fmt::Display, str::FromStr};

use thiserror::Error;

use crate::github::Repo;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Error)]
pub enum ErrorKind {
    #[error("Invalid selector: {0:?}")]
    InvalidSelector(Option<String>),
    #[error("Value must be a bool: {0}")]
    InvalidBool(String),
    #[error("Selector has no value: {0}")]
    MissingValue(String),
}

#[derive(Debug, Error)]
pub struct Error {
    pub kind: ErrorKind,
    pub line: usize,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.kind)
    }
}

impl From<(usize, ErrorKind)> for Error {
    fn from((line, kind): (usize, ErrorKind)) -> Self {
        Self { kind, line }
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub(crate) includes: Vec<Entry>,
    pub(crate) excludes: Vec<Entry>,
}

impl FromStr for RuleSet {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut include_file = Self::new();

        for (x, line) in s.lines().enumerate() {
            let line_number = x + 1;
            let mut line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            let exclude = line.starts_with('!');
            if exclude {
                line = &line[1..];
            }

            let entry = line.parse().map_err(|e| Error::from((line_number, e)))?;

            if exclude {
                include_file.excludes.push(entry);
            } else {
                include_file.includes.push(entry);
            }
        }

        Ok(include_file)
    }
}

#[allow(clippy::module_name_repetitions)]
pub enum IncludeResult<'a> {
    /// The repository was explicitly included and no rules excluded it.
    Include(&'a Entry),
    /// The repository was explicitly included but a rule excluded it.
    Exclude(&'a Entry, &'a Entry),
    /// No inclusions matched the repository, so it should be excluded.
    Default,
}

impl<'a> IncludeResult<'a> {
    #[inline]
    #[must_use]
    pub fn keep(&self) -> bool {
        matches!(self, Self::Include(_))
    }
}

impl RuleSet {
    #[must_use]
    pub fn new() -> Self {
        Self {
            includes: Vec::new(),
            excludes: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.includes.extend(other.includes);
        self.excludes.extend(other.excludes);
    }

    pub fn apply(&self, repos: &mut Vec<Repo>) {
        repos.retain(|r| {
            let res = self.test(r);
            match res {
                IncludeResult::Exclude(inclusion, exclusion) => {
                    debug!(
                        "excluding repo {}: {} but {}",
                        r.full_name.as_ref().unwrap_or(&r.name),
                        inclusion.describe(),
                        exclusion.describe()
                    );
                }
                IncludeResult::Include(inclusion) => {
                    debug!(
                        "including repo {}: {}",
                        r.full_name.as_ref().unwrap_or(&r.name),
                        inclusion.describe()
                    );
                }
                IncludeResult::Default => {
                    debug!(
                        "ignoring repo {}: no rules matched",
                        r.full_name.as_ref().unwrap_or(&r.name)
                    );
                }
            }
            res.keep()
        });
    }

    /// Returns the entry which matches the given repo, if any.
    ///
    /// If the repo is included but matches an exclusion, the repo is ignored.
    #[must_use]
    pub fn test(&self, repo: &Repo) -> IncludeResult<'_> {
        let Some(inclusion) = self.includes.iter().find(|entry| entry.matches(repo)) else {
            return IncludeResult::Default;
        };

        let Some(exclusion) = self.excludes.iter().find(|entry| entry.matches(repo)) else {
            return IncludeResult::Include(inclusion);
        };

        IncludeResult::Exclude(inclusion, exclusion)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub(crate) selector: Selector,
    pub(crate) value: String,
}

impl FromStr for Entry {
    type Err = ErrorKind;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut parts = line.splitn(2, ':');

        let selector_part = parts.next();

        let selector = match selector_part {
            Some("*") => Selector::All,
            Some("owner") => Selector::Owner,
            Some("name") => Selector::Name,
            Some("full_name") => Selector::FullName,
            Some("is_fork") => Selector::IsFork,
            Some("public") => Selector::Public,
            part => {
                return Err(ErrorKind::InvalidSelector(part.map(ToString::to_string)));
            }
        };

        let value = match parts.next() {
            Some(value) if !value.is_empty() => value,
            _ => return Err(ErrorKind::MissingValue(line.to_string())),
        };

        if matches!(selector, Selector::IsFork | Selector::Public)
            && value != "true"
            && value != "false"
        {
            return Err(ErrorKind::InvalidBool(value.to_string()));
        }

        Ok(Entry::new(selector, &value))
    }
}

impl Entry {
    #[must_use]
    pub fn describe(&self) -> String {
        let sel = match self.selector {
            Selector::All => return "* is enabled".into(),
            Selector::Owner => "owner",
            Selector::Name => "name",
            Selector::FullName => "full_name",
            Selector::IsFork => "is_fork",
            Selector::Public => "public",
        };

        format!("{} is {:?}", sel, self.value)
    }

    #[must_use]
    pub fn new(selector: Selector, value: &impl ToString) -> Self {
        Self {
            selector,
            value: value.to_string(),
        }
    }

    #[must_use]
    pub fn matches(&self, repo: &Repo) -> bool {
        match self.selector {
            Selector::All => true,
            Selector::Owner => repo.owner.login == self.value,
            Selector::Name => repo.name == self.value,
            Selector::FullName => repo.full_name() == self.value,
            Selector::IsFork => repo.fork.to_string() == self.value,
            Selector::Public => (!repo.private).to_string() == self.value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    All,
    Owner,
    Name,
    FullName,
    IsFork,
    Public,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rule() {
        const CASES: &[&str] = &[
            // valid cases
            "*:*",
            "owner:rust-lang",
            "name:rust",
            "full_name:rust-lang/rust",
            "is_fork:true",
            "public:false",
            "owner:rust-lang:extra",
            "owner:spaces are allowed",
            // invalid cases
            "invalid",
            "owner",
            "owner:",
            "is_fork:no",
            "public:yes",
        ];

        let expected = vec![
            Ok(Entry::new(Selector::All, &"*")),
            Ok(Entry::new(Selector::Owner, &"rust-lang")),
            Ok(Entry::new(Selector::Name, &"rust")),
            Ok(Entry::new(Selector::FullName, &"rust-lang/rust")),
            Ok(Entry::new(Selector::IsFork, &"true")),
            Ok(Entry::new(Selector::Public, &"false")),
            Ok(Entry::new(Selector::Owner, &"rust-lang:extra")),
            Ok(Entry::new(Selector::Owner, &"spaces are allowed")),
            Err(ErrorKind::InvalidSelector(Some("invalid".into()))),
            Err(ErrorKind::MissingValue("owner".into())),
            Err(ErrorKind::MissingValue("owner:".into())),
            Err(ErrorKind::InvalidBool("no".into())),
            Err(ErrorKind::InvalidBool("yes".into())),
        ];

        for (case, expected) in CASES.iter().zip(expected) {
            let actual = case.parse::<Entry>();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_parse_file() {
        const CONTENTS: &str = r"
# include all repos
*:*
# exclude any private repos
!public:false
# exclude forks
!is_fork:true
# exclude rust-lang repos
!owner:rust-lang
        ";

        let contents = CONTENTS.trim();

        let expected = RuleSet {
            includes: vec![Entry::new(Selector::All, &"*")],
            excludes: vec![
                Entry::new(Selector::Public, &"false"),
                Entry::new(Selector::IsFork, &"true"),
                Entry::new(Selector::Owner, &"rust-lang"),
            ],
        };

        let actual = contents.parse::<RuleSet>().unwrap();

        assert_eq!(actual, expected);
    }
}
