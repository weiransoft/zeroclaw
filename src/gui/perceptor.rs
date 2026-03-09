/// GUI Agent 多模态感知器模块（临时占位）
/// 
/// 本模块提供多模态感知功能，包括 UI 元素识别、屏幕理解等。
/// 注意：这是临时占位实现，用于支持编译

use std::result;

/// 感知器结果类型
pub type Result<T> = result::Result<T, MultimodalPerceptorError>;

/// 感知器错误类型
#[derive(Debug)]
pub enum MultimodalPerceptorError {
    /// LLM 推理失败
    LlmInferenceFailed(String),
    /// 通用错误消息
    Message(String),
}

impl std::fmt::Display for MultimodalPerceptorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultimodalPerceptorError::LlmInferenceFailed(msg) => {
                write!(f, "LLM 推理失败：{}", msg)
            }
            MultimodalPerceptorError::Message(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for MultimodalPerceptorError {}

/// UI 元素类型
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum UiElementType {
    Button,
    Input,
    Label,
    Checkbox,
    Radio,
    Select,
    Table,
    Image,
    Link,
    Other,
}

/// UI 元素
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UiElement {
    pub id: String,
    pub element_type: UiElementType,
    pub text: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// 屏幕理解（占位）
pub struct ScreenUnderstanding;

impl ScreenUnderstanding {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ScreenUnderstanding {
    fn default() -> Self {
        Self::new()
    }
}

/// 上下文管理器（占位）
pub struct ContextManager;

impl ContextManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 多模态感知器（占位）
pub struct MultimodalPerceptor;

impl MultimodalPerceptor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MultimodalPerceptor {
    fn default() -> Self {
        Self::new()
    }
}
