/// 图像分析模块
/// 
/// 本模块提供图像识别功能,支持模板匹配和 OCR 识别。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::image::ImageAnalyzer;
/// 
/// // 创建图像分析器实例
/// let mut analyzer = ImageAnalyzer::new();
/// 
/// // 加载模板
/// analyzer.load_template("button", "path/to/button.png").unwrap();
/// 
/// // 查找模板
/// // let screen = capture_screen();
/// // if let Some(rect) = analyzer.find_template(&screen, "button") {
/// //     println!("找到按钮: {:?}", rect);
/// // }
/// 
/// // OCR 识别
/// // let text = analyzer.ocr_region(&image).unwrap();
/// // println!("识别文本: {}", text);
/// ```

use std::collections::HashMap;
use std::fs;

/// 图像分析结果类型
pub type Result<T> = std::result::Result<T, ImageAnalyzerError>;

/// 图像分析错误类型
#[derive(Debug)]
pub enum ImageAnalyzerError {
    /// 模板加载失败
    TemplateLoadFailed(String),
    /// OCR 失败
    OcrFailed(String),
    /// LLM 识别失败
    LlmFailed(String),
    /// 文件操作失败
    FileError(String),
    /// 参数无效
    InvalidParameter(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for ImageAnalyzerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageAnalyzerError::TemplateLoadFailed(msg) => write!(f, "模板加载失败: {}", msg),
            ImageAnalyzerError::OcrFailed(msg) => write!(f, "OCR 失败: {}", msg),
            ImageAnalyzerError::LlmFailed(msg) => write!(f, "LLM 识别失败: {}", msg),
            ImageAnalyzerError::FileError(msg) => write!(f, "文件操作失败: {}", msg),
            ImageAnalyzerError::InvalidParameter(msg) => write!(f, "参数无效: {}", msg),
            ImageAnalyzerError::Other(msg) => write!(f, "其他错误: {}", msg),
        }
    }
}

impl std::error::Error for ImageAnalyzerError {}

/// 矩形区域
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    /// 左上角 X 坐标
    pub x: i32,
    /// 左上角 Y 坐标
    pub y: i32,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
}

impl Rect {
    /// 创建新的矩形区域
    /// 
    /// # 参数
    /// 
    /// * `x` - 左上角 X 坐标
    /// * `y` - 左上角 Y 坐标
    /// * `width` - 宽度
    /// * `height` - 高度
    /// 
    /// # 返回
    /// 
    /// * `Rect` - 矩形区域
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let rect = Rect::new(0, 0, 100, 100);
    /// println!("矩形: ({}, {}), 宽: {}, 高: {}", rect.x, rect.y, rect.width, rect.height);
    /// ```
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Rect { x, y, width, height }
    }
}

/// 图像分析器
/// 
/// 提供图像识别功能,支持模板匹配和 OCR 识别。
/// 
/// # 使用示例
/// 
/// ```rust
/// let mut analyzer = ImageAnalyzer::new();
/// analyzer.load_template("button", "path/to/button.png").unwrap();
/// ```

pub struct ImageAnalyzer {
    /// 模板缓存
    template_cache: HashMap<String, Vec<u8>>,
    /// LLM 识别客户端 (可选)
    llm_client: Option<crate::gui::screen::image::llm::LlmClient>,
}

impl ImageAnalyzer {
    /// 创建新的图像分析器实例
    /// 
    /// # 返回
    /// 
    /// * `ImageAnalyzer` - 图像分析器实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let analyzer = ImageAnalyzer::new();
    /// ```
    pub fn new() -> Self {
        ImageAnalyzer {
            template_cache: HashMap::new(),
            llm_client: None,
        }
    }
    
    /// 加载模板图像
    /// 
    /// # 参数
    /// 
    /// * `name` - 模板名称
    /// * `path` - 模板文件路径
    /// 
    /// # 返回
    /// 
    /// * `Result<()>` - 加载结果
    /// 
    /// # 错误
    /// 
    /// * `ImageAnalyzerError::TemplateLoadFailed` - 模板加载失败
    /// * `ImageAnalyzerError::FileError` - 文件操作失败
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let mut analyzer = ImageAnalyzer::new();
    /// analyzer.load_template("button", "path/to/button.png").unwrap();
    /// ```
    pub fn load_template(&mut self, name: &str, path: &str) -> Result<()> {
        // 读取模板文件
        let data = fs::read(path)
            .map_err(|e| ImageAnalyzerError::FileError(format!("读取模板文件失败: {}", e)))?;
        
        // 将模板添加到缓存
        self.template_cache.insert(name.to_string(), data);
        
        Ok(())
    }
    
    /// 移除模板
    /// 
    /// # 参数
    /// 
    /// * `name` - 模板名称
    /// 
    /// # 返回
    /// 
    /// * `bool` - 是否成功移除
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let mut analyzer = ImageAnalyzer::new();
    /// analyzer.load_template("button", "path/to/button.png").unwrap();
    /// let removed = analyzer.remove_template("button");
    /// println!("模板已移除: {}", removed);
    /// ```
    pub fn remove_template(&mut self, name: &str) -> bool {
        self.template_cache.remove(name).is_some()
    }
    
    /// 查找模板
    /// 
    /// # 参数
    /// 
    /// * `_screen` - 屏幕截图数据
    /// * `_name` - 模板名称
    /// 
    /// # 返回
    /// 
    /// * `Result<Option<Rect>>` - 模板位置,如果未找到则返回 None
    /// 
    /// # 错误
    /// 
    /// * `ImageAnalyzerError::TemplateLoadFailed` - 模板未加载
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let mut analyzer = ImageAnalyzer::new();
    /// analyzer.load_template("button", "path/to/button.png").unwrap();
    /// 
    /// // let screen = capture_screen();
    /// // if let Some(rect) = analyzer.find_template(&screen, "button") {
    /// //     println!("找到按钮: {:?}", rect);
    /// // }
    /// ```
    pub fn find_template(&self, _screen: &[u8], _name: &str) -> Result<Option<Rect>> {
        // TODO: 实现模板匹配
        unimplemented!("模板匹配待实现")
    }
    
    /// OCR 识别 (Tesseract + LLM 辅助)
    /// 
    /// 识别策略:
    /// 1. 首先使用 Tesseract 进行 OCR 识别
    /// 2. 如果 Tesseract 识别置信度低于阈值,则使用 LLM 进行辅助识别
    /// 3. 如果 LLM 也失败,则返回错误
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
    /// * `ImageAnalyzerError::OcrFailed` - OCR 失败 (Tesseract 和 LLM 都失败)
    /// * `ImageAnalyzerError::LlmFailed` - LLM 识别失败
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let analyzer = ImageAnalyzer::new();
    /// // let image = load_image("path/to/image.png");
    /// // let text = analyzer.ocr_region(&image).unwrap();
    /// // println!("识别文本: {}", text);
    /// ```
    pub async fn ocr_region(&self, image: &[u8]) -> Result<String> {
        // 步骤 1: 使用 Tesseract 进行 OCR 识别
        match self.ocr_region_tesseract(image).await {
            Ok(text) => {
                // 如果 Tesseract 识别成功,返回结果
                return Ok(text);
            }
            Err(e) => {
                // Tesseract 识别失败,检查是否有 LLM 客户端
                if let Some(ref llm_client) = self.llm_client {
                    // 步骤 2: 使用 LLM 进行辅助识别
                    match llm_client.ocr_image(image).await {
                        Ok(llm_text) => {
                            // LLM 识别成功,返回结果
                            return Ok(llm_text);
                        }
                        Err(llm_error) => {
                            // LLM 也失败,返回错误
                            return Err(ImageAnalyzerError::OcrFailed(format!(
                                "Tesseract 和 LLM 都失败: Tesseract: {}, LLM: {}",
                                e, llm_error
                            )));
                        }
                    }
                } else {
                    // 没有 LLM 客户端,返回 Tesseract 错误
                    return Err(e);
                }
            }
        }
    }
    
    /// Tesseract OCR 识别
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
    /// * `ImageAnalyzerError::OcrFailed` - OCR 失败
    #[cfg(feature = "ocr")]
    async fn ocr_region_tesseract(&self, image: &[u8]) -> Result<String> {
        // TODO: 实现 Tesseract OCR 识别
        unimplemented!("Tesseract OCR 待实现")
    }
    
    /// 设置 LLM 客户端
    /// 
    /// # 参数
    /// 
    /// * `client` - LLM 客户端
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let mut analyzer = ImageAnalyzer::new();
    /// // let llm_client = LlmClient::new();
    /// // analyzer.set_llm_client(llm_client);
    /// ```
    pub fn set_llm_client(&mut self, client: crate::gui::screen::image::llm::LlmClient) {
        self.llm_client = Some(client);
    }
}

impl Default for ImageAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
