//! 上下文构建器模块
//! 
//! 基于任务依赖关系构建精简上下文，利用现有的记忆、知识、经验系统

use std::{
    collections::HashMap,
    fmt::Write,
};
use serde::{Deserialize, Serialize};

// 引用现有模块中的类型
use super::agent_task::{AgentTask, DependencyGraph};

/// Token 预算配置的安全边界常量
pub mod token_limits {
    /// 最小允许的 max_tokens 值
    pub const MIN_TOKENS: usize = 100;
    /// 最大允许的 max_tokens 值（防止内存耗尽）
    pub const MAX_TOKENS: usize = 100_000;
    /// 比例值的最小边界
    pub const RATIO_MIN: f64 = 0.0;
    /// 比例值的最大边界
    pub const RATIO_MAX: f64 = 1.0;
    /// 比例值允许的误差范围
    pub const RATIO_EPSILON: f64 = 0.001;
}

/// Token 预算配置
/// 
/// 安全约束：
/// - max_tokens 必须在 [MIN_TOKENS, MAX_TOKENS] 范围内
/// - 所有比例值必须在 [0.0, 1.0] 范围内
/// - 比例总和应接近 1.0（允许误差范围内）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudgetConfig {
    /// 最大 token 数（安全范围：100-100000）
    pub max_tokens: usize,
    /// 依赖产出物分配比例
    pub dependency_output_ratio: f64,
    /// 记忆分配比例
    pub memory_ratio: f64,
    /// 知识分配比例
    pub knowledge_ratio: f64,
    /// 经验分配比例
    pub experience_ratio: f64,
}

impl Default for TokenBudgetConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4000,
            dependency_output_ratio: 0.4,
            memory_ratio: 0.2,
            knowledge_ratio: 0.2,
            experience_ratio: 0.2,
        }
    }
}

/// 验证比例值的宏，避免重复代码
macro_rules! validate_ratio {
    ($name:expr, $value:expr) => {
        if !(token_limits::RATIO_MIN..=token_limits::RATIO_MAX).contains(&$value) {
            return Err(format!(
                "{} ({}) 超出有效范围 [0.0, 1.0]",
                $name, $value
            ));
        }
    };
}

impl TokenBudgetConfig {
    /// 内部验证方法，提取公共验证逻辑
    fn validate_internal(
        max_tokens: usize,
        dependency_output_ratio: f64,
        memory_ratio: f64,
        knowledge_ratio: f64,
        experience_ratio: f64,
    ) -> Result<(), String> {
        // 验证 max_tokens 范围
        if !(token_limits::MIN_TOKENS..=token_limits::MAX_TOKENS).contains(&max_tokens) {
            return Err(format!(
                "max_tokens ({}) 超出有效范围 [{}, {}]",
                max_tokens, token_limits::MIN_TOKENS, token_limits::MAX_TOKENS
            ));
        }

        // 验证比例值范围
        validate_ratio!("dependency_output_ratio", dependency_output_ratio);
        validate_ratio!("memory_ratio", memory_ratio);
        validate_ratio!("knowledge_ratio", knowledge_ratio);
        validate_ratio!("experience_ratio", experience_ratio);

        // 验证比例总和接近 1.0
        let total_ratio = dependency_output_ratio + memory_ratio + knowledge_ratio + experience_ratio;
        if (total_ratio - 1.0).abs() > token_limits::RATIO_EPSILON {
            return Err(format!(
                "比例总和 ({}) 不等于 1.0，请调整比例配置",
                total_ratio
            ));
        }

        Ok(())
    }

    /// 创建新的 TokenBudgetConfig 并验证参数
    /// 
    /// # 安全验证
    /// - max_tokens 必须在有效范围内
    /// - 比例值必须在 [0.0, 1.0] 范围内
    /// - 比例总和应接近 1.0
    /// 
    /// # 返回
    /// - Ok(TokenBudgetConfig) 验证通过
    /// - Err(String) 验证失败，返回错误信息
    pub fn new(
        max_tokens: usize,
        dependency_output_ratio: f64,
        memory_ratio: f64,
        knowledge_ratio: f64,
        experience_ratio: f64,
    ) -> Result<Self, String> {
        Self::validate_internal(
            max_tokens,
            dependency_output_ratio,
            memory_ratio,
            knowledge_ratio,
            experience_ratio,
        )?;

        Ok(Self {
            max_tokens,
            dependency_output_ratio,
            memory_ratio,
            knowledge_ratio,
            experience_ratio,
        })
    }

    /// 验证现有配置是否有效
    pub fn validate(&self) -> Result<(), String> {
        Self::validate_internal(
            self.max_tokens,
            self.dependency_output_ratio,
            self.memory_ratio,
            self.knowledge_ratio,
            self.experience_ratio,
        )
    }

    /// 安全地获取预算值，确保不会溢出
    /// 
    /// # 安全处理
    /// - 确保比例值在有效范围内
    /// - 使用 saturating 操作防止溢出
    /// 
    /// # 算法说明
    /// 使用整数运算避免浮点数精度问题：
    /// budget = max_tokens * ratio
    /// 转换为：budget = (max_tokens * ratio * 1000) / 1000
    pub fn safe_budget(&self, ratio: f64) -> usize {
        // 限制比例在有效范围内
        let safe_ratio = ratio.clamp(0.0, 1.0);
        
        // 使用整数运算避免浮点数精度问题
        // 将比例转换为千分比进行计算
        let ratio_millis = (safe_ratio * 1000.0).round() as usize;
        let budget = self.max_tokens.saturating_mul(ratio_millis).saturating_div(1000);
        
        budget.min(self.max_tokens)
    }
}

/// 任务上下文
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskContext {
    /// 依赖任务的产出物引用
    pub dependency_outputs: Vec<DeliverableRef>,
    /// 相关记忆引用
    pub memories: Vec<MemoryRef>,
    /// 相关知识引用
    pub knowledge: Vec<KnowledgeRef>,
    /// 相关经验引用
    pub experiences: Vec<ExperienceRef>,
    /// 当前 token 使用量
    pub current_tokens: usize,
}

/// 产出物引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverableRef {
    /// 来源任务ID
    pub task_id: String,
    /// 产出物名称
    pub name: String,
    /// 产出物路径
    pub path: String,
    /// 产出物摘要
    pub summary: String,
}

/// 记忆引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRef {
    /// 记忆ID
    pub id: String,
    /// 记忆摘要
    pub summary: String,
    /// 重要性分数
    pub importance: f64,
    /// 记忆类型
    pub memory_type: String,
}

/// 知识引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRef {
    /// 知识ID
    pub id: String,
    /// 知识标题
    pub title: String,
    /// 知识摘要
    pub summary: String,
    /// 相关性分数
    pub relevance: f64,
}

/// 经验引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceRef {
    /// 经验ID
    pub id: String,
    /// 经验标题
    pub title: String,
    /// 经验教训摘要
    pub lessons_summary: String,
    /// 置信度
    pub confidence: f64,
}

/// 上下文构建器
/// 
/// 基于任务依赖关系构建精简上下文
/// 
/// 设计理念：
/// - 利用现有 AgentTask 和 DependencyGraph 实现故事机制
/// - 任务的 dependencies 定义了上下文边界
/// - 通过记忆/知识/经验共享机制构建精简上下文
/// 
/// # 安全特性
/// - 输入边界检查，防止内存耗尽
/// - Token 预算限制，防止上下文过大
/// - 迭代次数限制，防止无限循环
pub struct ContextBuilder {
    /// Token 预算配置
    token_budget: TokenBudgetConfig,
    // 注：记忆、知识、经验系统通过可选的 Arc 引用
    // 实际使用时由外部注入
}

/// 上下文构建的安全边界常量
pub mod context_limits {
    /// 最大依赖任务数量限制
    pub const MAX_DEPENDENCIES: usize = 50;
    /// 最大产出物数量限制
    pub const MAX_DELIVERABLES: usize = 100;
    /// 最大记忆引用数量
    pub const MAX_MEMORIES: usize = 10;
    /// 最大知识引用数量
    pub const MAX_KNOWLEDGE: usize = 10;
    /// 最大经验引用数量
    pub const MAX_EXPERIENCES: usize = 10;
    /// 最大 prompt 长度（字符数）
    pub const MAX_PROMPT_LENGTH: usize = 50_000;
}

impl ContextBuilder {
    /// 创建新的上下文构建器
    /// 
    /// # 安全验证
    /// 会验证 token_budget 配置是否有效
    /// 
    /// # 返回
    /// - Ok(ContextBuilder) 验证通过
    /// - Err(String) 验证失败
    pub fn new(token_budget: TokenBudgetConfig) -> Result<Self, String> {
        token_budget.validate()?;
        Ok(Self { token_budget })
    }

    /// 安全地创建上下文构建器
    /// 
    /// 如果配置验证失败，则使用默认配置
    /// 
    /// # 注意
    /// 此方法会静默降级，建议在生产环境使用 `new()` 方法
    pub fn new_safe(token_budget: TokenBudgetConfig) -> Self {
        match token_budget.validate() {
            Ok(_) => Self { token_budget },
            Err(e) => {
                eprintln!("TokenBudgetConfig 验证失败: {}, 使用默认配置", e);
                Self {
                    token_budget: TokenBudgetConfig::default(),
                }
            }
        }
    }

    /// 从 JSON Value 中安全获取字符串值
    /// 
    /// # 参数
    /// - `val`: JSON 值
    /// - `key`: 要获取的键名
    /// 
    /// # 返回
    /// - 字符串值，如果不存在则返回空字符串
    fn get_str_or_empty<'a>(val: &'a serde_json::Value, key: &str) -> &'a str {
        val.get(key).and_then(|v| v.as_str()).unwrap_or("")
    }

    /// 为任务构建上下文（简化版，不依赖外部系统）
    /// 
    /// 根据任务依赖关系，聚合相关上下文
    /// 
    /// # 安全特性
    /// - 限制迭代次数，防止无限循环
    /// - 限制数据大小，防止内存耗尽
    /// - Token 预算限制，防止上下文过大
    pub fn build_context(
        &self,
        task: &AgentTask,
        dependency_graph: &DependencyGraph,
        completed_tasks: &HashMap<String, AgentTask>,
    ) -> TaskContext {
        let dep_ids = dependency_graph.get_dependencies(&task.id);
        
        // 预分配容量，避免动态扩容
        let mut context = TaskContext {
            dependency_outputs: Vec::with_capacity(
                dep_ids.len().min(context_limits::MAX_DEPENDENCIES)
            ),
            memories: Vec::with_capacity(context_limits::MAX_MEMORIES),
            knowledge: Vec::with_capacity(context_limits::MAX_KNOWLEDGE),
            experiences: Vec::with_capacity(context_limits::MAX_EXPERIENCES),
            current_tokens: 0,
        };
        
        let mut current_tokens = 0usize;

        // 1. 获取依赖任务的产出物（最高优先级）
        let dep_budget = self.token_budget.safe_budget(self.token_budget.dependency_output_ratio);
        
        // 限制依赖任务数量
        for dep_id in dep_ids.iter().take(context_limits::MAX_DEPENDENCIES) {
            if current_tokens >= dep_budget {
                break;
            }
            
            if let Some(dep_task) = completed_tasks.get(dep_id) {
                // 从 metadata 中获取产出物
                if let Some(deliverables) = dep_task.metadata.get("deliverables").and_then(|d| d.as_array()) {
                    // 限制产出物数量
                    for deliverable in deliverables.iter().take(context_limits::MAX_DELIVERABLES) {
                        if current_tokens >= dep_budget {
                            break;
                        }
                        
                        let name = Self::get_str_or_empty(deliverable, "name");
                        let path = Self::get_str_or_empty(deliverable, "path");
                        
                        // 安全截断过长的字符串
                        let safe_name = Self::truncate(name, 200);
                        let safe_path = Self::truncate(path, 500);
                        
                        // 更精确的 token 估算（约4字符1token）
                        current_tokens += (safe_name.len() + safe_path.len()) / 4;
                        
                        context.dependency_outputs.push(DeliverableRef {
                            task_id: dep_id.clone(),
                            name: safe_name,
                            path: safe_path,
                            summary: String::new(),
                        });
                    }
                }
            }
        }

        // 2. 从任务 metadata 中获取上下文引用
        if let Some(context_refs) = task.metadata.get("context_refs") {
            // 记忆引用（限制数量）
            if let Some(memory_ids) = context_refs.get("memory_ids").and_then(|m| m.as_array()) {
                for mem_id in memory_ids.iter().take(context_limits::MAX_MEMORIES) {
                    if let Some(mem_id_str) = mem_id.as_str() {
                        let safe_id = Self::truncate(mem_id_str, 100);
                        context.memories.push(MemoryRef {
                            id: safe_id,
                            summary: String::new(),
                            importance: 0.5,
                            memory_type: "Unknown".to_string(),
                        });
                    }
                }
            }
            
            // 知识引用（限制数量）
            if let Some(knowledge_ids) = context_refs.get("knowledge_ids").and_then(|k| k.as_array()) {
                for k_id in knowledge_ids.iter().take(context_limits::MAX_KNOWLEDGE) {
                    if let Some(k_id_str) = k_id.as_str() {
                        let safe_id = Self::truncate(k_id_str, 100);
                        context.knowledge.push(KnowledgeRef {
                            id: safe_id,
                            title: String::new(),
                            summary: String::new(),
                            relevance: 0.5,
                        });
                    }
                }
            }
            
            // 经验引用（限制数量）
            if let Some(experience_ids) = context_refs.get("experience_ids").and_then(|e| e.as_array()) {
                for exp_id in experience_ids.iter().take(context_limits::MAX_EXPERIENCES) {
                    if let Some(exp_id_str) = exp_id.as_str() {
                        let safe_id = Self::truncate(exp_id_str, 100);
                        context.experiences.push(ExperienceRef {
                            id: safe_id,
                            title: String::new(),
                            lessons_summary: String::new(),
                            confidence: 0.5,
                        });
                    }
                }
            }
        }

        context.current_tokens = current_tokens;
        context
    }

    /// 构建执行时的精简 Prompt
    /// 
    /// 根据任务和上下文生成执行 prompt
    /// 
    /// # 安全特性
    /// - 预估字符串容量，减少内存分配
    /// - 限制 prompt 总长度
    /// - 安全截断过长的输入
    /// 
    /// # 注意
    /// 写入 String 不会失败，但为了类型安全，使用 unwrap() 显式处理
    #[allow(clippy::unwrap_used)]
    pub fn build_execution_prompt(
        &self,
        task: &AgentTask,
        context: &TaskContext,
        role_prompt: &str,
    ) -> String {
        // 预估容量：角色 + 任务 + 验收标准 + 依赖 + 记忆 + 知识 + 经验
        let estimated_capacity = role_prompt.len() 
            + task.title.len() + task.description.len() 
            + 500  // 基础开销
            + context.dependency_outputs.len() * 100
            + context.memories.len() * 100
            + context.knowledge.len() * 100
            + context.experiences.len() * 100;
        
        // 限制最大容量
        let safe_capacity = estimated_capacity.min(context_limits::MAX_PROMPT_LENGTH);
        let mut prompt = String::with_capacity(safe_capacity);

        // 角色定义（安全截断）
        let safe_role = Self::truncate(role_prompt, 5000);
        write!(prompt, "# 角色定义\n\n{}\n\n", safe_role).unwrap();

        // 任务描述（安全截断）
        let safe_title = Self::truncate(&task.title, 200);
        let safe_desc = Self::truncate(&task.description, 2000);
        write!(prompt, "# 当前任务\n\n## {}\n{}\n\n", safe_title, safe_desc).unwrap();

        // 验收标准
        if let Some(criteria) = task.metadata.get("acceptance_criteria").and_then(|c| c.as_array()) {
            prompt.push_str("## 验收标准\n");
            for c in criteria.iter().take(20) {  // 限制验收标准数量
                if let Some(c_str) = c.as_str() {
                    let safe_c = Self::truncate(c_str, 500);
                    write!(prompt, "- {}\n", safe_c).unwrap();
                }
            }
            prompt.push('\n');
        }

        // 依赖产出物
        if !context.dependency_outputs.is_empty() {
            prompt.push_str("## 前置产出物\n");
            for d in context.dependency_outputs.iter().take(20) {  // 限制数量
                let safe_name = Self::truncate(&d.name, 100);
                let safe_path = Self::truncate(&d.path, 200);
                write!(prompt, "- {} ({})\n", safe_name, safe_path).unwrap();
            }
            prompt.push('\n');
        }

        // 相关记忆摘要
        if !context.memories.is_empty() {
            prompt.push_str("## 相关记忆\n");
            for m in context.memories.iter().take(10) {
                if !m.summary.is_empty() {
                    let safe_summary = Self::truncate(&m.summary, 200);
                    write!(prompt, "- {} (重要性: {:.2})\n", safe_summary, m.importance).unwrap();
                }
            }
            prompt.push('\n');
        }

        // 相关知识摘要
        if !context.knowledge.is_empty() {
            prompt.push_str("## 相关知识\n");
            for k in context.knowledge.iter().take(10) {
                if !k.title.is_empty() {
                    let safe_title = Self::truncate(&k.title, 200);
                    write!(prompt, "- {} (相关性: {:.2})\n", safe_title, k.relevance).unwrap();
                }
            }
            prompt.push('\n');
        }

        // 相关经验摘要
        if !context.experiences.is_empty() {
            prompt.push_str("## 相关经验\n");
            for e in context.experiences.iter().take(10) {
                if !e.title.is_empty() {
                    let safe_title = Self::truncate(&e.title, 200);
                    write!(prompt, "- {} (置信度: {:.2})\n", safe_title, e.confidence).unwrap();
                }
            }
            prompt.push('\n');
        }

        // 执行指引
        prompt.push_str("## 执行指引\n");
        prompt.push_str("请根据以上上下文完成任务。如需更多详细信息，可使用相关工具查询。\n");

        // 最终安全检查：如果 prompt 过长，截断
        if prompt.len() > context_limits::MAX_PROMPT_LENGTH {
            prompt = Self::truncate(&prompt, context_limits::MAX_PROMPT_LENGTH);
        }

        prompt
    }

    /// 截断字符串到指定长度
    /// 
    /// 正确处理 UTF-8 字符边界，避免截断多字节字符
    /// 
    /// # 安全特性
    /// - UTF-8 安全截断
    /// - 单次遍历 O(n) 复杂度
    /// - 添加省略号表示截断
    /// 
    /// # 性能优化
    /// - 预分配容量避免重新分配
    /// - 单次遍历，避免多次 count()
    pub fn truncate(s: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return String::new();
        }
        
        let s_char_count = s.chars().count();
        
        // 如果字符串长度不超过限制，直接返回原字符串
        if s_char_count <= max_chars {
            return s.to_string();
        }
        
        // 计算实际截断位置（预留省略号空间）
        let real_max = if max_chars <= 3 {
            max_chars
        } else {
            max_chars.saturating_sub(3)
        };
        
        // 预分配容量（假设平均每个字符4字节UTF-8）
        let mut result = String::with_capacity(max_chars * 4);
        let mut char_count = 0;
        
        for c in s.chars() {
            if char_count >= real_max {
                break;
            }
            result.push(c);
            char_count += 1;
        }
        
        // 添加省略号（因为已经确定字符串被截断）
        result.push_str("...");
        
        result
    }
}

impl TaskContext {
    /// 检查上下文是否为空
    pub fn is_empty(&self) -> bool {
        self.dependency_outputs.is_empty()
            && self.memories.is_empty()
            && self.knowledge.is_empty()
            && self.experiences.is_empty()
    }

    /// 获取上下文条目总数
    pub fn total_entries(&self) -> usize {
        self.dependency_outputs.len()
            + self.memories.len()
            + self.knowledge.len()
            + self.experiences.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::agent_task::AgentTaskStatus;
    use crate::swarm::TaskPriority;
    use crate::swarm::agent_task::TaskSource;

    fn create_test_task(id: &str, title: &str, description: &str) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            status: AgentTaskStatus::Pending,
            priority: TaskPriority::Medium,
            parent_task_id: None,
            subtasks: vec![],
            dependencies: vec![],
            created_at: 0,
            updated_at: 0,
            estimated_completion: None,
            completed_at: None,
            source: TaskSource::SelfCreated,
            metadata: serde_json::json!({
                "acceptance_criteria": ["完成功能", "测试通过"]
            }),
        }
    }

    #[test]
    fn test_token_budget_config_default() {
        let config = TokenBudgetConfig::default();
        assert_eq!(config.max_tokens, 4000);
        assert!((config.dependency_output_ratio - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_token_budget_config_validation() {
        // 测试有效的配置
        let valid = TokenBudgetConfig::new(4000, 0.4, 0.2, 0.2, 0.2);
        assert!(valid.is_ok());

        // 测试 max_tokens 太小
        let too_small = TokenBudgetConfig::new(50, 0.4, 0.2, 0.2, 0.2);
        assert!(too_small.is_err());

        // 测试 max_tokens 太大
        let too_large = TokenBudgetConfig::new(200_000, 0.4, 0.2, 0.2, 0.2);
        assert!(too_large.is_err());

        // 测试比例超出范围
        let ratio_out_of_range = TokenBudgetConfig::new(4000, 1.5, -0.5, 0.0, 0.0);
        assert!(ratio_out_of_range.is_err());

        // 测试比例总和不等于 1.0
        let ratio_sum_wrong = TokenBudgetConfig::new(4000, 0.5, 0.5, 0.5, 0.5);
        assert!(ratio_sum_wrong.is_err());
    }

    #[test]
    fn test_safe_budget_calculation() {
        let config = TokenBudgetConfig::default();
        
        // 测试正常比例
        let budget = config.safe_budget(0.5);
        assert_eq!(budget, 2000); // 4000 * 0.5 = 2000

        // 测试超出范围的比例（应被 clamp）
        let budget_over = config.safe_budget(1.5);
        assert_eq!(budget_over, 4000); // 被 clamp 到 1.0

        let budget_under = config.safe_budget(-0.5);
        assert_eq!(budget_under, 0); // 被 clamp 到 0.0
    }

    #[test]
    fn test_task_context_is_empty() {
        let context = TaskContext::default();
        assert!(context.is_empty());
    }

    #[test]
    fn test_task_context_total_entries() {
        let mut context = TaskContext::default();
        assert_eq!(context.total_entries(), 0);
        
        context.memories.push(MemoryRef {
            id: "test".to_string(),
            summary: "test".to_string(),
            importance: 0.5,
            memory_type: "Decision".to_string(),
        });
        assert_eq!(context.total_entries(), 1);
    }

    #[test]
    fn test_truncate() {
        // 测试正常截断
        let s = "这是一个很长的字符串用于测试截断功能";
        let truncated = ContextBuilder::truncate(s, 10);
        assert!(truncated.chars().count() <= 10);
        assert!(truncated.ends_with("..."));
        
        // 测试短字符串不截断
        let short = "短字符串";
        let not_truncated = ContextBuilder::truncate(short, 10);
        assert_eq!(not_truncated, short);
        assert!(!not_truncated.ends_with("..."));
        
        // 测试零长度
        let zero = ContextBuilder::truncate("test", 0);
        assert!(zero.is_empty());
        
        // 测试小于3的长度
        let small = ContextBuilder::truncate("test", 2);
        assert_eq!(small.chars().count(), 5); // "te..." = 5 chars
        
        // 测试空字符串
        let empty = ContextBuilder::truncate("", 10);
        assert!(empty.is_empty());
        assert!(!empty.ends_with("..."));
        
        // 测试刚好等于限制长度
        let exact = ContextBuilder::truncate("1234567890", 10);
        assert_eq!(exact, "1234567890");
        assert!(!exact.ends_with("..."));
    }

    #[test]
    fn test_context_builder_new() {
        // 测试有效配置
        let valid_config = TokenBudgetConfig::default();
        let builder = ContextBuilder::new(valid_config);
        assert!(builder.is_ok());

        // 测试无效配置
        let invalid_config = TokenBudgetConfig {
            max_tokens: 50, // 太小
            ..Default::default()
        };
        let builder_invalid = ContextBuilder::new(invalid_config);
        assert!(builder_invalid.is_err());
    }

    #[test]
    fn test_context_builder_new_safe() {
        // 测试有效配置
        let valid_config = TokenBudgetConfig::default();
        let _builder = ContextBuilder::new_safe(valid_config);
        // 正常创建，不会 panic

        // 测试无效配置（应使用默认值）
        let invalid_config = TokenBudgetConfig {
            max_tokens: 50, // 太小
            ..Default::default()
        };
        let builder_safe = ContextBuilder::new_safe(invalid_config);
        // 应该成功创建，使用默认配置
        let default_config = TokenBudgetConfig::default();
        assert_eq!(builder_safe.token_budget.max_tokens, default_config.max_tokens);
    }

    #[test]
    fn test_build_context() {
        let task = create_test_task("task-1", "测试任务", "这是一个测试任务描述");
        let graph = DependencyGraph::default();
        let completed_tasks = HashMap::new();
        
        let builder = ContextBuilder::new(TokenBudgetConfig::default()).unwrap();
        let context = builder.build_context(&task, &graph, &completed_tasks);
        
        assert!(context.is_empty());
    }

    #[test]
    fn test_build_execution_prompt() {
        let task = create_test_task("task-1", "测试任务", "这是一个测试任务描述");
        let context = TaskContext::default();
        let builder = ContextBuilder::new(TokenBudgetConfig::default()).unwrap();
        let prompt = builder.build_execution_prompt(&task, &context, "你是一个开发工程师");

        assert!(prompt.contains("角色定义"));
        assert!(prompt.contains("测试任务"));
        assert!(prompt.contains("验收标准"));
        assert!(prompt.contains("执行指引"));
    }

    #[test]
    fn test_build_execution_prompt_with_context() {
        let task = create_test_task("task-1", "测试任务", "这是一个测试任务描述");
        let mut context = TaskContext::default();
        
        context.dependency_outputs.push(DeliverableRef {
            task_id: "prev-task".to_string(),
            name: "PRD文档".to_string(),
            path: "docs/prd.md".to_string(),
            summary: "产品需求文档".to_string(),
        });
        
        context.memories.push(MemoryRef {
            id: "mem-1".to_string(),
            summary: "之前做出的技术决策".to_string(),
            importance: 0.8,
            memory_type: "Decision".to_string(),
        });

        let builder = ContextBuilder::new(TokenBudgetConfig::default()).unwrap();
        let prompt = builder.build_execution_prompt(&task, &context, "你是一个开发工程师");

        assert!(prompt.contains("前置产出物"));
        assert!(prompt.contains("PRD文档"));
        assert!(prompt.contains("相关记忆"));
        assert!(prompt.contains("技术决策"));
    }

    #[test]
    fn test_get_str_or_empty() {
        let json = serde_json::json!({
            "name": "test",
            "empty": "",
            "null": null
        });
        
        assert_eq!(ContextBuilder::get_str_or_empty(&json, "name"), "test");
        assert_eq!(ContextBuilder::get_str_or_empty(&json, "empty"), "");
        assert_eq!(ContextBuilder::get_str_or_empty(&json, "null"), "");
        assert_eq!(ContextBuilder::get_str_or_empty(&json, "missing"), "");
    }
}
