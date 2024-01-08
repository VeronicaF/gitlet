use anyhow::{ensure, Context};
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub struct GitIgnore {
    pub global: Vec<Vec<Rule>>,
    pub local: IndexMap<String, Vec<Rule>>,
}

#[derive(Debug, PartialEq)]
pub enum Rule {
    Negation(String),
    Pattern(String),
}

impl Default for GitIgnore {
    fn default() -> Self {
        Self {
            global: vec![],
            local: IndexMap::new(),
        }
    }
}

impl GitIgnore {
    // todo do not clone the string
    pub fn parse(lines: &str) -> Vec<Rule> {
        lines
            .trim()
            .split('\n')
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    None
                } else {
                    match line.chars().next() {
                        Some('!') => {
                            let pattern = &line[1..];

                            if Path::new(pattern).is_dir() {
                                Some(Rule::Negation(format!("{}/**", pattern)))
                            } else {
                                Some(Rule::Negation(pattern.to_string()))
                            }
                        }
                        Some('\\') | Some('/') => {
                            let pattern = &line[1..];

                            if Path::new(pattern).is_dir() {
                                Some(Rule::Pattern(format!("{}/**", pattern)))
                            } else {
                                Some(Rule::Pattern(pattern.to_string()))
                            }
                        }
                        _ => {
                            let pattern = line;

                            if Path::new(pattern).is_dir() {
                                Some(Rule::Pattern(format!("{}/**", pattern)))
                            } else {
                                Some(Rule::Pattern(pattern.to_string()))
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>()
    }

    fn check_rules(rules: &Vec<Rule>, path: &str) -> Option<bool> {
        for rule in rules {
            match rule {
                Rule::Negation(pattern) => {
                    let glob = glob::Pattern::new(pattern)
                        .context("invalid glob pattern")
                        .ok()?;

                    if glob.matches(path) {
                        return Some(false);
                    }
                }
                Rule::Pattern(pattern) => {
                    let glob = glob::Pattern::new(pattern).ok()?;

                    if glob.matches(path) {
                        return Some(true);
                    }
                }
            }
        }

        None
    }

    pub fn check(&self, path: &str) -> anyhow::Result<Option<bool>> {
        let pathbuf = PathBuf::from(path);

        ensure!(
            pathbuf.is_relative(),
            "path must be relative to the repository root"
        );

        if let Some(result) = self.check_scoped(path) {
            return Ok(Some(result));
        }

        Ok(self.check_global(path))
    }

    pub fn check_scoped(&self, path: &str) -> Option<bool> {
        let mut parent = PathBuf::from(path);
        parent.pop();

        loop {
            let parent_str = parent.to_str().unwrap();
            if let Some(rules) = self.local.get(parent_str) {
                if let Some(result) = Self::check_rules(rules, path) {
                    return Some(result);
                }
            }

            if !parent.pop() {
                break;
            }
        }
        None
    }

    pub fn check_global(&self, path: &str) -> Option<bool> {
        for rules in &self.global {
            if let Some(result) = Self::check_rules(rules, path) {
                return Some(result);
            }
        }
        None
    }
}
