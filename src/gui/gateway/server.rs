/// GUI Agent HTTP 服务器
/// 
/// 本模块提供 HTTP 服务器功能,用于处理 GUI Agent 的 HTTP 请求。

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;

use crate::gui::gateway::handlers::GuiAgentHandlers;

/// GUI Agent HTTP 服务器
/// 
/// 提供 GUI Agent 的 HTTP 服务器功能。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::gateway::server::GuiAgentServer;
/// 
/// let server = GuiAgentServer::new(3000);
/// server.start().await.unwrap();
/// ```

pub struct GuiAgentServer {
    /// 端口号
    port: u16,
}

impl GuiAgentServer {
    /// 创建新的 HTTP 服务器实例
    /// 
    /// # 参数
    /// 
    /// * `port` - 端口号
    /// 
    /// # 返回
    /// 
    /// * `GuiAgentServer` - HTTP 服务器实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let server = GuiAgentServer::new(3000);
    /// ```
    pub fn new(port: u16) -> Self {
        GuiAgentServer { port }
    }
    
    /// 启动 HTTP 服务器
    /// 
    /// # 返回
    /// 
    /// * `Result<(), Box<dyn std::error::Error>>` - 启动结果
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let server = GuiAgentServer::new(3000);
    /// server.start().await.unwrap();
    /// ```
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 创建路由
        let app = Router::new()
            .route("/gui/capture/screen", get(GuiAgentHandlers::capture_screen_handler))
            .route("/gui/capture/region", get(GuiAgentHandlers::capture_region_handler))
            .route("/gui/capture/window", get(GuiAgentHandlers::capture_window_handler))
            .route("/gui/automation/click", post(GuiAgentHandlers::click_handler))
            .route("/gui/automation/type", post(GuiAgentHandlers::type_handler))
            // SSE 流式路由
            .route("/gui/capture/screen/stream", get(GuiAgentHandlers::capture_screen_stream_handler))
            .route("/gui/capture/region/stream", get(GuiAgentHandlers::capture_region_stream_handler))
            .route("/gui/capture/window/stream", get(GuiAgentHandlers::capture_window_stream_handler));
        
        // 绑定地址
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        
        // 启动服务器
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
        
        Ok(())
    }
}
