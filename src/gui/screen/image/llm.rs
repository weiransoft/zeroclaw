/// LLM 客户端 - 使用 ZeroClaw 已有的 Provider 架构
/// 
/// 本模块提供与多模态 LLM（Qwen3.5 0.8B/1.8B）的集成，支持：
/// - 屏幕语义理解
/// - UI 元素识别
/// - 自然语言查询
/// 
/// # 支持的推理框架
/// 
/// - **Ollama**: 本地推理，支持 qwen3.5:0.8b 和 qwen3.5:1.8b
/// - **OpenRouter**: 云端推理，支持 Qwen3.5 系列
/// - **其他 OpenAI 兼容提供商**: 支持多模态的提供商
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::providers::{Provider, ChatMessage};
/// use zeroclaw::providers::ollama::OllamaProvider;
/// 
/// // 使用 Ollama Provider
/// let provider = OllamaProvider::new("http://localhost:11434");
/// 
/// // 进行屏幕理解
/// let elements = understand_screen(&provider, "qwen3.5:0.8b", &screen_image).await?;
/// ```

use std::result;
use base64::{Engine, engine::GeneralPurpose, engine::general_purpose::PAD};
use crate::gui::perceptor::{UiElement, Result as PerceptorResult, MultimodalPerceptorError};
use crate::providers::Provider;

/// LLM 客户端结果类型
pub type Result<T> = result::Result<T, LlmClientError>;

/// LLM 客户端错误类型
#[derive(Debug)]
pub enum LlmClientError {
    /// LLM 推理失败
    InferenceFailed(String),
    /// 响应解析失败
    ResponseParseFailed(String),
    /// 模型不支持多模态
    MultimodalNotSupported(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for LlmClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmClientError::InferenceFailed(msg) => write!(f, "LLM 推理失败：{}", msg),
            LlmClientError::ResponseParseFailed(msg) => write!(f, "响应解析失败：{}", msg),
            LlmClientError::MultimodalNotSupported(msg) => write!(f, "模型不支持多模态：{}", msg),
            LlmClientError::Other(msg) => write!(f, "其他错误：{}", msg),
        }
    }
}

impl std::error::Error for LlmClientError {}

/// 理解屏幕内容
/// 
/// 使用多模态 LLM 对屏幕截图进行语义理解，识别 UI 元素和文本区域。
/// 
/// # 参数
/// 
/// * `provider` - LLM Provider 实例
/// * `model` - 模型名称（如 "qwen3.5:0.8b", "qwen3.5:1.8b"）
/// * `image` - 屏幕截图数据（PNG 或 JPEG 格式）
/// 
/// # 返回
/// 
/// * `Result<Vec<UiElement>>` - 识别的 UI 元素列表
/// 
/// # 错误
/// 
/// * `LlmClientError::InferenceFailed` - LLM 推理失败
/// * `LlmClientError::ResponseParseFailed` - 响应解析失败
/// * `LlmClientError::MultimodalNotSupported` - 模型不支持多模态
/// 
/// # 示例
/// 
/// ```rust
/// use zeroclaw::providers::Provider;
/// use zeroclaw::gui::screen::image::llm::understand_screen;
/// 
/// let elements = understand_screen(&provider, "qwen3.5:0.8b", &screen_image).await?;
/// for element in elements {
///     println!("元素：{:?} - {}", element.element_type, element.description);
/// }
/// ```
pub async fn understand_screen<P: Provider>(
    provider: &P,
    model: &str,
    image: &[u8],
) -> PerceptorResult<Vec<UiElement>> {
    // 将图像转换为 Base64 编码
    let base64_image = GeneralPurpose::new(&base64::alphabet::STANDARD, PAD).encode(image);
    
    // 构建系统提示词
    let system_prompt = r#"你是一个专业的 GUI 界面分析助手。你的任务是分析屏幕截图，识别所有 UI 元素。
    
请识别以下类型的 UI 元素：
- button: 按钮
- input: 输入框
- label: 文本标签
- image: 图片
- link: 链接
- checkbox: 复选框
- radio: 单选框
- dropdown: 下拉框
- table: 表格
- list: 列表

对于每个元素，请提供：
1. 元素类型（element_type）
2. 自然语言描述（description）
3. 边界框坐标 [x, y, width, height]（bounding_box）
4. 元素中的文本内容（text，如果有）
5. 置信度（confidence，0.0-1.0）
6. 是否可交互（interactive）

请以 JSON 数组格式返回结果，每个元素包含以下字段：
{
  "id": "唯一 ID",
  "element_type": "类型",
  "description": "描述",
  "bounding_box": [x, y, width, height],
  "text": "文本内容或 null",
  "confidence": 0.95,
  "interactive": true,
  "state": "状态（可选）"
}"#;

    // 构建用户消息（包含 Base64 图像）
    // 注意：不同的 Provider 对多模态的支持方式不同
    // 对于 Ollama 和其他 OpenAI 兼容的 Provider，需要在消息中包含图像
    let user_message = format!(
        r#"请分析这张屏幕截图，识别所有 UI 元素。确保识别准确，边界框精确。

图像数据（Base64）:
data:image/png;base64,{}

请以 JSON 数组格式返回识别结果。"#,
        base64_image
    );
    
    // 调用 LLM Provider
    let response = provider
        .chat_with_system(
            Some(system_prompt),
            &user_message,
            model,
            0.1, // 低温度，确保输出稳定
        )
        .await
        .map_err(|e| {
            MultimodalPerceptorError::LlmInferenceFailed(
                format!("LLM 推理失败：{}", e)
            )
        })?;
    
    // 提取响应文本
    let content = response.text_or_empty();
    
    if content.is_empty() {
        return Err(MultimodalPerceptorError::LlmInferenceFailed(
            "LLM 响应为空".to_string()
        ));
    }
    
    // 解析 UI 元素
    let elements = parse_ui_elements(content)?;
    
    Ok(elements)
}

/// 查找 UI 元素
/// 
/// 使用自然语言描述查找特定的 UI 元素。
/// 
/// # 参数
/// 
/// * `provider` - LLM Provider 实例
/// * `model` - 模型名称
/// * `image` - 屏幕截图数据
/// * `description` - 元素描述（自然语言，如 "提交按钮"、"用户名输入框"）
/// 
/// # 返回
/// 
/// * `Result<Option<UiElement>>` - 找到的 UI 元素
/// 
/// # 示例
/// 
/// ```rust
/// use zeroclaw::providers::Provider;
/// use zeroclaw::gui::screen::image::llm::find_ui_element;
/// 
/// let button = find_ui_element(&provider, "qwen3.5:0.8b", &screen_image, "提交按钮").await?;
/// if let Some(element) = button {
///     println!("找到元素：{:?}", element);
/// }
/// ```
pub async fn find_ui_element<P: Provider>(
    provider: &P,
    model: &str,
    image: &[u8],
    description: &str,
) -> PerceptorResult<Option<UiElement>> {
    let base64_image = GeneralPurpose::new(&base64::alphabet::STANDARD, PAD).encode(image);
    
    let system_prompt = r#"你是一个专业的 GUI 界面分析助手。你的任务是根据用户的描述，在屏幕截图中找到对应的 UI 元素。

请精确识别用户描述的元素，并提供其详细信息。如果找不到匹配的元素，请返回空数组。"#;

    let user_message = format!(
        r#"请在屏幕截图中找到：{}。如果找到，请返回该元素的详细信息；如果未找到，请返回空数组。

图像数据（Base64）:
data:image/png;base64,{}

请以 JSON 数组格式返回结果（0 个或 1 个元素）。"#,
        description,
        base64_image
    );
    
    let response = provider
        .chat_with_system(
            Some(system_prompt),
            &user_message,
            model,
            0.1,
        )
        .await
        .map_err(|e| {
            MultimodalPerceptorError::LlmInferenceFailed(
                format!("LLM 推理失败：{}", e)
            )
        })?;
    
    let content = response.text_or_empty();
    
    if content.is_empty() {
        return Ok(None);
    }
    
    let elements = parse_ui_elements(content)?;
    
    Ok(elements.into_iter().next())
}

/// 解析 LLM 响应中的 UI 元素
fn parse_ui_elements(content: &str) -> PerceptorResult<Vec<UiElement>> {
    // 尝试从响应中提取 JSON
    let json_start = content.find('[').unwrap_or(0);
    let json_end = content.rfind(']').map(|i| i + 1).unwrap_or(content.len());
    let json_str = &content[json_start..json_end];
    
    // 解析 JSON 数组
    let elements: Vec<UiElement> = serde_json::from_str(json_str)
        .map_err(|e| {
            MultimodalPerceptorError::LlmInferenceFailed(
                format!("解析 UI 元素失败：{}", e)
            )
        })?;
    
    Ok(elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encoding() {
        let image_data = b"fake image data";
        let base64_image = GeneralPurpose::new(&base64::alphabet::STANDARD, PAD).encode(image_data);
        assert!(!base64_image.is_empty());
    }

    #[test]
    fn test_parse_ui_elements_empty() {
        let content = "[]";
        let elements = parse_ui_elements(content).unwrap();
        assert!(elements.is_empty());
    }

    #[test]
    fn test_parse_ui_elements_with_data() {
        let content = r#"[{
            "id": "btn-1",
            "element_type": "button",
            "description": "提交按钮",
            "bounding_box": [100, 200, 80, 40],
            "text": "提交",
            "confidence": 0.95,
            "interactive": true
        }]"#;
        
        let elements = parse_ui_elements(content).unwrap();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].id, "btn-1");
        assert_eq!(elements[0].description, "提交按钮");
    }
}
