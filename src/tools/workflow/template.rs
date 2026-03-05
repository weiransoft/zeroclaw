use std::fmt::Write;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 模板安全处理模块
/// 
/// 提供输入清理和转义功能，防止模板注入攻击
pub mod template_security {
    /// 最大允许的模板变量长度（防止内存耗尽攻击）
    pub const MAX_TEMPLATE_VAR_LENGTH: usize = 10_000;
    /// 最大允许的模板总长度
    pub const MAX_TEMPLATE_LENGTH: usize = 100_000;
    
    /// 清理输入字符串，移除危险字符
    /// 
    /// # 安全处理
    /// - 移除控制字符（除了换行和制表符）
    /// - 限制字符串长度
    /// - 转义 HTML 特殊字符
    pub fn sanitize_input(input: &str) -> String {
        // 首先限制长度
        let truncated = if input.len() > MAX_TEMPLATE_VAR_LENGTH {
            &input[..MAX_TEMPLATE_VAR_LENGTH]
        } else {
            input
        };
        
        // 移除危险字符并转义
        let mut result = String::with_capacity(truncated.len());
        for ch in truncated.chars() {
            match ch {
                // 移除控制字符（保留换行和制表符）
                c if c.is_control() && c != '\n' && c != '\t' => continue,
                // 转义 HTML 特殊字符
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '&' => result.push_str("&amp;"),
                '"' => result.push_str("&quot;"),
                '\'' => result.push_str("&#39;"),
                // 正常字符
                c => result.push(c),
            }
        }
        result
    }
    
    /// 清理模板变量名，只允许字母、数字、下划线
    pub fn sanitize_var_name(name: &str) -> String {
        name.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(50)  // 限制变量名长度
            .collect()
    }
    
    /// 验证模板字符串安全性
    /// 
    /// # 检查项
    /// - 长度限制
    /// - 禁止的模板语法
    pub fn validate_template(template: &str) -> Result<(), String> {
        if template.len() > MAX_TEMPLATE_LENGTH {
            return Err(format!(
                "模板长度 {} 超过最大限制 {}",
                template.len(), MAX_TEMPLATE_LENGTH
            ));
        }
        
        // 检查可能的危险模式
        let dangerous_patterns = [
            "{{#",      // 模板注释/块
            "{{~",      // 模板空白控制
            "<%",       // ERB 风格
            "<?",       // PHP 风格
            "${",       // Shell 风格
        ];
        
        for pattern in dangerous_patterns {
            if template.contains(pattern) {
                return Err(format!(
                    "模板包含潜在危险的语法: {}",
                    pattern
                ));
            }
        }
        
        Ok(())
    }
}

// 操作风险等级
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum OperationRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

// 确认状态
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ConfirmationStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

// 生成状态
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum GenerationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

// 角色定义（扩展版）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoleDefinition {
    /// 角色名称
    pub name: String,
    /// 角色描述
    pub description: String,
    /// 权限列表
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 职责列表
    #[serde(default)]
    pub responsibilities: Vec<String>,
    
    // ===== 扩展字段 =====
    /// 角色类型（用于 LLM 生成 prompt）
    #[serde(default)]
    pub role_type: Option<String>,
    /// 技能列表
    #[serde(default)]
    pub skills: Vec<String>,
    /// 产出物模板
    #[serde(default)]
    pub deliverables: Vec<DeliverableTemplate>,
    /// 协作角色
    #[serde(default)]
    pub collaborators: Vec<String>,
    /// 决策权限级别
    #[serde(default)]
    pub decision_authority: Option<String>,
    /// Prompt 模板（可自定义）
    #[serde(default)]
    pub prompt_template: Option<String>,
    /// 是否需要 LLM 生成 prompt
    #[serde(default)]
    pub generate_prompt: bool,
}

/// 产出物模板
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeliverableTemplate {
    /// 产出物名称
    pub name: String,
    /// 产出物描述
    pub description: String,
    /// 模板路径
    #[serde(default)]
    pub template_path: Option<String>,
    /// 是否必需
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool { true }

// RoleDefinition 实现
impl RoleDefinition {
    /// 生成角色 Prompt
    /// 
    /// 根据角色定义生成执行时的角色提示词
    /// 
    /// # 安全处理
    /// - 所有输入都会经过安全清理
    /// - 模板会进行安全性验证
    pub fn generate_prompt(&self, context: &RolePromptContext) -> String {
        // 如果有自定义模板，优先使用
        if let Some(template) = &self.prompt_template {
            // 验证模板安全性
            if let Err(e) = template_security::validate_template(template) {
                // 模板验证失败时，回退到默认 prompt
                eprintln!("模板验证失败: {}, 使用默认 prompt", e);
                return self.generate_default_prompt(context);
            }
            return self.render_template(template, context);
        }
        
        // 否则生成默认 prompt
        self.generate_default_prompt(context)
    }
    
    /// 渲染自定义模板
    /// 
    /// # 安全处理
    /// - 所有变量值都经过 sanitize_input 清理
    /// - 防止模板注入攻击
    fn render_template(&self, template: &str, context: &RolePromptContext) -> String {
        // 使用安全函数清理所有输入
        let safe_name = template_security::sanitize_input(&self.name);
        let safe_description = template_security::sanitize_input(&self.description);
        let safe_responsibilities = self.responsibilities
            .iter()
            .map(|r| template_security::sanitize_input(r))
            .collect::<Vec<_>>()
            .join("\n- ");
        let safe_skills = self.skills
            .iter()
            .map(|s| template_security::sanitize_input(s))
            .collect::<Vec<_>>()
            .join("\n- ");
        let safe_project_type = template_security::sanitize_input(&context.project_type);
        let safe_current_phase = template_security::sanitize_input(&context.current_phase);
        let safe_task_description = template_security::sanitize_input(&context.task_description);
        
        // 执行模板替换
        template
            .replace("{{name}}", &safe_name)
            .replace("{{description}}", &safe_description)
            .replace("{{responsibilities}}", &safe_responsibilities)
            .replace("{{skills}}", &safe_skills)
            .replace("{{project_type}}", &safe_project_type)
            .replace("{{current_phase}}", &safe_current_phase)
            .replace("{{task_description}}", &safe_task_description)
    }
    
    /// 生成默认 prompt
    /// 
    /// # 性能优化
    /// - 预估字符串容量，减少重新分配
    /// - 使用安全输入清理
    /// - 使用 write! 宏避免临时字符串分配
    fn generate_default_prompt(&self, context: &RolePromptContext) -> String {
        // 预估容量：基础信息 + 职责 + 技能 + 产出物 + 协作
        let estimated_capacity = 500 
            + self.responsibilities.len() * 50 
            + self.skills.len() * 50 
            + self.deliverables.len() * 100 
            + self.collaborators.len() * 30;
        let mut prompt = String::with_capacity(estimated_capacity);
        
        // 角色概述（使用安全清理）
        let _ = write!(prompt, "# {} - 角色定义\n\n", template_security::sanitize_input(&self.name));
        let _ = write!(prompt, "{}\n\n", template_security::sanitize_input(&self.description));
        
        // 核心职责
        if !self.responsibilities.is_empty() {
            prompt.push_str("## 核心职责\n");
            for resp in &self.responsibilities {
                let _ = write!(prompt, "- {}\n", template_security::sanitize_input(resp));
            }
            prompt.push('\n');
        }
        
        // 专业技能
        if !self.skills.is_empty() {
            prompt.push_str("## 专业技能\n");
            for skill in &self.skills {
                let _ = write!(prompt, "- {}\n", template_security::sanitize_input(skill));
            }
            prompt.push('\n');
        }
        
        // 产出标准
        if !self.deliverables.is_empty() {
            prompt.push_str("## 产出物\n");
            for deliverable in &self.deliverables {
                let required = if deliverable.required { "（必需）" } else { "（可选）" };
                let _ = write!(prompt, "- {} {}\n", 
                    template_security::sanitize_input(&deliverable.name), required);
                if !deliverable.description.is_empty() {
                    let _ = write!(prompt, "  {}\n", 
                        template_security::sanitize_input(&deliverable.description));
                }
            }
            prompt.push('\n');
        }
        
        // 协作关系
        if !self.collaborators.is_empty() {
            prompt.push_str("## 协作角色\n");
            let safe_collaborators: Vec<String> = self.collaborators
                .iter()
                .map(|c| template_security::sanitize_input(c))
                .collect();
            let _ = write!(prompt, "需要与以下角色协作：{}\n\n", safe_collaborators.join(", "));
        }
        
        // 决策权限
        if let Some(authority) = &self.decision_authority {
            prompt.push_str("## 决策权限\n");
            let _ = write!(prompt, "{}\n\n", template_security::sanitize_input(authority));
        }
        
        // 上下文信息
        if !context.project_type.is_empty() || !context.current_phase.is_empty() {
            prompt.push_str("## 当前上下文\n");
            if !context.project_type.is_empty() {
                let _ = write!(prompt, "- 项目类型：{}\n", 
                    template_security::sanitize_input(&context.project_type));
            }
            if !context.current_phase.is_empty() {
                let _ = write!(prompt, "- 当前阶段：{}\n", 
                    template_security::sanitize_input(&context.current_phase));
            }
            prompt.push('\n');
        }
        
        prompt
    }
}

/// 角色 Prompt 上下文
#[derive(Debug, Clone, Default)]
pub struct RolePromptContext {
    /// 项目类型
    pub project_type: String,
    /// 当前阶段
    pub current_phase: String,
    /// 任务描述
    pub task_description: String,
}

// 工作流阶段
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowPhase {
    pub id: String,
    pub name: String,
    pub description: String,
    pub duration_days: Option<u32>,
    pub activity_ids: Vec<String>,
    pub required_roles: Vec<String>,
    pub exit_conditions: Vec<Condition>,
}

// 条件
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Condition {
    pub type_: String,
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

// 活动输入
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActivityInput {
    pub name: String,
    pub type_: String,
    pub required: bool,
    pub description: String,
}

// 活动输出（扩展版）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActivityOutput {
    /// 输出名称
    pub name: String,
    /// 输出类型
    #[serde(rename = "type")]
    pub type_: String,
    /// 输出描述
    #[serde(default)]
    pub description: String,
    
    // ===== 扩展字段 =====
    /// 文档模板路径
    #[serde(default)]
    pub template_path: Option<String>,
    /// 是否必需
    #[serde(default = "default_true")]
    pub required: bool,
    /// 验收标准
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
}

// 活动定义
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActivityDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub assigned_roles: Vec<String>,
    pub duration_minutes: Option<u32>,
    pub deliverables: Vec<String>,
    pub dependencies: Vec<String>,
    pub inputs: Vec<ActivityInput>,
    pub outputs: Vec<ActivityOutput>,
    pub risk_level: OperationRiskLevel,
    pub skill_references: Vec<SkillReference>,
}

// 事件模板
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventTemplate {
    pub id: String,
    pub name: String,
    pub type_: String,
    pub description: String,
    pub trigger: String,
    pub actions: Vec<EventAction>,
}

// 事件动作
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventAction {
    pub type_: String,
    pub target: String,
    pub parameters: serde_json::Value,
}

// 集成定义
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IntegrationDefinition {
    pub id: String,
    pub name: String,
    pub type_: String,
    pub config: serde_json::Value,
    pub triggers: Vec<String>,
    pub actions: Vec<String>,
}

// Skill 引用
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillReference {
    pub id: String,
    pub name: String,
    pub version: String,
    pub parameters: serde_json::Value,
    pub execution_context: ExecutionContext,
}

// 执行上下文
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExecutionContext {
    pub role: String,
    pub phase: String,
    pub activity: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

// 风险规则
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiskRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub condition: Condition,
    pub risk_level: OperationRiskLevel,
    pub required_approval: bool,
}

// 模板变量
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateVariable {
    pub name: String,
    pub description: String,
    pub type_: String,
    pub default_value: serde_json::Value,
    pub required: bool,
}

// 工作流模板
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub categories: Vec<String>,
    pub roles: Vec<RoleDefinition>,
    pub phases: Vec<WorkflowPhase>,
    pub activities: Vec<ActivityDefinition>,
    pub events: Vec<EventTemplate>,
    pub integrations: Vec<IntegrationDefinition>,
    pub skills: Vec<SkillReference>,
    pub risk_rules: Vec<RiskRule>,
    pub variables: Vec<TemplateVariable>,
    pub applicable_scenarios: Vec<String>,
}

// 高危操作确认请求
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiskyOperationConfirmation {
    pub id: String,
    pub operation: String,
    pub risk_level: OperationRiskLevel,
    pub description: String,
    pub parameters: serde_json::Value,
    pub requested_by: String,
    pub requested_at: String,
    pub status: ConfirmationStatus,
}

// 模板生成请求
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateGenerationRequest {
    pub id: String,
    pub prompt: String,
    pub parameters: serde_json::Value,
    pub requested_by: String,
    pub requested_at: String,
    pub status: GenerationStatus,
}

// 模板验证结果
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateValidationResult {
    pub is_valid: bool,
    pub error_message: Option<String>,
    pub warnings: Vec<String>,
}

// 工作流模板实现
impl WorkflowTemplate {
    // 创建新模板
    pub fn new(name: String, description: String, author: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            version: "1.0.0".to_string(),
            author,
            categories: Vec::new(),
            roles: Vec::new(),
            phases: Vec::new(),
            activities: Vec::new(),
            events: Vec::new(),
            integrations: Vec::new(),
            skills: Vec::new(),
            risk_rules: Vec::new(),
            variables: Vec::new(),
            applicable_scenarios: Vec::new(),
        }
    }
    
    // 验证模板
    pub fn validate(&self) -> TemplateValidationResult {
        let mut warnings = Vec::new();
        
        // 验证基本信息
        if self.name.trim().is_empty() {
            return TemplateValidationResult {
                is_valid: false,
                error_message: Some("Template name cannot be empty".to_string()),
                warnings: Vec::new(),
            };
        }
        
        if self.description.trim().is_empty() {
            warnings.push("Template description is empty".to_string());
        }
        
        // 验证角色
        if self.roles.is_empty() {
            warnings.push("No roles defined".to_string());
        }
        
        // 验证阶段
        if self.phases.is_empty() {
            warnings.push("No phases defined".to_string());
        } else {
            // 验证阶段的活动引用
            for phase in &self.phases {
                for activity_id in &phase.activity_ids {
                    if !self.activities.iter().any(|a| a.id == *activity_id) {
                        return TemplateValidationResult {
                            is_valid: false,
                            error_message: Some(format!("Phase {} references non-existent activity {}", phase.id, activity_id)),
                            warnings: Vec::new(),
                        };
                    }
                }
            }
        }
        
        // 验证活动
        for activity in &self.activities {
            // 验证活动依赖
            for dependency in &activity.dependencies {
                if !self.activities.iter().any(|a| a.id == *dependency) {
                    return TemplateValidationResult {
                        is_valid: false,
                        error_message: Some(format!("Activity {} references non-existent dependency {}", activity.id, dependency)),
                        warnings: Vec::new(),
                    };
                }
            }
            
            // 验证技能引用
            for skill_ref in &activity.skill_references {
                if !self.skills.iter().any(|s| s.id == skill_ref.id) {
                    warnings.push(format!("Activity {} references skill {} not defined in template", activity.id, skill_ref.id));
                }
            }
        }
        
        TemplateValidationResult {
            is_valid: true,
            error_message: None,
            warnings,
        }
    }
}
