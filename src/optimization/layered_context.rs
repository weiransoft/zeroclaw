use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

const MAX_SHARED_TOKENS: usize = 1000;
const MAX_SESSION_TOKENS: usize = 500;
const MAX_DECISIONS: usize = 20;
const MAX_TOOL_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageRecord {
    pub agent_name: String,
    pub tool_name: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub result_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub agent_name: String,
    pub decision: String,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
    pub scope: DecisionScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DecisionScope {
    Private,
    Shared,
    Broadcast,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: String,
    pub timestamp: DateTime<Utc>,
    pub agent_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub version: String,
    pub dep_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceKnowledge {
    pub project_structure: String,
    pub key_files: HashMap<String, String>,
    pub dependencies: Vec<DependencyInfo>,
    pub recent_changes: Vec<FileChange>,
    pub last_updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionKnowledge {
    pub decisions: Vec<DecisionRecord>,
    pub tool_history: Vec<ToolUsageRecord>,
    pub task_progress: HashMap<String, TaskProgress>,
    pub shared_findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: Uuid,
    pub agent_name: String,
    pub status: String,
    pub progress_percent: u8,
    pub last_update: DateTime<Utc>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub agent_name: String,
    pub finding_type: FindingType,
    pub content: String,
    pub relevance: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FindingType {
    CodePattern,
    ApiEndpoint,
    Configuration,
    Error,
    Optimization,
    Dependency,
}

pub struct LayeredSharedContext {
    workspace_dir: PathBuf,
    session_id: Uuid,
    
    workspace_knowledge: Arc<RwLock<WorkspaceKnowledge>>,
    session_knowledge: Arc<RwLock<SessionKnowledge>>,
    
    cache: Arc<RwLock<HashMap<String, CachedValue>>>,
    
    max_shared_tokens: usize,
    max_session_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct CachedValue {
    pub value: String,
    pub timestamp: DateTime<Utc>,
    pub ttl_seconds: u64,
    pub scope: CacheScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheScope {
    Workspace,
    Session,
    Private,
}

impl CachedValue {
    pub fn new(value: String, ttl_seconds: u64, scope: CacheScope) -> Self {
        Self {
            value,
            timestamp: Utc::now(),
            ttl_seconds,
            scope,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        let elapsed = (Utc::now() - self.timestamp).num_seconds() as u64;
        elapsed > self.ttl_seconds
    }
}

impl LayeredSharedContext {
    pub fn new(workspace_dir: PathBuf, session_id: Uuid) -> Self {
        Self {
            workspace_dir,
            session_id,
            workspace_knowledge: Arc::new(RwLock::new(WorkspaceKnowledge::default())),
            session_knowledge: Arc::new(RwLock::new(SessionKnowledge::default())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_shared_tokens: MAX_SHARED_TOKENS,
            max_session_tokens: MAX_SESSION_TOKENS,
        }
    }
    
    pub fn with_token_limits(
        mut self, 
        max_shared: usize, 
        max_session: usize
    ) -> Self {
        self.max_shared_tokens = max_shared;
        self.max_session_tokens = max_session;
        self
    }
    
    pub async fn record_tool_usage(
        &self, 
        agent_name: &str, 
        tool_name: &str, 
        success: bool, 
        result_summary: &str
    ) {
        let record = ToolUsageRecord {
            agent_name: agent_name.to_string(),
            tool_name: tool_name.to_string(),
            timestamp: Utc::now(),
            success,
            result_summary: result_summary.to_string(),
        };
        
        let mut session = self.session_knowledge.write().await;
        session.tool_history.push(record);
        
        if session.tool_history.len() > MAX_TOOL_HISTORY {
            session.tool_history.remove(0);
        }
    }
    
    pub async fn record_decision(
        &self, 
        agent_name: &str, 
        decision: &str, 
        rationale: &str,
        scope: DecisionScope
    ) {
        let record = DecisionRecord {
            agent_name: agent_name.to_string(),
            decision: decision.to_string(),
            rationale: rationale.to_string(),
            timestamp: Utc::now(),
            scope,
        };
        
        let mut session = self.session_knowledge.write().await;
        session.decisions.push(record);
        
        if session.decisions.len() > MAX_DECISIONS {
            session.decisions.remove(0);
        }
    }
    
    pub async fn add_finding(
        &self,
        agent_name: &str,
        finding_type: FindingType,
        content: &str,
        relevance: f64,
    ) {
        let finding = Finding {
            agent_name: agent_name.to_string(),
            finding_type,
            content: content.to_string(),
            relevance,
            timestamp: Utc::now(),
        };
        
        let mut session = self.session_knowledge.write().await;
        session.shared_findings.push(finding);
        
        session.shared_findings.sort_by(|a, b| 
            b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal)
        );
        
        session.shared_findings.truncate(20);
    }
    
    pub async fn update_task_progress(
        &self,
        task_id: Uuid,
        agent_name: &str,
        status: &str,
        progress_percent: u8,
        summary: &str,
    ) {
        let progress = TaskProgress {
            task_id,
            agent_name: agent_name.to_string(),
            status: status.to_string(),
            progress_percent,
            last_update: Utc::now(),
            summary: summary.to_string(),
        };
        
        let mut session = self.session_knowledge.write().await;
        session.task_progress.insert(task_id.to_string(), progress);
    }
    
    pub async fn update_project_structure(&self, structure: &str) {
        let mut workspace = self.workspace_knowledge.write().await;
        workspace.project_structure = structure.to_string();
        workspace.last_updated = Some(Utc::now());
    }
    
    pub async fn add_key_file(&self, path: &str, content: &str) {
        let mut workspace = self.workspace_knowledge.write().await;
        let truncated = if content.len() > 5000 {
            format!("{}...", &content[..5000])
        } else {
            content.to_string()
        };
        workspace.key_files.insert(path.to_string(), truncated);
        
        if workspace.key_files.len() > 20 {
            let first_key = workspace.key_files.keys().next().cloned();
            if let Some(key) = first_key {
                workspace.key_files.remove(&key);
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
    
    pub async fn set_cached(
        &self, 
        key: &str, 
        value: &str, 
        ttl_seconds: u64,
        scope: CacheScope
    ) {
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), CachedValue::new(value.to_string(), ttl_seconds, scope));
    }
    
    pub async fn build_context_for_agent(
        &self,
        _agent_name: &str,
        include_workspace: bool,
        include_session: bool,
    ) -> String {
        let mut context = String::new();
        let mut current_tokens = 0;
        
        if include_workspace {
            let workspace = self.workspace_knowledge.read().await;
            if !workspace.project_structure.is_empty() {
                let section = format!(
                    "## Project Context\n{}\n\n",
                    &workspace.project_structure[..workspace.project_structure.len().min(800)]
                );
                let tokens = section.len() / 4;
                if current_tokens + tokens < self.max_shared_tokens {
                    context.push_str(&section);
                    current_tokens += tokens;
                }
            }
        }
        
        if include_session {
            let session = self.session_knowledge.read().await;
            
            let shared_decisions: Vec<_> = session.decisions.iter()
                .filter(|d| d.scope != DecisionScope::Private)
                .rev()
                .take(5)
                .collect();
            
            if !shared_decisions.is_empty() {
                let decisions_str = shared_decisions.iter()
                    .map(|d| format!("- [{}] {}: {}", d.agent_name, d.decision, d.rationale))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                let section = format!("## Key Decisions\n{}\n\n", decisions_str);
                let tokens = section.len() / 4;
                if current_tokens + tokens < self.max_shared_tokens + self.max_session_tokens {
                    context.push_str(&section);
                    current_tokens += tokens;
                }
            }
            
            if !session.shared_findings.is_empty() {
                let findings_str = session.shared_findings.iter()
                    .take(5)
                    .map(|f| format!("- [{}] {:?}", f.agent_name, f.finding_type))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                let section = format!("## Recent Findings\n{}\n\n", findings_str);
                let tokens = section.len() / 4;
                if current_tokens + tokens < self.max_shared_tokens + self.max_session_tokens {
                    context.push_str(&section);
                }
            }
        }
        
        context
    }
    
    pub async fn build_summary_for_parent(&self, agent_name: &str) -> String {
        let session = self.session_knowledge.read().await;
        
        let my_decisions: Vec<_> = session.decisions.iter()
            .filter(|d| d.agent_name == agent_name)
            .rev()
            .take(3)
            .collect();
        
        let my_findings: Vec<_> = session.shared_findings.iter()
            .filter(|f| f.agent_name == agent_name)
            .take(3)
            .collect();
        
        let mut summary = String::new();
        
        if !my_decisions.is_empty() {
            summary.push_str("### Decisions Made\n");
            for d in my_decisions {
                summary.push_str(&format!("- {}: {}\n", d.decision, d.rationale));
            }
        }
        
        if !my_findings.is_empty() {
            summary.push_str("\n### Key Findings\n");
            for f in my_findings {
                summary.push_str(&format!("- {:?}: {}\n", f.finding_type, f.content));
            }
        }
        
        summary
    }
    
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }
    
    pub fn workspace_dir(&self) -> &PathBuf {
        &self.workspace_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_layered_context_creation() {
        let ctx = LayeredSharedContext::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
        );
        assert!(!ctx.workspace_dir().to_str().unwrap().is_empty());
    }
    
    #[tokio::test]
    async fn test_record_tool_usage() {
        let ctx = LayeredSharedContext::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
        );
        ctx.record_tool_usage("agent1", "shell", true, "executed").await;
        
        let session = ctx.session_knowledge.read().await;
        assert_eq!(session.tool_history.len(), 1);
    }
    
    #[tokio::test]
    async fn test_record_decision_with_scope() {
        let ctx = LayeredSharedContext::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
        );
        ctx.record_decision("agent1", "Use Rust", "Performance", DecisionScope::Shared).await;
        
        let session = ctx.session_knowledge.read().await;
        assert_eq!(session.decisions.len(), 1);
        assert_eq!(session.decisions[0].scope, DecisionScope::Shared);
    }
    
    #[tokio::test]
    async fn test_build_context_for_agent() {
        let ctx = LayeredSharedContext::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
        );
        ctx.update_project_structure("src/\n  main.rs").await;
        ctx.record_decision("agent1", "Use async", "Better performance", DecisionScope::Shared).await;
        
        let context = ctx.build_context_for_agent("agent2", true, true).await;
        assert!(context.contains("Project Context") || context.contains("Decisions"));
    }
    
    #[tokio::test]
    async fn test_add_finding() {
        let ctx = LayeredSharedContext::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
        );
        ctx.add_finding(
            "agent1", 
            FindingType::CodePattern, 
            "Found singleton pattern", 
            0.9
        ).await;
        
        let session = ctx.session_knowledge.read().await;
        assert_eq!(session.shared_findings.len(), 1);
    }
}
