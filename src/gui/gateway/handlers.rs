/// GUI Agent HTTP API 处理器
/// 
/// 本模块提供 GUI Agent 的 HTTP API 处理器,用于处理屏幕捕获、窗口管理等请求。
/// 
/// # 使用示例
/// 
/// ```rust
/// use axum::{routing::get, Router};
/// use zeroclaw::gui::gateway::handlers::GuiAgentHandlers;
/// 
/// let app = Router::new()
///     .route("/gui/capture/screen", get(GuiAgentHandlers::capture_screen_handler))
///     .route("/gui/capture/region", get(GuiAgentHandlers::capture_region_handler))
///     .route("/gui/capture/window", get(GuiAgentHandlers::capture_window_handler));
/// ```

use axum::{
    extract::Query,
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tokio::time;
use std::convert::Infallible;

use crate::gui::screen::capture::ScreenCapture;
use crate::gui::automation::executor::AutomationExecutor;

/// 屏幕捕获请求参数
#[derive(Debug, Deserialize)]
pub struct CaptureScreenQuery {
    /// 捕获类型
    #[serde(rename = "type")]
    pub capture_type: Option<String>,
}

/// 屏幕捕获响应
#[derive(Debug, Serialize)]
pub struct CaptureScreenResponse {
    /// 截图数据 (Base64 编码的 PNG 数据)
    pub data: String,
    /// 截图宽度
    pub width: u32,
    /// 截图高度
    pub height: u32,
}

/// 区域捕获请求参数
#[derive(Debug, Deserialize)]
pub struct CaptureRegionQuery {
    /// 区域左上角 X 坐标
    pub x: u32,
    /// 区域左上角 Y 坐标
    pub y: u32,
    /// 区域宽度
    pub width: u32,
    /// 区域高度
    pub height: u32,
}

/// 区域捕获响应
#[derive(Debug, Serialize)]
pub struct CaptureRegionResponse {
    /// 截图数据 (Base64 编码的 PNG 数据)
    pub data: String,
    /// 区域 X 坐标
    pub x: u32,
    /// 区域 Y 坐标
    pub y: u32,
    /// 区域宽度
    pub width: u32,
    /// 区域高度
    pub height: u32,
}

/// 窗口捕获请求参数
#[derive(Debug, Deserialize)]
pub struct CaptureWindowQuery {
    /// 窗口 ID
    pub window_id: u64,
}

/// 窗口捕获响应
#[derive(Debug, Serialize)]
pub struct CaptureWindowResponse {
    /// 截图数据 (Base64 编码的 PNG 数据)
    pub data: String,
    /// 窗口 ID
    pub window_id: u64,
}

/// GUI Agent 处理器
/// 
/// 提供 GUI Agent 的 HTTP API 处理器。

pub struct GuiAgentHandlers;

impl GuiAgentHandlers {
    /// 捕获全屏处理器
    /// 
    /// # 返回
    /// 
    /// * `StatusCode::OK` - 成功
    /// * `StatusCode::INTERNAL_SERVER_ERROR` - 服务器错误
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl http://localhost:3000/gui/capture/screen
    /// ```
    pub async fn capture_screen_handler(
        Query(_params): Query<CaptureScreenQuery>,
    ) -> (StatusCode, Json<CaptureScreenResponse>) {
        // 创建屏幕捕获实例
        let capture = ScreenCapture::new();
        
        // 捕获全屏
        match capture.capture_screen() {
            Ok(data) => {
                // 将数据转换为 Base64 编码
                let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
                
                (
                    StatusCode::OK,
                    Json(CaptureScreenResponse {
                        data: base64_data,
                        width: capture.get_width(),
                        height: capture.get_height(),
                    }),
                )
            }
            Err(e) => {
                eprintln!("屏幕捕获失败: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CaptureScreenResponse {
                        data: "".to_string(),
                        width: 0,
                        height: 0,
                    }),
                )
            }
        }
    }
    
    /// 捕获区域处理器
    /// 
    /// # 返回
    /// 
    /// * `StatusCode::OK` - 成功
    /// * `StatusCode::BAD_REQUEST` - 参数错误
    /// * `StatusCode::INTERNAL_SERVER_ERROR` - 服务器错误
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl "http://localhost:3000/gui/capture/region?x=0&y=0&width=100&height=100"
    /// ```
    pub async fn capture_region_handler(
        Query(params): Query<CaptureRegionQuery>,
    ) -> (StatusCode, Json<CaptureRegionResponse>) {
        // 创建屏幕捕获实例
        let capture = ScreenCapture::new();
        
        // 捕获指定区域
        match capture.capture_region(params.x, params.y, params.width, params.height) {
            Ok(data) => {
                // 将数据转换为 Base64 编码
                let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
                
                (
                    StatusCode::OK,
                    Json(CaptureRegionResponse {
                        data: base64_data,
                        x: params.x,
                        y: params.y,
                        width: params.width,
                        height: params.height,
                    }),
                )
            }
            Err(e) => {
                eprintln!("区域捕获失败: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CaptureRegionResponse {
                        data: "".to_string(),
                        x: 0,
                        y: 0,
                        width: 0,
                        height: 0,
                    }),
                )
            }
        }
    }
    
    /// 捕获窗口处理器
    /// 
    /// # 返回
    /// 
    /// * `StatusCode::OK` - 成功
    /// * `StatusCode::BAD_REQUEST` - 参数错误
    /// * `StatusCode::INTERNAL_SERVER_ERROR` - 服务器错误
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl "http://localhost:3000/gui/capture/window?window_id=1234"
    /// ```
    pub async fn capture_window_handler(
        Query(params): Query<CaptureWindowQuery>,
    ) -> (StatusCode, Json<CaptureWindowResponse>) {
        // 创建屏幕捕获实例
        let capture = ScreenCapture::new();
        
        // 捕获指定窗口
        match capture.capture_window(params.window_id) {
            Ok(data) => {
                // 将数据转换为 Base64 编码
                let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
                
                (
                    StatusCode::OK,
                    Json(CaptureWindowResponse {
                        data: base64_data,
                        window_id: params.window_id,
                    }),
                )
            }
            Err(e) => {
                eprintln!("窗口捕获失败: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CaptureWindowResponse {
                        data: "".to_string(),
                        window_id: params.window_id,
                    }),
                )
            }
        }
    }
    
    /// 鼠标点击处理器
    /// 
    /// # 返回
    /// 
    /// * `StatusCode::OK` - 成功
    /// * `StatusCode::BAD_REQUEST` - 参数错误
    /// * `StatusCode::INTERNAL_SERVER_ERROR` - 服务器错误
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl -X POST http://localhost:3000/gui/automation/click -H "Content-Type: application/json" -d '{"x": 100, "y": 100}'
    /// ```
    pub async fn click_handler() -> (StatusCode, Json<serde_json::Value>) {
        // 创建自动化执行器实例
        let executor = AutomationExecutor::new();
        
        // 模拟鼠标点击操作
        // 注意: 实际的坐标需要根据屏幕分辨率进行转换
        if let Err(e) = executor.mouse_click(100, 100) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"status": "error", "message": format!("鼠标点击失败: {}", e)})),
            );
        }
        
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok", "message": "鼠标点击操作成功", "x": 100, "y": 100})),
        )
    }
    
    /// 键盘输入处理器
    /// 
    /// # 返回
    /// 
    /// * `StatusCode::OK` - 成功
    /// * `StatusCode::BAD_REQUEST` - 参数错误
    /// * `StatusCode::INTERNAL_SERVER_ERROR` - 服务器错误
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl -X POST http://localhost:3000/gui/automation/type -H "Content-Type: application/json" -d '{"text": "Hello, World!"}'
    /// ```
    pub async fn type_handler() -> (StatusCode, Json<serde_json::Value>) {
        // 创建自动化执行器实例
        let executor = AutomationExecutor::new();
        
        // 模拟键盘输入操作
        let text = "Hello, World!";
        if let Err(e) = executor.type_text(text) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"status": "error", "message": format!("键盘输入失败: {}", e)})),
            );
        }
        
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok", "message": "键盘输入操作成功", "text": text})),
        )
    }
    
    /// 捕获全屏流式处理器 (SSE)
    /// 
    /// # 返回
    /// 
    /// * `Sse<Body>` - SSE 流式响应
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl -N http://localhost:3000/gui/capture/screen/stream
    /// ```
    pub async fn capture_screen_stream_handler() -> axum::response::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
        use axum::response::Sse;
        use tokio_stream::wrappers::UnboundedReceiverStream;
        use tokio::time::Duration;
        use tokio::sync::mpsc;
        
        // 创建屏幕捕获实例
        let capture = ScreenCapture::new();
        
        // 获取屏幕分辨率
        let screen_width = capture.get_width();
        let screen_height = capture.get_height();
        
        // 创建通道用于在流中传递数据
        let (tx, rx) = mpsc::unbounded_channel::<Result<axum::response::sse::Event, Infallible>>();
        
        // 启动流式捕获任务
        tokio::spawn(async move {
            // 创建定时器，每 100ms 捕获一次屏幕
            let mut interval = time::interval(Duration::from_millis(100));
            
            loop {
                // 等待下一个时间点
                interval.tick().await;
                
                // 尝试捕获屏幕
                match capture.capture_screen() {
                    Ok(data) => {
                        // 将截图数据编码为 Base64
                        let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
                        
                        // 创建 SSE 事件
                        let json_data = serde_json::json!({
                            "width": screen_width,
                            "height": screen_height,
                            "data": base64_data,
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        });
                        let event = axum::response::sse::Event::default()
                            .data(serde_json::to_string(&json_data).unwrap());
                        
                        // 发送事件
                        if tx.send(Ok(event)).is_err() {
                            // 如果接收端已关闭，退出循环
                            break;
                        }
                    }
                    Err(e) => {
                        // 记录错误但继续尝试
                        eprintln!("屏幕捕获失败: {}", e);
                    }
                }
            }
        });
        
        // 创建 SSE 流
        let sse_stream = UnboundedReceiverStream::new(rx);
        
        Sse::new(sse_stream)
    }
    
    /// 捕获区域流式处理器 (SSE)
    /// 
    /// # 返回
    /// 
    /// * `Sse<impl Stream>` - SSE 流式响应
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl -N http://localhost:3000/gui/capture/region/stream?x=0&y=0&width=100&height=100
    /// ```
    pub async fn capture_region_stream_handler(
        Query(params): Query<CaptureRegionQuery>,
    ) -> axum::response::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
        use axum::response::Sse;
        use tokio_stream::wrappers::UnboundedReceiverStream;
        use tokio::time::Duration;
        use tokio::sync::mpsc;
        
        // 创建屏幕捕获实例
        let capture = ScreenCapture::new();
        
        // 获取屏幕分辨率
        let screen_width = capture.get_width();
        let screen_height = capture.get_height();
        
        // 验证参数
        if params.x >= screen_width || params.y >= screen_height {
            let json_data = serde_json::json!({
                "error": "区域参数无效"
            });
            let event = axum::response::sse::Event::default()
                .data(serde_json::to_string(&json_data).unwrap());
            let (tx, rx) = mpsc::unbounded_channel();
            tx.send(Ok(event)).unwrap();
            let sse_stream = UnboundedReceiverStream::new(rx);
            return Sse::new(sse_stream);
        }
        
        // 创建通道用于在流中传递数据
        let (tx, rx) = mpsc::unbounded_channel::<Result<axum::response::sse::Event, Infallible>>();
        
        // 启动流式捕获任务
        tokio::spawn(async move {
            // 创建定时器，每 100ms 捕获一次区域
            let mut interval = time::interval(Duration::from_millis(100));
            
            loop {
                // 等待下一个时间点
                interval.tick().await;
                
                // 尝试捕获指定区域
                match capture.capture_region(params.x, params.y, params.width, params.height) {
                    Ok(data) => {
                        // 将截图数据编码为 Base64
                        let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
                        
                        // 创建 SSE 事件
                        let json_data = serde_json::json!({
                            "x": params.x,
                            "y": params.y,
                            "width": params.width,
                            "height": params.height,
                            "data": base64_data,
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        });
                        let event = axum::response::sse::Event::default()
                            .data(serde_json::to_string(&json_data).unwrap());
                        
                        // 发送事件
                        if tx.send(Ok(event)).is_err() {
                            // 如果接收端已关闭，退出循环
                            break;
                        }
                    }
                    Err(e) => {
                        // 记录错误但继续尝试
                        eprintln!("区域捕获失败: {}", e);
                    }
                }
            }
        });
        
        // 创建 SSE 流
        let sse_stream = UnboundedReceiverStream::new(rx);
        
        Sse::new(sse_stream)
    }
    
    /// 捕获窗口流式处理器 (SSE)
    /// 
    /// # 返回
    /// 
    /// * `Sse<impl Stream>` - SSE 流式响应
    /// 
    /// # 示例
    /// 
    /// ```bash
    /// curl -N http://localhost:3000/gui/capture/window/stream?window_id=12345
    /// ```
    pub async fn capture_window_stream_handler(
        Query(params): Query<CaptureWindowQuery>,
    ) -> axum::response::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
        use axum::response::Sse;
        use tokio_stream::wrappers::UnboundedReceiverStream;
        use tokio::time::Duration;
        use tokio::sync::mpsc;
        
        // 创建屏幕捕获实例
        let capture = ScreenCapture::new();
        
        // 创建通道用于在流中传递数据
        let (tx, rx) = mpsc::unbounded_channel::<Result<axum::response::sse::Event, Infallible>>();
        
        // 启动流式捕获任务
        tokio::spawn(async move {
            // 创建定时器，每 100ms 捕获一次窗口
            let mut interval = time::interval(Duration::from_millis(100));
            
            loop {
                // 等待下一个时间点
                interval.tick().await;
                
                // 尝试捕获指定窗口
                match capture.capture_window(params.window_id) {
                    Ok(data) => {
                        // 将截图数据编码为 Base64
                        let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
                        
                        // 创建 SSE 事件
                        let json_data = serde_json::json!({
                            "window_id": params.window_id,
                            "data": base64_data,
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        });
                        let event = axum::response::sse::Event::default()
                            .data(serde_json::to_string(&json_data).unwrap());
                        
                        // 发送事件
                        if tx.send(Ok(event)).is_err() {
                            // 如果接收端已关闭，退出循环
                            break;
                        }
                    }
                    Err(e) => {
                        // 记录错误但继续尝试
                        eprintln!("窗口捕获失败: {}", e);
                    }
                }
            }
        });
        
        // 创建 SSE 流
        let sse_stream = UnboundedReceiverStream::new(rx);
        
        Sse::new(sse_stream)
    }
}
