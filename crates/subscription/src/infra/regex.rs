use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use regex::Regex;

use crate::entity::cap::RuleMatcher;

#[derive(Clone)]
pub struct RegexRuleMatcher {
    cache: Arc<DashMap<String, Regex>>,
}

impl RegexRuleMatcher {
    pub fn new(cache: Arc<DashMap<String, Regex>>) -> Self {
        Self { cache }
    }

    pub fn remove(&self, pattern: &str) {
        self.cache.remove(pattern);
    }
}

#[async_trait]
impl RuleMatcher for RegexRuleMatcher {
    fn is_match(&self, pattern: &str, text: &str) -> bool {
        if let Some(re) = self.cache.get(pattern) {
            return re.is_match(text);
        }

        if let Ok(re) = Regex::new(pattern) {
            let res = re.is_match(text);
            self.cache.insert(pattern.to_string(), re);
            return res;
        }

        false
    }

    fn validate(&self, pattern: &str) -> Result<()> {
        let re = regex::Regex::new(pattern)?;
        self.cache.insert(pattern.to_string(), re);
        Ok(())
    }
}
