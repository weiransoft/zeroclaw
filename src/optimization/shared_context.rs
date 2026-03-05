use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ToolUsageRecord {
    pub tool_name: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub result_summary: String,
}

#[derive(Debug, Clone)]
pub struct DecisionRecord {
    pub decision: String,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
    pub subagent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub change_type: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DependencyInfo {
    pub name: String,
    pub version: String,
    pub dep_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceKnowledge {
    pub project_structure: String,
    pub key_files: HashMap<String, String>,
    pub dependencies: Vec<DependencyInfo>,
    pub recent_changes: Vec<FileChange>,
}

pub struct SharedContext {
    workspace_dir: PathBuf,
    workspace_knowledge: Arc<RwLock<WorkspaceKnowledge>>,
    tool_history: Arc<RwLock<Vec<ToolUsageRecord>>>,
    decisions: Arc<RwLock<Vec<DecisionRecord>>>,
    cache: Arc<RwLock<HashMap<String, CachedValue>>>,
    max_shared_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct CachedValue {
    pub value: String,
    pub timestamp: DateTime<Utc>,
    pub ttl_seconds: u64,
}

impl CachedValue {
    pub fn new(value: String, ttl_seconds: u64) -> Self {
        Self {
            value,
            timestamp: Utc::now(),
            ttl_seconds,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        let elapsed = (Utc::now() - self.timestamp).num_seconds() as u64;
        elapsed > self.ttl_seconds
    }
}

impl SharedContext {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self {
            workspace_dir,
            workspace_knowledge: Arc::new(RwLock::new(WorkspaceKnowledge::default())),
            tool_history: Arc::new(RwLock::new(Vec::new())),
            decisions: Arc::new(RwLock::new(Vec::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_shared_tokens: 1000,
        }
    }
    
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_shared_tokens = max_tokens;
        self
    }
    
    pub async fn record_tool_usage(&self, tool_name: &str, success: bool, result_summary: &str) {
        let record = ToolUsageRecord {
            tool_name: tool_name.to_string(),
            timestamp: Utc::now(),
            success,
            result_summary: result_summary.to_string(),
        };
        
        let mut history = self.tool_history.write().await;
        history.push(record);
        
        if history.len() > 100 {
            history.remove(0);
        }
    }
    
    pub async fn record_decision(&self, decision: &str, rationale: &str, subagent_id: Option<Uuid>) {
        let record = DecisionRecord {
            decision: decision.to_string(),
            rationale: rationale.to_string(),
            timestamp: Utc::now(),
            subagent_id,
        };
        
        let mut decisions = self.decisions.write().await;
        decisions.push(record);
        
        if decisions.len() > 50 {
            decisions.remove(0);
        }
    }
    
    pub async fn update_project_structure(&self, structure: &str) {
        let mut knowledge = self.workspace_knowledge.write().await;
        knowledge.project_structure = structure.to_string();
    }
    
    pub async fn add_key_file(&self, path: &str, content: &str) {
        let mut knowledge = self.workspace_knowledge.write().await;
        let truncated = if content.len() > 5000 {
            format!("{}...", &content[..5000])
        } else {
            content.to_string()
        };
        knowledge.key_files.insert(path.to_string(), truncated);
        
        if knowledge.key_files.len() > 20 {
            let first_key = knowledge.key_files.keys().next().cloned();
            if let Some(key) = first_key {
                knowledge.key_files.remove(&key);
            }
        }
    }
    
    pub async fn get_cached(&self, key: &str) -> Option<String> {
        let cache = self.cache.read().await;
        cache.get(key).and_then(|v| {
            if v.is_expired() {
                None
            } else {
                Some(v.value.clone())
            }
        })
    }
    
    pub async fn set_cached(&self, key: &str, value: &str, ttl_seconds: u64) {
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), CachedValue::new(value.to_string(), ttl_seconds));
    }
    
    pub async fn build_shared_context_prompt(&self) -> String {
        let mut prompt = String::new();
        let mut current_tokens = 0;
        let max_tokens = self.max_shared_tokens;
        
        let knowledge = self.workspace_knowledge.read().await;
        if !knowledge.project_structure.is_empty() {
            let structure = &knowledge.project_structure;
            let tokens = structure.len() / 4;
            if current_tokens + tokens < max_tokens {
                prompt.push_str(&format!("## Project Structure\n{}\n\n", structure));
                current_tokens += tokens;
            }
        }
        
        let decisions = self.decisions.read().await;
        if !decisions.is_empty() {
            let decisions_str = decisions.iter()
                .rev()
                .take(5)
                .map(|d| format!("- {}: {}", d.decision, d.rationale))
                .collect::<Vec<_>>()
                .join("\n");
            let tokens = decisions_str.len() / 4;
            if current_tokens + tokens < max_tokens {
                prompt.push_str(&format!("## Key Decisions\n{}\n\n", decisions_str));
                current_tokens += tokens;
            }
        }
        
        let tool_history = self.tool_history.read().await;
        if !tool_history.is_empty() {
            let recent_tools = tool_history.iter()
                .rev()
                .take(10)
                .map(|t| format!("- {} ({})", t.tool_name, if t.success { "✓" } else { "✗" }))
                .collect::<Vec<_>>()
                .join("\n");
            let tokens = recent_tools.len() / 4;
            if current_tokens + tokens < max_tokens {
                prompt.push_str(&format!("## Recent Tool Usage\n{}\n\n", recent_tools));
            }
        }
        
        prompt
    }
    
    pub fn workspace_dir(&self) -> &PathBuf {
        &self.workspace_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_shared_context_creation() {
        let ctx = SharedContext::new(PathBuf::from("/tmp/test"));
        assert_eq!(ctx.workspace_dir(), &PathBuf::from("/tmp/test"));
    }
    
    #[tokio::test]
    async fn test_record_tool_usage() {
        let ctx = SharedContext::new(PathBuf::from("/tmp/test"));
        ctx.record_tool_usage("shell", true, "executed successfully").await;
        
        let history = ctx.tool_history.read().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].tool_name, "shell");
    }
    
    #[tokio::test]
    async fn test_record_decision() {
        let ctx = SharedContext::new(PathBuf::from("/tmp/test"));
        ctx.record_decision("Use Rust", "Performance critical", None).await;
        
        let decisions = ctx.decisions.write().await;
        assert_eq!(decisions.len(), 1);
    }
    
    #[tokio::test]
    async fn test_build_shared_context_prompt() {
        let ctx = SharedContext::new(PathBuf::from("/tmp/test"));
        ctx.update_project_structure("src/\n  main.rs\n  lib.rs").await;
        
        let prompt = ctx.build_shared_context_prompt().await;
        assert!(prompt.contains("Project Structure"));
    }
    
    #[tokio::test]
    async fn test_cache() {
        let ctx = SharedContext::new(PathBuf::from("/tmp/test"));
        ctx.set_cached("test_key", "test_value", 60).await;
        
        let value = ctx.get_cached("test_key").await;
        assert_eq!(value, Some("test_value".to_string()));
    }
}
