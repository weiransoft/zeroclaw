use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CLAWHUB_API_URL: &str = "https://api.clawhub.com";
const LEARNING_MIN_SUCCESS_RATE: f64 = 0.7;
const LEARNING_MIN_USAGE_COUNT: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub auto_update: bool,
    pub update_interval_hours: u64,
    pub learning_enabled: bool,
    pub min_confidence: f64,
    pub emoji: Option<String>,
    pub homepage: Option<String>,
}

impl Default for SkillMetadata {
    fn default() -> Self {
        Self {
            auto_update: true,
            update_interval_hours: 24,
            learning_enabled: true,
            min_confidence: 0.7,
            emoji: None,
            homepage: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUsageStats {
    pub total_uses: u32,
    pub successful_uses: u32,
    pub failed_uses: u32,
    pub last_used: Option<SystemTime>,
    pub avg_response_time_ms: f64,
}

impl Default for SkillUsageStats {
    fn default() -> Self {
        Self {
            total_uses: 0,
            successful_uses: 0,
            failed_uses: 0,
            last_used: None,
            avg_response_time_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFeedback {
    pub skill_name: String,
    pub tool_name: String,
    pub success: bool,
    pub response_time_ms: f64,
    pub user_rating: Option<u8>,
    pub error_message: Option<String>,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPattern {
    pub pattern_id: String,
    pub skill_name: String,
    pub tool_name: String,
    pub args_pattern: HashMap<String, String>,
    pub success_rate: f64,
    pub usage_count: u32,
    pub last_matched: SystemTime,
}

pub struct SkillLearningEngine {
    min_success_rate: f64,
    min_usage_count: u32,
    confidence_threshold: f64,
    patterns: Vec<SkillPattern>,
    feedback_history: Vec<SkillFeedback>,
    stats: HashMap<String, SkillUsageStats>,
}

impl SkillLearningEngine {
    pub fn new() -> Self {
        Self {
            min_success_rate: LEARNING_MIN_SUCCESS_RATE,
            min_usage_count: LEARNING_MIN_USAGE_COUNT,
            confidence_threshold: 0.7,
            patterns: Vec::new(),
            feedback_history: Vec::new(),
            stats: HashMap::new(),
        }
    }

    pub fn record_feedback(&mut self, feedback: SkillFeedback) {
        let skill_name = feedback.skill_name.clone();
        self.feedback_history.push(feedback);
        
        let stats = self.stats.entry(skill_name).or_default();
        stats.total_uses += 1;
        if self.feedback_history.last().map(|f| f.success).unwrap_or(false) {
            stats.successful_uses += 1;
        } else {
            stats.failed_uses += 1;
        }
        stats.last_used = Some(SystemTime::now());
    }

    pub fn get_skill_confidence(&self, skill_name: &str) -> f64 {
        if let Some(stats) = self.stats.get(skill_name) {
            if stats.total_uses == 0 {
                return 0.5;
            }
            stats.successful_uses as f64 / stats.total_uses as f64
        } else {
            0.5
        }
    }

    pub fn should_suggest_improvement(&self, skill_name: &str) -> bool {
        let confidence = self.get_skill_confidence(skill_name);
        confidence < self.min_success_rate
    }

    pub fn get_usage_stats(&self, skill_name: &str) -> Option<&SkillUsageStats> {
        self.stats.get(skill_name)
    }

    pub fn analyze_patterns(&mut self) -> Vec<SkillPattern> {
        let mut pattern_counts: HashMap<String, (u32, u32)> = HashMap::new();
        
        for feedback in &self.feedback_history {
            let key = format!("{}:{}", feedback.skill_name, feedback.tool_name);
            let (total, success) = pattern_counts.entry(key).or_insert((0, 0));
            *total += 1;
            if feedback.success {
                *success += 1;
            }
        }

        let good_patterns: Vec<SkillPattern> = pattern_counts
            .into_iter()
            .filter(|(_, (total, _))| *total >= self.min_usage_count)
            .map(|(key, (total, success))| {
                let parts: Vec<&str> = key.split(':').collect();
                SkillPattern {
                    pattern_id: uuid::Uuid::new_v4().to_string(),
                    skill_name: parts.get(0).unwrap_or(&"").to_string(),
                    tool_name: parts.get(1).unwrap_or(&"").to_string(),
                    args_pattern: HashMap::new(),
                    success_rate: success as f64 / total as f64,
                    usage_count: total,
                    last_matched: SystemTime::now(),
                }
            })
            .filter(|p| p.success_rate >= self.min_success_rate)
            .collect();

        self.patterns = good_patterns.clone();
        good_patterns
    }

    pub fn save_state(&self, path: &Path) -> Result<()> {
        let state = LearningState {
            patterns: self.patterns.clone(),
            feedback_history: self.feedback_history.clone(),
            stats: self.stats.clone(),
        };
        let content = serde_json::to_string_pretty(&state)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn load_state(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(path)?;
        let state: LearningState = serde_json::from_str(&content)?;
        self.patterns = state.patterns;
        self.feedback_history = state.feedback_history;
        self.stats = state.stats;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct LearningState {
    patterns: Vec<SkillPattern>,
    feedback_history: Vec<SkillFeedback>,
    stats: HashMap<String, SkillUsageStats>,
}

pub struct SkillUpdater {
    check_interval: Duration,
    registry_url: String,
}

impl SkillUpdater {
    pub fn new() -> Self {
        Self {
            check_interval: Duration::from_secs(60 * 60 * 24),
            registry_url: CLAWHUB_API_URL.to_string(),
        }
    }

    pub fn check_updates(&self, skills: &[super::Skill]) -> Vec<SkillUpdate> {
        let mut updates = Vec::new();
        
        for skill in skills {
            if let Ok(latest) = self.fetch_latest_version(&skill.name) {
                if latest != skill.version {
                    updates.push(SkillUpdate {
                        name: skill.name.clone(),
                        current_version: skill.version.clone(),
                        latest_version: latest,
                        location: skill.location.clone(),
                    });
                }
            }
        }
        
        updates
    }

    fn fetch_latest_version(&self, skill_name: &str) -> Result<String> {
        let url = format!("{}/skills/{}/version", self.registry_url, skill_name);
        let response = ureq::get(&url)
            .timeout(Duration::from_secs(10))
            .call()?;
        
        let version: VersionResponse = response.into_json()?;
        Ok(version.version)
    }

    pub fn auto_update(&self, skill: &super::Skill) -> Result<super::Skill> {
        let url = format!("{}/skills/{}/download", self.registry_url, skill.name);
        let response = ureq::get(&url)
            .timeout(Duration::from_secs(60))
            .call()?;
        
        let content = response.into_string()?;
        let updated_skill: super::Skill = serde_json::from_str(&content)?;
        
        if let Some(ref location) = skill.location {
            if let Some(parent) = location.parent() {
                let skill_file = parent.join("SKILL.toml");
                let toml_content = toml::to_string_pretty(&updated_skill)?;
                std::fs::write(skill_file, toml_content)?;
            }
        }
        
        Ok(updated_skill)
    }
}

#[derive(Debug, Clone)]
pub struct SkillUpdate {
    pub name: String,
    pub current_version: String,
    pub latest_version: String,
    pub location: Option<PathBuf>,
}

#[derive(Deserialize)]
struct VersionResponse {
    version: String,
}

pub struct ClawHubClient {
    api_url: String,
    token: Option<String>,
}

impl ClawHubClient {
    pub fn new() -> Self {
        Self {
            api_url: CLAWHUB_API_URL.to_string(),
            token: None,
        }
    }

    pub fn with_token(token: String) -> Self {
        Self {
            api_url: CLAWHUB_API_URL.to_string(),
            token: Some(token),
        }
    }

    pub fn search(&self, query: &str) -> Result<Vec<SkillSearchResult>> {
        let url = format!("{}/skills/search?q={}", self.api_url, 
            urlencoding::encode(query));
        
        let mut request = ureq::get(&url);
        if let Some(ref token) = self.token {
            request = request.set("Authorization", &format!("Bearer {}", token));
        }
        
        let response = request.timeout(Duration::from_secs(10)).call()?;
        let results: SearchResponse = response.into_json()?;
        
        Ok(results.skills)
    }

    pub fn install(&self, slug: &str) -> Result<super::Skill> {
        let url = format!("{}/skills/{}/download", self.api_url, slug);
        
        let mut request = ureq::get(&url);
        if let Some(ref token) = self.token {
            request = request.set("Authorization", &format!("Bearer {}", token));
        }
        
        let response = request.timeout(Duration::from_secs(60)).call()?;
        let skill: super::Skill = response.into_json()?;
        
        Ok(skill)
    }

    pub fn publish(&self, skill: &super::Skill) -> Result<()> {
        let url = format!("{}/skills", self.api_url);
        
        let token = self.token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Authentication required"))?;
        
        let body = serde_json::to_string(skill)?;
        
        ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", token))
            .set("Content-Type", "application/json")
            .send_string(&body)?;
        
        Ok(())
    }

    pub fn sync(&self, local_skills: &[super::Skill]) -> Result<Vec<SkillSyncResult>> {
        let mut results = Vec::new();
        
        for skill in local_skills {
            let url = format!("{}/skills/{}/sync", self.api_url, skill.name);
            
            if let Some(ref token) = self.token {
                let response = ureq::post(&url)
                    .set("Authorization", &format!("Bearer {}", token))
                    .timeout(Duration::from_secs(10))
                    .call();
                
                match response {
                    Ok(resp) => {
                        let sync_result: SkillSyncResult = resp.into_json()?;
                        results.push(sync_result);
                    }
                    Err(e) => {
                        results.push(SkillSyncResult {
                            name: skill.name.clone(),
                            status: "error".to_string(),
                            message: e.to_string(),
                        });
                    }
                }
            }
        }
        
        Ok(results)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchResult {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub downloads: u64,
    pub rating: f64,
}

#[derive(Deserialize)]
struct SearchResponse {
    skills: Vec<SkillSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSyncResult {
    pub name: String,
    pub status: String,
    pub message: String,
}

impl Default for SkillLearningEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SkillUpdater {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ClawHubClient {
    fn default() -> Self {
        Self::new()
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
}
