/// LLM 图像识别模块
/// 
/// 本模块提供基于 LLM 的图像识别功能,作为 Tesseract OCR 的辅助。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::image::llm::LlmClient;
/// 
/// // 创建 LLM 客户端
/// let client = LlmClient::new("your-api-key");
/// 
/// // 进行图像识别
/// // let image = load_image("path/to/image.png");
/// // let text = client.ocr_image(&image).unwrap();
/// // println!("识别文本: {}", text);
/// ```

use std::result;
use serde::{Deserialize, Serialize};
use reqwest::Client;

/// LLM 客户端结果类型
pub type Result<T> = result::Result<T, LlmClientError>;

/// LLM 客户端错误类型
#[derive(Debug)]
pub enum LlmClientError {
    /// API 请求失败
    ApiRequestFailed(String),
    /// 响应解析失败
    ResponseParseFailed(String),
    /// 认证失败
    AuthenticationFailed(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for LlmClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmClientError::ApiRequestFailed(msg) => write!(f, "API 请求失败: {}", msg),
            LlmClientError::ResponseParseFailed(msg) => write!(f, "响应解析失败: {}", msg),
            LlmClientError::AuthenticationFailed(msg) => write!(f, "认证失败: {}", msg),
            LlmClientError::Other(msg) => write!(f, "其他错误: {}", msg),
        }
    }
}

impl std::error::Error for LlmClientError {}

/// LLM 客户端
/// 
/// 提供基于 LLM 的图像识别功能。
/// 
/// # 使用示例
/// 
/// ```rust
/// let client = LlmClient::new("your-api-key");
/// ```

pub struct LlmClient {
    /// API 密钥
    api_key: String,
    /// API 端点
    endpoint: String,
    /// 模型名称
    model: String,
    /// HTTP 客户端
    client: Client,
}

impl LlmClient {
    /// 创建新的 LLM 客户端实例
    /// 
    /// # 参数
    /// 
    /// * `api_key` - API 密钥
    /// 
    /// # 返回
    /// 
    /// * `LlmClient` - LLM 客户端实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let client = LlmClient::new("your-api-key");
    /// ```
    pub fn new(api_key: &str) -> Self {
        LlmClient {
            api_key: api_key.to_string(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            model: "gpt-4-vision-preview".to_string(),
            client: Client::new(),
        }
    }
    
    /// 设置 API 端点
    /// 
    /// # 参数
    /// 
    /// * `endpoint` - API 端点 URL
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let mut client = LlmClient::new("your-api-key");
    /// client.set_endpoint("https://your-custom-api.com/v1/chat/completions");
    /// ```
    pub fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.to_string();
    }
    
    /// 设置模型名称
    /// 
    /// # 参数
    /// 
    /// * `model` - 模型名称
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let mut client = LlmClient::new("your-api-key");
    /// client.set_model("gpt-4-vision-preview");
    /// ```
    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }
    
    /// 图像识别 (OCR)
    /// 
    /// 使用 LLM 进行图像识别,将图像中的文本转换为可编辑的文本。
    /// 
    /// # 参数
    /// 
    /// * `image` - 图像数据
    /// 
    /// # 返回
    /// 
    /// * `Result<String>` - 识别的文本
    /// 
    /// # 错误
    /// 
    /// * `LlmClientError::ApiRequestFailed` - API 请求失败
    /// * `LlmClientError::ResponseParseFailed` - 响应解析失败
    /// * `LlmClientError::AuthenticationFailed` - 认证失败
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let client = LlmClient::new("your-api-key");
    /// // let image = load_image("path/to/image.png");
    /// // let text = client.ocr_image(&image).unwrap();
    /// // println!("识别文本: {}", text);
    /// ```
    pub async fn ocr_image(&self, image: &[u8]) -> Result<String> {
        // 将图像转换为 Base64 编码
        let base64_image = base64::Engine::encode(
            &base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD),
            image
        );
        
        // 构建请求体
        let request = LlmRequest {
            model: self.model.clone(),
            messages: vec![
                LlmMessage {
                    role: "user".to_string(),
                    content: vec![
                        LlmContent::Text(LlmTextContent {
                            r#type: "text".to_string(),
                            text: "请识别这张图片中的所有文本内容,并以清晰的格式返回。如果图片中没有文本,请返回空字符串。".to_string(),
                        }),
                        LlmContent::Image(LlmImageContent {
                            r#type: "image_url".to_string(),
                            image_url: LlmImageUrl {
                                url: format!("data:image/png;base64,{}", base64_image),
                            },
                        }),
                    ],
                },
            ],
            max_tokens: 300,
        };
        
        // 发送请求到 LLM API
        let response = self.client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmClientError::ApiRequestFailed(format!("发送请求失败: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "未知错误".to_string());
            return Err(LlmClientError::ApiRequestFailed(format!(
                "API 请求失败: 状态码 {}, 错误: {}",
                status, error_text
            )));
        }
        
        // 解析响应
        let response_data: LlmResponse = response.json()
            .await
            .map_err(|e| LlmClientError::ResponseParseFailed(format!("解析响应失败: {}", e)))?;
        
        // 提取识别的文本
        let text = response_data.choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_else(|| "".to_string());
        
        Ok(text)
    }
}

/// LLM 请求结构体
#[derive(Debug, Serialize)]
struct LlmRequest {
    /// 模型名称
    model: String,
    /// 消息列表
    messages: Vec<LlmMessage>,
    /// 最大 token 数
    max_tokens: u32,
}

/// LLM 消息结构体
#[derive(Debug, Serialize)]
struct LlmMessage {
    /// 角色 (user, system, assistant)
    role: String,
    /// 内容
    content: Vec<LlmContent>,
}

/// LLM 内容结构体
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum LlmContent {
    /// 文本内容
    Text(LlmTextContent),
    /// 图像内容
    Image(LlmImageContent),
}

/// LLM 文本内容结构体
#[derive(Debug, Serialize)]
struct LlmTextContent {
    /// 内容类型
    r#type: String,
    /// 文本内容
    text: String,
}

/// LLM 图像内容结构体
#[derive(Debug, Serialize)]
struct LlmImageContent {
    /// 内容类型
    r#type: String,
    /// 图像 URL
    image_url: LlmImageUrl,
}

/// LLM 图像 URL 结构体
#[derive(Debug, Serialize)]
struct LlmImageUrl {
    /// 图像 URL
    url: String,
}

/// LLM 响应结构体
#[derive(Debug, Deserialize)]
struct LlmResponse {
    /// 选择列表
    choices: Vec<LlmChoice>,
}

/// LLM 选择结构体
#[derive(Debug, Deserialize)]
struct LlmChoice {
    /// 消息
    message: LlmResponseMessage,
}

/// LLM 响应消息结构体
#[derive(Debug, Deserialize)]
struct LlmResponseMessage {
    /// 角色
    #[serde(default)]
    role: Option<String>,
    /// 内容
    content: Option<String>,
}
