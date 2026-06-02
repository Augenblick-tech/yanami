use crate::entity::cap::RegexCaps;
use dashmap::DashMap;
use domain::shared::error::DomainError;

pub struct RegexCache {
    cache: DashMap<String, regex::Regex>,
}

impl RegexCache {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }
}

impl RegexCaps for RegexCache {
    fn is_match(
        &self,
        pattern: &str,
        text: &str,
    ) -> Result<bool, domain::shared::error::DomainError> {
        if let Some(re) = self.cache.get(pattern) {
            return Ok(re.is_match(text));
        }
        let reg = regex::Regex::new(pattern)
            .map_err(|e| DomainError::external("init regex failed", e))?;
        let is_match = reg.is_match(text);
        self.cache.insert(pattern.into(), reg);
        Ok(is_match)
    }

    fn delete_pattern(&self, pattern: &str) {
        self.cache.remove(pattern.into());
    }

    fn verify(&self, pattern: &str) -> Result<(), DomainError> {
        let re = self.cache.get(pattern);
        if re.is_some() {
            return Ok(());
        }
        let reg = regex::Regex::new(pattern)
            .map_err(|e| DomainError::external("init regex failed", e))?;
        self.cache.insert(pattern.into(), reg);
        Ok(())
    }
}
