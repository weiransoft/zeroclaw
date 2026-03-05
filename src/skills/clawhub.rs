use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const CLAWHUB_API_BASE: &str = "https://api.clawhub.io/v1";
const DEFAULT_CACHE_TTL_SECS: u64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub category: SkillCategory,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub rating: f64,
    pub downloads: u64,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default)]
    pub examples: Vec<SkillExample>,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Memory,
    Analysis,
    Automation,
    Integration,
    Communication,
    Development,
    Research,
    Creative,
}

impl std::fmt::Display for SkillCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "记忆管理"),
            Self::Analysis => write!(f, "数据分析"),
            Self::Automation => write!(f, "自动化"),
            Self::Integration => write!(f, "系统集成"),
            Self::Communication => write!(f, "通信协作"),
            Self::Development => write!(f, "开发辅助"),
            Self::Research => write!(f, "研究探索"),
            Self::Creative => write!(f, "创意生成"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    pub title: String,
    pub description: String,
    pub usage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchResult {
    pub skills: Vec<SkillInfo>,
    pub total: u64,
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDownloadRequest {
    pub skill_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    pub requested_by: String,
    pub reason: String,
    pub priority: SkillPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillPriority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillApproval {
    pub id: String,
    pub request: SkillDownloadRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<SkillInfo>,
    pub status: SkillApprovalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Downloading,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub id: String,
    pub name: String,
    pub version: String,
    pub installed_at: i64,
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawHubConfig {
    pub enabled: bool,
    pub api_url: String,
    pub auto_discover: bool,
    pub auto_update: bool,
    pub skills_dir: String,
    pub cache_ttl_seconds: u64,
    pub require_approval: bool,
    pub auto_approve_trusted: bool,
    pub trusted_authors: Vec<String>,
}

impl Default for ClawHubConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_url: CLAWHUB_API_BASE.to_string(),
            auto_discover: true,
            auto_update: false,
            skills_dir: ".clawhub".to_string(),
            cache_ttl_seconds: DEFAULT_CACHE_TTL_SECS,
            require_approval: true,
            auto_approve_trusted: false,
            trusted_authors: vec!["official".to_string(), "verified".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawHubStats {
    pub total_installed: usize,
    pub by_category: HashMap<String, usize>,
    pub last_sync: Option<i64>,
    pub pending_approvals: usize,
}

pub struct ClawHubClient {
    config: ClawHubConfig,
    workspace_dir: PathBuf,
    cache: HashMap<String, (Vec<u8>, SystemTime)>,
    http_client: reqwest::Client,
}

impl ClawHubClient {
    pub fn new(workspace_dir: PathBuf, config: ClawHubConfig) -> Self {
        Self {
            config,
            workspace_dir,
            cache: HashMap::new(),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_default_config(workspace_dir: PathBuf) -> Self {
        Self::new(workspace_dir, ClawHubConfig::default())
    }

    pub async fn search_skills(
        &self,
        query: &str,
        options: Option<SkillSearchOptions>,
    ) -> Result<SkillSearchResult> {
        let mut params = vec![("q", query.to_string())];
        
        if let Some(opts) = options {
            if let Some(caps) = opts.capabilities {
                params.push(("capabilities", caps.join(",")));
            }
            if let Some(cat) = opts.category {
                params.push(("category", serde_json::to_string(&cat)?));
            }
            if let Some(tags) = opts.tags {
                params.push(("tags", tags.join(",")));
            }
            if let Some(limit) = opts.limit {
                params.push(("limit", limit.to_string()));
            }
            if let Some(offset) = opts.offset {
                params.push(("offset", offset.to_string()));
            }
        }

        let url = format!("{}/skills/search?{}", self.config.api_url, 
            params.iter()
                .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&")
        );

        let response = self.http_client
            .get(&url)
            .send()
            .await?
            .json::<SkillSearchResult>()
            .await?;

        Ok(response)
    }

    pub async fn list_skills(&self, options: Option<SkillListOptions>) -> Result<SkillSearchResult> {
        let mut params = Vec::new();
        
        if let Some(opts) = options {
            if let Some(cat) = opts.category {
                params.push(("category", serde_json::to_string(&cat)?));
            }
            if let Some(sort) = opts.sort {
                params.push(("sort", sort));
            }
            if let Some(limit) = opts.limit {
                params.push(("limit", limit.to_string()));
            }
            if let Some(offset) = opts.offset {
                params.push(("offset", offset.to_string()));
            }
        }

        let url = if params.is_empty() {
            format!("{}/skills", self.config.api_url)
        } else {
            format!("{}/skills?{}", self.config.api_url,
                params.iter()
                    .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                    .collect::<Vec<_>>()
                    .join("&")
            )
        };

        let response = self.http_client
            .get(&url)
            .send()
            .await?
            .json::<SkillSearchResult>()
            .await?;

        Ok(response)
    }

    pub async fn get_skill(&self, skill_id: &str) -> Result<Option<SkillInfo>> {
        let url = format!("{}/skills/{}", self.config.api_url, skill_id);
        
        let response = self.http_client
            .get(&url)
            .send()
            .await?;

        if response.status() == 404 {
            return Ok(None);
        }

        let skill = response.json::<SkillInfo>().await?;
        Ok(Some(skill))
    }

    pub async fn get_categories(&self) -> Result<Vec<CategoryInfo>> {
        let url = format!("{}/skills/categories", self.config.api_url);
        
        #[derive(Deserialize)]
        struct CategoriesResponse {
            categories: Vec<CategoryInfo>,
        }

        let response = self.http_client
            .get(&url)
            .send()
            .await?
            .json::<CategoriesResponse>()
            .await?;

        Ok(response.categories)
    }

    pub async fn recommend_skills(
        &self,
        context: Option<&str>,
        current_skills: Option<&[String]>,
    ) -> Result<Vec<SkillInfo>> {
        let mut params = Vec::new();
        
        if let Some(ctx) = context {
            params.push(("context", ctx.to_string()));
        }
        if let Some(skills) = current_skills {
            params.push(("currentSkills", skills.join(",")));
        }

        let url = if params.is_empty() {
            format!("{}/skills/recommend", self.config.api_url)
        } else {
            format!("{}/skills/recommend?{}", self.config.api_url,
                params.iter()
                    .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                    .collect::<Vec<_>>()
                    .join("&")
            )
        };

        #[derive(Deserialize)]
        struct RecommendResponse {
            skills: Vec<SkillInfo>,
        }

        let response = self.http_client
            .get(&url)
            .send()
            .await?
            .json::<RecommendResponse>()
            .await?;

        Ok(response.skills)
    }

    pub async fn download_skill(&self, skill_id: &str, version: Option<&str>) -> Result<DownloadInfo> {
        let url = format!("{}/skills/{}/download", self.config.api_url, skill_id);
        
        #[derive(Serialize)]
        struct DownloadRequest {
            #[serde(skip_serializing_if = "Option::is_none")]
            version: Option<String>,
        }

        let response = self.http_client
            .post(&url)
            .json(&DownloadRequest { version: version.map(|v| v.to_string()) })
            .send()
            .await?
            .json::<DownloadInfo>()
            .await?;

        Ok(response)
    }

    pub fn get_installed_skills(&self) -> Result<Vec<InstalledSkill>> {
        let lock_path = self.workspace_dir
            .join(&self.config.skills_dir)
            .join("lock.json");

        if !lock_path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&lock_path)?;
        let lock: LockFile = serde_json::from_str(&content)?;

        Ok(lock.skills.into_iter().map(|(id, data)| {
            let id_for_path = id.clone();
            let id_for_name = id.clone();
            let path = self.workspace_dir
                .join(&self.config.skills_dir)
                .join(&id_for_path)
                .to_string_lossy()
                .to_string();
            InstalledSkill {
                id,
                name: id_for_name,
                version: data.version,
                installed_at: data.installed_at,
                enabled: true,
                path,
            }
        }).collect())
    }

    pub fn is_skill_installed(&self, skill_id: &str) -> bool {
        let skill_path = self.workspace_dir
            .join(&self.config.skills_dir)
            .join(skill_id);
        skill_path.exists()
    }

    pub fn get_skills_dir(&self) -> PathBuf {
        self.workspace_dir.join(&self.config.skills_dir)
    }

    pub fn config(&self) -> &ClawHubConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: ClawHubConfig) {
        self.config = config;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<SkillCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillListOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<SkillCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    pub id: SkillCategory,
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub download_url: String,
    pub checksum: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockFile {
    version: u32,
    skills: HashMap<String, LockFileSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockFileSkill {
    version: String,
    installed_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_category_display() {
        assert_eq!(SkillCategory::Memory.to_string(), "记忆管理");
        assert_eq!(SkillCategory::Development.to_string(), "开发辅助");
    }

    #[test]
    fn test_default_config() {
        let config = ClawHubConfig::default();
        assert!(config.enabled);
        assert!(config.require_approval);
        assert_eq!(config.cache_ttl_seconds, 3600);
    }

    #[test]
    fn test_skill_priority_serde() {
        let priority = SkillPriority::High;
        let json = serde_json::to_string(&priority).unwrap();
        assert_eq!(json, "\"high\"");
        
        let parsed: SkillPriority = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SkillPriority::High);
    }

    #[test]
    fn test_skill_approval_status_serde() {
        let status = SkillApprovalStatus::Downloading;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"downloading\"");
    }

    #[test]
    fn test_installed_skill() {
        let skill = InstalledSkill {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            version: "1.0.0".to_string(),
            installed_at: 1234567890,
            enabled: true,
            path: "/path/to/skill".to_string(),
        };
        
        assert_eq!(skill.id, "test-skill");
        assert!(skill.enabled);
    }
}
