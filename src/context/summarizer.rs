//! LLM-enhanced context summarizer
//! 
//! This module provides intelligent context summarization using LLM:
//! - Global context summarization
//! - Task context summarization
//! - Incremental summary updates
//! - Summary quality validation

use super::llm_client::{LLMClient, Result};
use std::sync::Arc;

/// Abstraction level for summaries
#[derive(Debug, Clone)]
pub enum AbstractionLevel {
    /// High-level overview (brief)
    Overview,
    /// Medium detail (balanced)
    Balanced,
    /// Detailed summary (comprehensive)
    Detailed,
}

/// Focus areas for summarization
#[derive(Debug, Clone)]
pub enum FocusArea {
    UserProfile,
    DomainKnowledge,
    HistoricalExperience,
    CollaborationNetwork,
    CapabilityModel,
}

/// LLM-enhanced context summarizer
pub struct ContextSummarizer {
    llm_client: Arc<dyn LLMClient>,
    max_tokens: usize,
    abstraction_level: AbstractionLevel,
}

impl ContextSummarizer {
    /// Create a new summarizer
    pub fn new(
        llm_client: Arc<dyn LLMClient>,
        max_tokens: usize,
        abstraction_level: AbstractionLevel,
    ) -> Self {
        Self {
            llm_client,
            max_tokens,
            abstraction_level,
        }
    }
    
    /// Generate summary for global context
    pub async fn summarize_global_context(
        &self,
        user_profile: &str,
        domain_knowledge: &str,
        historical_experience: &str,
        focus_areas: &[FocusArea],
    ) -> Result<String> {
        let prompt = self.build_summary_prompt(
            user_profile,
            domain_knowledge,
            historical_experience,
            focus_areas,
        );
        
        let response = self.llm_client.generate(&prompt).await?;
        let refined = self.refine_summary(&response.content).await?;
        
        Ok(refined)
    }
    
    /// Generate summary for task context
    pub async fn summarize_task_context(
        &self,
        task_id: &str,
        task_type: &str,
        task_status: &str,
        conversation_history: &str,
        intermediate_results: &str,
        decisions: &str,
        include_decisions: bool,
    ) -> Result<String> {
        let prompt = format!(
            r#"请为以下任务上下文生成简洁摘要，突出关键信息{}:

任务 ID: {}
任务类型：{}
任务状态：{}

对话历史:
{}

中间结果:
{}

决策和结论:
{}

请生成一个不超过 200 字的摘要，重点描述:
1. 任务的核心目标和关键发现
2. 重要决策及其原因 (如果包含决策)
3. 需要传递给全局上下文的经验教训

摘要应该简洁明了，便于后续快速理解。"#,
            if include_decisions { "，特别是关键决策" } else { "" },
            task_id,
            task_type,
            task_status,
            if conversation_history.is_empty() { "无" } else { conversation_history },
            if intermediate_results.is_empty() { "无" } else { intermediate_results },
            if decisions.is_empty() { "无" } else { decisions },
        );
        
        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content)
    }
    
    /// Incremental summary update
    pub async fn update_summary_incrementally(
        &self,
        existing_summary: &str,
        new_information: &str,
    ) -> Result<String> {
        let prompt = format!(
            r#"已有摘要:
{}

新信息:
{}

请更新摘要，融合新信息，保持简洁性和连贯性。
保留关键信息，删除冗余内容，确保摘要不超过 300 字。
如果新信息与已有摘要冲突，请以新信息为准。"#,
            existing_summary,
            new_information
        );
        
        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content)
    }
    
    /// Extract key decisions from conversation
    pub async fn extract_decisions(
        &self,
        conversation_history: &str,
    ) -> Result<String> {
        let prompt = format!(
            r#"请从以下对话历史中提取所有关键决策:

{}

请列出:
1. 每个决策的内容
2. 决策的原因
3. 决策的影响

只提取明确的决策，不要包含讨论过程。"#,
            conversation_history
        );
        
        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content)
    }
    
    /// Extract lessons learned from task
    pub async fn extract_lessons_learned(
        &self,
        task_summary: &str,
        task_outcome: &str,
    ) -> Result<String> {
        let prompt = format!(
            r#"请从以下任务中提取经验教训:

任务摘要:
{}

任务结果:
{}

请总结:
1. 成功的经验和做法
2. 遇到的问题和解决方案
3. 对未来类似任务的建议

经验教训应该具体、可操作、可复用。"#,
            task_summary,
            task_outcome
        );
        
        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content)
    }
    
    fn build_summary_prompt(
        &self,
        user_profile: &str,
        domain_knowledge: &str,
        historical_experience: &str,
        focus_areas: &[FocusArea],
    ) -> String {
        let focus_str = format!("{:?}", focus_areas);
        
        format!(
            r#"请为以下全局上下文生成结构化摘要，重点关注{:?}:

用户画像:
{}

领域知识:
{}

历史经验:
{}

请生成一个层次化的摘要，包括:
1. 核心用户特征和偏好 (50 字以内)
2. 关键领域知识和技能 (100 字以内)
3. 重要历史经验和教训 (100 字以内)

摘要应该:
- 突出关键信息，删除冗余细节
- 使用清晰的结构和简洁的语言
- 便于后续任务快速理解和应用
- 保持专业性和准确性"#,
            focus_str,
            user_profile,
            domain_knowledge,
            historical_experience,
        )
    }
    
    async fn refine_summary(&self, summary: &str) -> Result<String> {
        let validation_prompt = format!(
            r#"请评估以下摘要的质量:

{}

评估标准:
1. 信息完整性：是否包含所有关键信息
2. 简洁性：是否删除了冗余内容
3. 可读性：是否结构清晰、语言流畅
4. 实用性：是否便于后续应用

如果摘要存在问题，请提供改进版本。
如果摘要质量良好，请返回"摘要质量良好，无需改进"。"#,
            summary
        );
        
        let response = self.llm_client.generate(&validation_prompt).await?;
        
        if response.content.contains("无需改进") {
            Ok(summary.to_string())
        } else {
            Ok(response.content)
        }
    }
}

/// Helper functions for formatting context data
impl ContextSummarizer {
    /// Format user profile for summarization
    pub fn format_user_profile(&self, profile: &serde_json::Value) -> String {
        serde_json::to_string_pretty(profile).unwrap_or_default()
    }
    
    /// Format domain knowledge for summarization
    pub fn format_domain_knowledge(&self, knowledge: &serde_json::Value) -> String {
        serde_json::to_string_pretty(knowledge).unwrap_or_default()
    }
    
    /// Format historical experience for summarization
    pub fn format_historical_experience(&self, experience: &serde_json::Value) -> String {
        serde_json::to_string_pretty(experience).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::llm_client::MockLLMClient;
    
    #[tokio::test]
    async fn test_summarize_task_context() {
        let mock_response = r#"任务摘要:
1. 核心目标：实现用户认证功能
2. 关键发现：使用 JWT 令牌方案
3. 重要决策：选择 Redis 存储会话
4. 经验教训：提前规划令牌刷新机制"#;
        
        let llm_client = Arc::new(MockLLMClient::with_response(mock_response.to_string()));
        let summarizer = ContextSummarizer::new(llm_client, 2000, AbstractionLevel::Balanced);
        
        let summary = summarizer
            .summarize_task_context(
                "task-001",
                "Technical",
                "Completed",
                "用户询问认证方案",
                "实现了 JWT 认证",
                "决定使用 Redis 存储",
                true,
            )
            .await
            .unwrap();
        
        assert!(!summary.is_empty());
        assert!(summary.contains("认证") || summary.contains("JWT") || summary.contains("Redis"));
    }
    
    #[tokio::test]
    async fn test_extract_decisions() {
        let mock_response = r#"关键决策:
1. 使用 JWT 令牌进行认证
   原因：无状态、可扩展
   影响：需要实现令牌刷新机制
2. 选择 Redis 存储会话
   原因：高性能、支持过期
   影响：需要部署 Redis 服务"#;
        
        let llm_client = Arc::new(MockLLMClient::with_response(mock_response.to_string()));
        let summarizer = ContextSummarizer::new(llm_client, 2000, AbstractionLevel::Balanced);
        
        let decisions = summarizer
            .extract_decisions("讨论认证方案的对话历史...")
            .await
            .unwrap();
        
        assert!(!decisions.is_empty());
        assert!(decisions.contains("JWT") || decisions.contains("Redis"));
    }
    
    #[tokio::test]
    async fn test_extract_lessons_learned() {
        let mock_response = r#"经验教训:
1. 成功经验：提前设计 API 接口
2. 问题解决：遇到性能问题，通过缓存优化
3. 建议：在开发前进行技术预研"#;
        
        let llm_client = Arc::new(MockLLMClient::with_response(mock_response.to_string()));
        let summarizer = ContextSummarizer::new(llm_client, 2000, AbstractionLevel::Balanced);
        
        let lessons = summarizer
            .extract_lessons_learned("实现认证功能", "成功上线，性能良好")
            .await
            .unwrap();
        
        assert!(!lessons.is_empty());
        assert!(lessons.contains("经验") || lessons.contains("建议"));
    }
    
    #[tokio::test]
    async fn test_incremental_update() {
        let mock_response = r#"更新后的摘要:
用户偏好使用 Python 进行开发，最近开始学习 Rust。
在 Web 开发方面有丰富经验，正在探索系统编程。
技术栈包括 Django、FastAPI，现在尝试使用 Actix-web。"#;
        
        let llm_client = Arc::new(MockLLMClient::with_response(mock_response.to_string()));
        let summarizer = ContextSummarizer::new(llm_client, 2000, AbstractionLevel::Balanced);
        
        let updated = summarizer
            .update_summary_incrementally(
                "用户偏好使用 Python 进行开发，有 Web 开发经验。",
                "用户最近开始学习 Rust，对系统编程感兴趣。",
            )
            .await
            .unwrap();
        
        assert!(!updated.is_empty());
        assert!(updated.contains("Python") && updated.contains("Rust"));
    }
}
