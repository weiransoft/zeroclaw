use crate::swarm::chat::{ChatMessage, ChatMessageType, SwarmChatManager};
use crate::swarm::store::SwarmSqliteStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusStatus {
    NotRequested,
    Pending,
    WaitingForClarification,
    Disagreement,
    Reached,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConsensus {
    pub task_id: String,
    pub run_id: Option<String>,
    pub status: ConsensusStatus,
    pub topic: String,
    pub participants: Vec<String>,
    pub votes: HashMap<String, bool>,
    pub disagreements: Vec<DisagreementEntry>,
    pub clarifications: Vec<ClarificationEntry>,
    pub resolution: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisagreementEntry {
    pub participant: String,
    pub issue: String,
    pub suggestion: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationEntry {
    pub question: String,
    pub answer: Option<String>,
    pub questioner: String,
    pub responder: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusProposal {
    pub task_id: String,
    pub topic: String,
    pub description: String,
    pub proposed_by: String,
    pub participants: Vec<String>,
    pub timeout_seconds: u64,
}

pub struct ConsensusManager {
    chat_manager: Arc<SwarmChatManager>,
    store: Arc<SwarmSqliteStore>,
}

impl ConsensusManager {
    pub fn new(workspace_dir: &std::path::Path) -> Self {
        let chat_manager = Arc::new(SwarmChatManager::new(workspace_dir));
        let store = Arc::new(SwarmSqliteStore::new(workspace_dir));
        Self {
            chat_manager,
            store,
        }
    }

    pub fn initiate_consensus(
        &self,
        proposal: ConsensusProposal,
        lang: &str,
    ) -> Result<String> {
        let now = now_unix();
        
        let message_id = self.chat_manager.request_consensus(
            None,
            proposal.task_id.clone(),
            proposal.proposed_by.clone(),
            "orchestrator".to_string(),
            proposal.topic.clone(),
            proposal.participants.clone(),
            lang.to_string(),
        )?;

        tracing::info!(
            "[Consensus] Initiated consensus for task {}: {} by {}",
            proposal.task_id,
            proposal.topic,
            proposal.proposed_by
        );

        Ok(message_id)
    }

    pub fn vote(
        &self,
        task_id: String,
        run_id: Option<Uuid>,
        participant: String,
        agrees: bool,
        reason: String,
        parent_message_id: String,
        lang: &str,
    ) -> Result<String> {
        let message_id = self.chat_manager.respond_consensus(
            run_id,
            task_id.clone(),
            participant.clone(),
            "participant".to_string(),
            agrees,
            reason.clone(),
            parent_message_id,
            lang.to_string(),
        )?;

        tracing::info!(
            "[Consensus] {} voted {} on task {}: {}",
            participant,
            if agrees { "YES" } else { "NO" },
            task_id,
            reason
        );

        Ok(message_id)
    }

    pub fn report_disagreement(
        &self,
        task_id: String,
        run_id: Option<Uuid>,
        participant: String,
        issue: String,
        suggestion: Option<String>,
        lang: &str,
    ) -> Result<String> {
        let message_id = self.chat_manager.report_disagreement(
            run_id,
            task_id.clone(),
            participant.clone(),
            "participant".to_string(),
            issue.clone(),
            suggestion.clone(),
            lang.to_string(),
        )?;

        tracing::warn!(
            "[Consensus] Disagreement reported by {} on task {}: {}",
            participant,
            task_id,
            issue
        );

        Ok(message_id)
    }

    pub fn request_clarification(
        &self,
        task_id: String,
        run_id: Option<Uuid>,
        questioner: String,
        question: String,
        parent_message_id: String,
        lang: &str,
    ) -> Result<String> {
        let message_id = self.chat_manager.request_clarification(
            run_id,
            task_id.clone(),
            questioner.clone(),
            "participant".to_string(),
            question.clone(),
            parent_message_id,
            lang.to_string(),
        )?;

        tracing::info!(
            "[Consensus] Clarification requested by {} on task {}: {}",
            questioner,
            task_id,
            question
        );

        Ok(message_id)
    }

    pub fn provide_clarification(
        &self,
        task_id: String,
        run_id: Option<Uuid>,
        responder: String,
        answer: String,
        parent_message_id: String,
        lang: &str,
    ) -> Result<String> {
        let message_id = self.chat_manager.provide_correction(
            run_id,
            task_id.clone(),
            responder.clone(),
            "orchestrator".to_string(),
            answer.clone(),
            parent_message_id,
            lang.to_string(),
        )?;

        tracing::info!(
            "[Consensus] Clarification provided by {} on task {}: {}",
            responder,
            task_id,
            answer
        );

        Ok(message_id)
    }

    pub fn get_consensus_state(&self, task_id: String) -> Result<TaskConsensus> {
        let messages = self.chat_manager.get_conversation_history(None, Some(task_id.clone()))?;
        
        let mut consensus_request: Option<&ChatMessage> = None;
        let mut participants = std::collections::HashSet::new();
        let mut votes: HashMap<String, bool> = HashMap::new();
        let mut disagreements: Vec<DisagreementEntry> = Vec::new();
        let mut clarifications: Vec<ClarificationEntry> = Vec::new();
        let mut topic = String::new();
        let mut run_id: Option<String> = None;

        for msg in &messages {
            participants.insert(msg.author.clone());
            
            match msg.message_type {
                ChatMessageType::ConsensusRequest => {
                    consensus_request = Some(msg);
                    topic = msg.content.clone();
                    run_id = msg.run_id.clone();
                    if let Some(participants_list) = msg.metadata.get("participants").and_then(|v| v.as_array()) {
                        for p in participants_list {
                            if let Some(s) = p.as_str() {
                                participants.insert(s.to_string());
                            }
                        }
                    }
                }
                ChatMessageType::ConsensusResponse => {
                    if let Some(agrees) = msg.metadata.get("agrees").and_then(|v| v.as_bool()) {
                        votes.insert(msg.author.clone(), agrees);
                    }
                }
                ChatMessageType::Disagreement => {
                    if let Some(issue) = msg.metadata.get("issue").and_then(|v| v.as_str()) {
                        let suggestion = msg.metadata.get("suggestion")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        disagreements.push(DisagreementEntry {
                            participant: msg.author.clone(),
                            issue: issue.to_string(),
                            suggestion,
                            timestamp: msg.timestamp,
                        });
                    }
                }
                ChatMessageType::Clarification => {
                    if let Some(question) = msg.metadata.get("question").and_then(|v| v.as_str()) {
                        clarifications.push(ClarificationEntry {
                            question: question.to_string(),
                            answer: None,
                            questioner: msg.author.clone(),
                            responder: None,
                            timestamp: msg.timestamp,
                        });
                    }
                }
                ChatMessageType::Correction => {
                    if let Some(correction) = msg.metadata.get("correction").and_then(|v| v.as_str()) {
                        if let Some(clar) = clarifications.last_mut() {
                            if clar.answer.is_none() {
                                clar.answer = Some(correction.to_string());
                                clar.responder = Some(msg.author.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let status = if consensus_request.is_none() {
            ConsensusStatus::NotRequested
        } else if !clarifications.iter().any(|c| c.answer.is_none()) && !clarifications.is_empty() {
            ConsensusStatus::WaitingForClarification
        } else if !disagreements.is_empty() {
            ConsensusStatus::Disagreement
        } else {
            let total_participants = participants.len();
            let agree_count = votes.values().filter(|&&v| v).count();
            
            if agree_count == total_participants && total_participants > 0 {
                ConsensusStatus::Reached
            } else if agree_count > 0 {
                ConsensusStatus::Pending
            } else {
                ConsensusStatus::Pending
            }
        };

        let created_at = messages
            .first()
            .map(|m| m.timestamp)
            .unwrap_or_else(now_unix);
        let updated_at = messages
            .last()
            .map(|m| m.timestamp)
            .unwrap_or_else(now_unix);

        Ok(TaskConsensus {
            task_id,
            run_id,
            status,
            topic,
            participants: participants.into_iter().collect(),
            votes,
            disagreements,
            clarifications,
            resolution: None,
            created_at,
            updated_at,
        })
    }

    pub fn check_consensus(&self, task_id: String) -> Result<bool> {
        let state = self.get_consensus_state(task_id)?;
        Ok(state.status == ConsensusStatus::Reached)
    }

    pub fn has_disagreements(&self, task_id: String) -> Result<bool> {
        let state = self.get_consensus_state(task_id)?;
        Ok(state.status == ConsensusStatus::Disagreement)
    }

    pub fn needs_clarification(&self, task_id: String) -> Result<bool> {
        let state = self.get_consensus_state(task_id)?;
        Ok(state.status == ConsensusStatus::WaitingForClarification)
    }

    pub fn resolve_consensus(
        &self,
        task_id: String,
        resolution: String,
        resolved_by: String,
        lang: &str,
    ) -> Result<String> {
        let message_id = self.chat_manager.send_message(
            None,
            Some(task_id.clone()),
            resolved_by.clone(),
            "orchestrator".to_string(),
            ChatMessageType::Info,
            resolution.clone(),
            lang.to_string(),
            None,
            serde_json::json!({ "resolution": resolution }),
        )?;

        tracing::info!(
            "[Consensus] Consensus resolved for task {} by {}: {}",
            task_id,
            resolved_by,
            resolution
        );

        Ok(message_id)
    }

    pub fn get_pending_disagreements(&self, task_id: String) -> Result<Vec<DisagreementEntry>> {
        let state = self.get_consensus_state(task_id)?;
        Ok(state.disagreements)
    }

    pub fn get_pending_clarifications(&self, task_id: String) -> Result<Vec<ClarificationEntry>> {
        let state = self.get_consensus_state(task_id)?;
        Ok(state.clarifications)
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_initiate_consensus() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ConsensusManager::new(temp_dir.path());

        let proposal = ConsensusProposal {
            task_id: "task-1".to_string(),
            topic: "Should we proceed?".to_string(),
            description: "Testing consensus".to_string(),
            proposed_by: "agent-1".to_string(),
            participants: vec!["agent-1".to_string(), "agent-2".to_string()],
            timeout_seconds: 300,
        };

        let result = manager.initiate_consensus(proposal, "en");
        assert!(result.is_ok());
    }

    #[test]
    fn test_vote_and_check_consensus() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ConsensusManager::new(temp_dir.path());

        let proposal = ConsensusProposal {
            task_id: "task-2".to_string(),
            topic: "Approve plan?".to_string(),
            description: "Testing voting".to_string(),
            proposed_by: "agent-1".to_string(),
            participants: vec!["agent-1".to_string(), "agent-2".to_string()],
            timeout_seconds: 300,
        };

        let request_id = manager.initiate_consensus(proposal, "en").unwrap();

        let vote1 = manager.vote(
            "task-2".to_string(),
            None,
            "agent-1".to_string(),
            true,
            "Looks good".to_string(),
            request_id.clone(),
            "en",
        );

        let vote2 = manager.vote(
            "task-2".to_string(),
            None,
            "agent-2".to_string(),
            true,
            "Agreed".to_string(),
            request_id,
            "en",
        );

        assert!(vote1.is_ok());
        assert!(vote2.is_ok());

        let consensus_reached = manager.check_consensus("task-2".to_string()).unwrap();
        assert!(consensus_reached);
    }

    #[test]
    fn test_disagreement_reporting() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ConsensusManager::new(temp_dir.path());

        let result = manager.report_disagreement(
            "task-3".to_string(),
            None,
            "agent-1".to_string(),
            "Issue with approach".to_string(),
            Some("Try alternative method".to_string()),
            "en",
        );

        assert!(result.is_ok());

        let has_disagreements = manager.has_disagreements("task-3".to_string()).unwrap();
        assert!(has_disagreements);
    }

    #[test]
    fn test_clarification_flow() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ConsensusManager::new(temp_dir.path());

        let question_id = manager.request_clarification(
            "task-4".to_string(),
            None,
            "agent-1".to_string(),
            "What does this mean?".to_string(),
            "parent-msg-id".to_string(),
            "en",
        );

        assert!(question_id.is_ok());

        let answer_id = manager.provide_clarification(
            "task-4".to_string(),
            None,
            "orchestrator".to_string(),
            "It means X".to_string(),
            question_id.unwrap(),
            "en",
        );

        assert!(answer_id.is_ok());
    }
}
