use anyhow::Result;
use super::template::WorkflowTemplate;
use crate::swarm::SwarmManager;

/// 工作流模板生成器
/// 
/// 核心能力：
/// - 通过 LLM 语义理解生成工作流模板
/// - 动态构建分析 prompt，避免硬编码
/// - 生成风险评估标准 prompt
/// - 支持模板优化和变体生成
pub struct WorkflowTemplateGenerator {
    manager: std::sync::Arc<SwarmManager>,
}

impl WorkflowTemplateGenerator {
    /// 创建新的模板生成器
    pub fn new(manager: std::sync::Arc<SwarmManager>) -> Self {
        Self {
            manager,
        }
    }
    
    /// 从提示词生成模板 - 现在需要提供provider和model
    pub async fn generate_template_from_prompt(
        &self,
        prompt: String,
        parameters: serde_json::Value,
        _requester: String,
        provider: &dyn crate::providers::Provider,
        model: &str,
    ) -> Result<WorkflowTemplate> {
        // 构建增强提示词
        let enhanced_prompt = self.build_enhanced_prompt(prompt, parameters)?;
        
        // 使用提供的provider获取LLM响应
        let llm_response = self.call_llm(&enhanced_prompt, provider, model).await?;
        
        // 解析 LLM 响应
        self.parse_llm_template_response(&llm_response)
    }
    
    /// 调用LLM的实际实现
    async fn call_llm(&self, prompt: &str, provider: &dyn crate::providers::Provider, model: &str) -> Result<String> {
        // 使用提供的provider来执行聊天请求
        let response = provider.chat(prompt, model, 0.7).await?;
        
        Ok(response.text_or_empty().to_string())
    }
    
    /// 构建增强提示词
    fn build_enhanced_prompt(&self, prompt: String, _parameters: serde_json::Value) -> Result<String> {
        let base_prompt = r#"
请根据以下描述生成一个完整的工作流模板 JSON：

{prompt}

生成的模板应包含：
1. 基本信息（id、name、description、version、author 等）
2. 推荐的团队角色及其职责和权限
3. 工作阶段划分及每个阶段的主要活动
4. 各阶段和活动的时间估算
5. 关键交付物
6. 适用的技能（skills）
7. 必要的集成配置
8. 模板变量定义
9. 风险规则定义

请确保生成的 JSON 格式正确，并且包含所有必要的字段。
JSON 应该直接输出，不要包含任何其他文本。
"#;
        
        let enhanced_prompt = base_prompt.replace("{prompt}", &prompt);
        Ok(enhanced_prompt)
    }
    
    /// 构建风险评估标准 prompt
    /// 通过语义理解进行风险评估，避免硬编码规则
    /// 
    /// # 参数
    /// - `workflow_description`: 工作流描述
    /// - `context`: 工作流上下文
    /// 
    /// # 返回
    /// - `String`: 风险评估 prompt
    pub fn build_risk_assessment_prompt(&self, workflow_description: &str, context: &str) -> String {
        let mut prompt = String::new();
        
        prompt.push_str("请分析以下工作流的风险，提供详细的风险评估：\n\n");
        
        prompt.push_str("## 工作流描述\n");
        prompt.push_str(workflow_description);
        prompt.push('\n');
        
        if !context.is_empty() {
            prompt.push_str("\n## 工作流上下文\n");
            prompt.push_str(context);
            prompt.push('\n');
        }
        
        prompt.push_str("\n## 风险评估要求\n");
        prompt.push_str("请从以下几个维度进行语义理解分析：\n");
        prompt.push_str("1. 技术风险：分析技术实现的难度和不确定性（新技术、新框架、技术债务等）\n");
        prompt.push_str("2. 依赖风险：分析外部依赖的稳定性和可用性（第三方服务、API、数据源等）\n");
        prompt.push_str("3. 时间风险：分析时间估算的准确性和缓冲空间（截止日期、里程碑、资源限制等）\n");
        prompt.push_str("4. 资源风险：分析团队资源的充足性和能力匹配（人员技能、团队规模、经验匹配等）\n");
        prompt.push_str("5. 需求风险：分析需求的明确性和变更可能性（需求模糊、频繁变更、范围蔓延等）\n");
        prompt.push_str("6. 质量风险：分析质量保证的充分性和测试覆盖度（测试策略、自动化程度、质量标准等）\n");
        
        prompt.push_str("\n## 风险等级定义\n");
        prompt.push_str("HIGH（高）：风险发生的可能性大，且影响严重，需要立即采取措施\n");
        prompt.push_str("MEDIUM（中）：风险发生的可能性中等，影响中等，需要关注和预防\n");
        prompt.push_str("LOW（低）：风险发生的可能性小，影响轻微，可以接受或监控\n");
        
        prompt.push_str("\n## 输出格式\n");
        prompt.push_str("请以 JSON 格式返回风险评估结果：\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"risks\": [\n");
        prompt.push_str("    {\n");
        prompt.push_str("      \"risk\": \"风险描述\",\n");
        prompt.push_str("      \"category\": \"风险类别（技术/依赖/时间/资源/需求/质量）\",\n");
        prompt.push_str("      \"probability\": \"高/中/低\",\n");
        prompt.push_str("      \"impact\": \"高/中/低\",\n");
        prompt.push_str("      \"mitigation\": \"缓解措施\",\n");
        prompt.push_str("      \"trigger\": \"风险触发条件\"\n");
        prompt.push_str("    }\n");
        prompt.push_str("  ],\n");
        prompt.push_str("  \"overall_risk_level\": \"整体风险等级（高/中/低）\",\n");
        prompt.push_str("  \"risk_score\": 0,\n");
        prompt.push_str("  \"mitigation_strategies\": [\"整体缓解策略列表\"],\n");
        prompt.push_str("  \"monitoring_recommendations\": [\"监控建议列表\"],\n");
        prompt.push_str("  \"analysis_notes\": \"分析说明\"\n");
        prompt.push_str("}\n");
        
        prompt.push_str("\n## 注意事项\n");
        prompt.push_str("- 请基于工作流描述进行语义理解，不要假设不存在的信息\n");
        prompt.push_str("- 风险评估应综合考虑多个维度\n");
        prompt.push_str("- 缓解措施应具体可行，具有可操作性\n");
        prompt.push_str("- 如果信息不足，请标注为\"待补充\"并说明需要哪些信息\n");
        
        prompt
    }
    
    /// 构建代码分析 prompt
    /// 通过语义理解进行代码分析，避免硬编码正则表达式
    /// 
    /// # 参数
    /// - `code`: 代码内容
    /// - `context`: 代码上下文
    /// 
    /// # 返回
    /// - `String`: 代码分析 prompt
    pub fn build_code_analysis_prompt(&self, code: &str, context: &str) -> String {
        let mut prompt = String::new();
        
        prompt.push_str("请分析以下代码，提供详细的语义理解分析：\n\n");
        
        prompt.push_str("## 代码内容\n");
        prompt.push_str(code);
        prompt.push('\n');
        
        if !context.is_empty() {
            prompt.push_str("\n## 代码上下文\n");
            prompt.push_str(context);
            prompt.push('\n');
        }
        
        prompt.push_str("\n## 代码分析要求\n");
        prompt.push_str("请从以下几个维度进行语义理解分析：\n");
        prompt.push_str("1. 代码功能：分析代码的主要功能和业务逻辑（功能描述、业务流程、核心算法等）\n");
        prompt.push_str("2. 代码结构：分析代码的结构和组织方式（模块划分、类设计、函数组织等）\n");
        prompt.push_str("3. 代码质量：分析代码的质量和可维护性（代码规范、可读性、可测试性等）\n");
        prompt.push_str("4. 性能特征：分析代码的性能特征和优化空间（时间复杂度、空间复杂度、性能瓶颈等）\n");
        prompt.push_str("5. 安全性：分析代码的安全性和潜在风险（注入风险、权限控制、数据安全等）\n");
        prompt.push_str("6. 依赖关系：分析代码的依赖关系和耦合度（外部依赖、内部依赖、模块解耦等）\n");
        prompt.push_str("7. 扩展性：分析代码的扩展性和可配置性（扩展点、配置项、插件机制等）\n");
        
        prompt.push_str("\n## 输出格式\n");
        prompt.push_str("请以 JSON 格式返回分析结果：\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"functionality\": {\n");
        prompt.push_str("    \"primary_function\": \"主要功能\",\n");
        prompt.push_str("    \"business_logic\": \"业务逻辑描述\",\n");
        prompt.push_str("    \"core_algorithms\": [\"核心算法列表\"]\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"structure\": {\n");
        prompt.push_str("    \"module_organization\": \"模块组织方式\",\n");
        prompt.push_str("    \"class_design\": \"类设计描述\",\n");
        prompt.push_str("    \"function_organization\": \"函数组织方式\"\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"quality\": {\n");
        prompt.push_str("    \"code_quality_score\": 0.0,\n");
        prompt.push_str("    \"readability\": 0.0,\n");
        prompt.push_str("    \"maintainability\": 0.0,\n");
        prompt.push_str("    \"testability\": 0.0,\n");
        prompt.push_str("    \"issues\": [\"问题列表\"]\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"performance\": {\n");
        prompt.push_str("    \"time_complexity\": \"时间复杂度（如 O(n)）\",\n");
        prompt.push_str("    \"space_complexity\": \"空间复杂度（如 O(1)）\",\n");
        prompt.push_str("    \"bottlenecks\": [\"性能瓶颈列表\"],\n");
        prompt.push_str("    \"optimization_suggestions\": [\"优化建议列表\"]\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"security\": {\n");
        prompt.push_str("    \"security_score\": 0.0,\n");
        prompt.push_str("    \"injection_risks\": [\"注入风险列表\"],\n");
        prompt.push_str("    \"access_control_issues\": [\"权限控制问题列表\"],\n");
        prompt.push_str("    \"data_security_concerns\": [\"数据安全问题列表\"],\n");
        prompt.push_str("    \"security_recommendations\": [\"安全建议列表\"]\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"dependencies\": {\n");
        prompt.push_str("    \"external_dependencies\": [\"外部依赖列表\"],\n");
        prompt.push_str("    \"internal_dependencies\": [\"内部依赖列表\"],\n");
        prompt.push_str("    \"coupling_degree\": \"耦合度（高/中/低）\",\n");
        prompt.push_str("    \"decoupling_suggestions\": [\"解耦建议列表\"]\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"extensibility\": {\n");
        prompt.push_str("    \"extension_points\": [\"扩展点列表\"],\n");
        prompt.push_str("    \"configurability\": 0.0,\n");
        prompt.push_str("    \"plugin_support\": false,\n");
        prompt.push_str("    \"extensibility_suggestions\": [\"扩展性建议列表\"]\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"analysis_notes\": \"分析说明\"\n");
        prompt.push_str("}\n");
        
        prompt.push_str("\n## 注意事项\n");
        prompt.push_str("- 请基于代码内容进行语义理解，不要假设不存在的信息\n");
        prompt.push_str("- 代码分析应综合考虑多个维度\n");
        prompt.push_str("- 建议应具体可行，具有可操作性\n");
        prompt.push_str("- 如果信息不足，请标注为\"待补充\"并说明需要哪些信息\n");
        
        prompt
    }
    
    /// 解析 LLM 响应
    fn parse_llm_template_response(&self, response: &str) -> Result<WorkflowTemplate> {
        // 提取 JSON 部分
        let json_start = response.find('{');
        let json_end = response.rfind('}');
        
        if let (Some(start), Some(end)) = (json_start, json_end) {
            let json_str = &response[start..=end];
            
            // 解析 JSON
            match serde_json::from_str::<WorkflowTemplate>(json_str) {
                Ok(template) => {
                    // 验证模板
                    let validation = template.validate();
                    if validation.is_valid {
                        Ok(template)
                    } else {
                        Err(anyhow::anyhow!("Generated template is invalid: {:?}", validation.error_message))
                    }
                }
                Err(e) => {
                    Err(anyhow::anyhow!("Failed to parse generated template: {:?}", e))
                }
            }
        } else {
            Err(anyhow::anyhow!("No JSON found in LLM response"))
        }
    }
    
    /// 优化生成的模板
    pub fn optimize_template(&self, template: &mut WorkflowTemplate) -> Result<()> {
        // 确保所有活动都有唯一 ID
        for (index, activity) in template.activities.iter_mut().enumerate() {
            if activity.id.is_empty() {
                activity.id = format!("activity-{}", index + 1);
            }
        }
        
        // 确保所有阶段都有唯一 ID
        for (index, phase) in template.phases.iter_mut().enumerate() {
            if phase.id.is_empty() {
                phase.id = format!("phase-{}", index + 1);
            }
        }
        
        // 确保所有角色都有描述
        for role in &mut template.roles {
            if role.description.is_empty() {
                role.description = format!("{} role", role.name);
            }
        }
        
        // 验证优化后的模板
        let validation = template.validate();
        if validation.is_valid {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Optimized template is invalid: {:?}", validation.error_message))
        }
    }
    
    /// 生成模板变体
    pub async fn generate_template_variant(
        &self,
        base_template: &WorkflowTemplate,
        variation_prompt: String,
        provider: &dyn crate::providers::Provider,
        model: &str,
    ) -> Result<WorkflowTemplate> {
        // 构建变体提示词
        let variant_prompt = format!(
            r#"
基于以下工作流模板，生成一个变体，满足以下要求：

{variation_prompt}

基础模板：
{base_template}
"#,
            variation_prompt = variation_prompt,
            base_template = serde_json::to_string(base_template)?
        );
        
        // 使用提供的provider获取LLM响应
        let llm_response = self.call_llm(&variant_prompt, provider, model).await?;
        
        // 解析 LLM 响应
        self.parse_llm_template_response(&llm_response)
    }
}
