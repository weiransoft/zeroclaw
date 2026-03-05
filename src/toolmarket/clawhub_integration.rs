use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::skills::clawhub::{ClawHubClient, SkillInfo, InstalledSkill};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawHubIntegrationConfig {
    pub enabled: bool,
    pub api_url: String,
    pub auto_discover: bool,
    pub auto_update: bool,
    pub require_approval: bool,
    pub auto_approve_trusted: bool,
    pub trusted_authors: Vec<String>,
}

impl Default for ClawHubIntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_url: "https://api.clawhub.io/v1".to_string(),
            auto_discover: true,
            auto_update: false,
            require_approval: true,
            auto_approve_trusted: false,
            trusted_authors: vec!["official".to_string(), "verified".to_string()],
        }
    }
}

pub struct ClawHubIntegration {
    client: ClawHubClient,
    config: ClawHubIntegrationConfig,
}

impl ClawHubIntegration {
    pub fn new(workspace_dir: PathBuf, config: ClawHubIntegrationConfig) -> Self {
        let clawhub_config = crate::skills::clawhub::ClawHubConfig {
            enabled: config.enabled,
            api_url: config.api_url.clone(),
            auto_discover: config.auto_discover,
            auto_update: config.auto_update,
            skills_dir: ".clawhub".to_string(),
            cache_ttl_seconds: 3600,
            require_approval: config.require_approval,
            auto_approve_trusted: config.auto_approve_trusted,
            trusted_authors: config.trusted_authors.clone(),
        };

        Self {
            client: ClawHubClient::new(workspace_dir, clawhub_config),
            config,
        }
    }

    pub async fn search_skills(
        &self,
        query: &str,
        category: Option<&str>,
        tags: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<SkillInfo>> {
        let options = crate::skills::clawhub::SkillSearchOptions {
            capabilities: None,
            category: category.map(|c| {
                serde_json::from_str(c).unwrap_or(crate::skills::clawhub::SkillCategory::Development)
            }),
            tags: tags.map(|t| t.to_vec()),
            limit,
            offset,
        };

        let result = self.client.search_skills(query, Some(options)).await?;
        Ok(result.skills)
    }

    pub async fn list_skills(
        &self,
        category: Option<&str>,
        sort: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<SkillInfo>> {
        let options = crate::skills::clawhub::SkillListOptions {
            category: category.map(|c| {
                serde_json::from_str(c).unwrap_or(crate::skills::clawhub::SkillCategory::Development)
            }),
            sort: sort.map(|s| s.to_string()),
            limit,
            offset,
        };

        let result = self.client.list_skills(Some(options)).await?;
        Ok(result.skills)
    }

    pub async fn get_skill(&self, skill_id: &str) -> Result<Option<SkillInfo>> {
        self.client.get_skill(skill_id).await
    }

    pub async fn get_categories(&self) -> Result<Vec<crate::skills::clawhub::CategoryInfo>> {
        self.client.get_categories().await
    }

    pub async fn recommend_skills(
        &self,
        context: Option<&str>,
        current_skills: Option<&[String]>,
    ) -> Result<Vec<SkillInfo>> {
        self.client.recommend_skills(context, current_skills).await
    }

    pub async fn download_skill(&self, skill_id: &str, version: Option<&str>) -> Result<crate::skills::clawhub::DownloadInfo> {
        self.client.download_skill(skill_id, version).await
    }

    pub fn get_installed_skills(&self) -> Result<Vec<InstalledSkill>> {
        self.client.get_installed_skills()
    }

    pub fn is_skill_installed(&self, skill_id: &str) -> bool {
        self.client.is_skill_installed(skill_id)
    }

    pub fn get_skills_dir(&self) -> PathBuf {
        self.client.get_skills_dir()
    }

    pub fn config(&self) -> &ClawHubIntegrationConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: ClawHubIntegrationConfig) {
        let clawhub_config = crate::skills::clawhub::ClawHubConfig {
            enabled: config.enabled,
            api_url: config.api_url.clone(),
            auto_discover: config.auto_discover,
            auto_update: config.auto_update,
            skills_dir: ".clawhub".to_string(),
            cache_ttl_seconds: 3600,
            require_approval: config.require_approval,
            auto_approve_trusted: config.auto_approve_trusted,
            trusted_authors: config.trusted_authors.clone(),
        };
        self.config = config;
        self.client.update_config(clawhub_config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_clawhub_integration_config_default() {
        let config = ClawHubIntegrationConfig::default();
        assert!(config.enabled);
        assert_eq!(config.api_url, "https://api.clawhub.io/v1");
        assert!(config.require_approval);
        assert!(!config.auto_approve_trusted);
        assert!(config.trusted_authors.contains(&"official".to_string()));
        assert!(config.trusted_authors.contains(&"verified".to_string()));
    }

    #[test]
    fn test_clawhub_integration_creation() {
        let temp_dir = tempdir().unwrap();
        let config = ClawHubIntegrationConfig::default();
        let integration = ClawHubIntegration::new(temp_dir.path().to_path_buf(), config);
        
        assert!(integration.config().enabled);
        assert_eq!(integration.get_skills_dir(), temp_dir.path().join(".clawhub"));
    }

    #[test]
    fn test_skill_installed_check() {
        let temp_dir = tempdir().unwrap();
        let config = ClawHubIntegrationConfig::default();
        let integration = ClawHubIntegration::new(temp_dir.path().to_path_buf(), config);
        
        // 测试不存在的技能
        assert!(!integration.is_skill_installed("non-existent-skill"));
    }
}
