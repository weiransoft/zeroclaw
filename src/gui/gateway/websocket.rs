/// GUI Agent WebSocket 事件推送模块
/// 
/// 本模块提供 WebSocket 事件推送功能,用于实时推送 GUI Agent 事件。

use tokio::sync::broadcast;

/// GUI Agent 事件
#[derive(Debug, Clone)]
pub enum GuiAgentEvent {
    /// 屏幕捕获完成
    ScreenCaptured {
        /// 截图数据
        data: Vec<u8>,
        /// 截图宽度
        width: u32,
        /// 截图高度
        height: u32,
    },
    /// 窗口列表更新
    WindowsUpdated {
        /// 窗口列表
        windows: Vec<crate::gui::screen::window::WindowInfo>,
    },
    /// 任务开始
    TaskStarted {
        /// 任务 ID
        task_id: String,
    },
    /// 任务完成
    TaskCompleted {
        /// 任务 ID
        task_id: String,
        /// 任务结果
        result: crate::gui::automation::scheduler::TaskAction,
    },
    /// 任务失败
    TaskFailed {
        /// 任务 ID
        task_id: String,
        /// 错误信息
        error: String,
    },
    /// 其他事件
    Other {
        /// 事件类型
        event_type: String,
        /// 事件数据
        data: String,
    },
}

/// GUI Agent 事件发送器
/// 
/// 用于通过 WebSocket 推送 GUI Agent 事件到客户端。

pub struct GuiAgentEventSender {
    /// 广播通道发送端
    sender: broadcast::Sender<GuiAgentEvent>,
}

impl GuiAgentEventSender {
    /// 创建新的事件发送器
    /// 
    /// # 返回
    /// 
    /// * `GuiAgentEventSender` - 事件发送器
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let sender = GuiAgentEventSender::new();
    /// ```
    pub fn new() -> Self {
        // 创建广播通道,缓冲区大小为 100
        let (sender, _) = broadcast::channel(100);
        
        GuiAgentEventSender { sender }
    }
    
    /// 发送事件
    /// 
    /// # 参数
    /// 
    /// * `event` - 事件
    /// 
    /// # 返回
    /// 
    /// * `Result<(), broadcast::error::SendError<GuiAgentEvent>>` - 发送结果
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let sender = GuiAgentEventSender::new();
    /// let event = GuiAgentEvent::Other {
    ///     event_type: "test".to_string(),
    ///     data: "test data".to_string(),
    /// };
    /// sender.send(event).unwrap();
    /// ```
    pub fn send(&self, event: GuiAgentEvent) -> Result<(), broadcast::error::SendError<GuiAgentEvent>> {
        // 发送事件到所有订阅者
        let _ = self.sender.send(event);
        Ok(())
    }
    
    /// 订阅事件
    /// 
    /// # 返回
    /// 
    /// * `broadcast::Receiver<GuiAgentEvent>` - 事件接收器
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let sender = GuiAgentEventSender::new();
    /// let mut receiver = sender.subscribe();
    /// 
    /// // 在另一个线程中发送事件
    /// tokio::spawn(async move {
    ///     loop {
    ///         if let Ok(event) = receiver.recv().await {
    ///             println!("收到事件: {:?}", event);
    ///         }
    ///     }
    /// });
    /// ```
    pub fn subscribe(&self) -> broadcast::Receiver<GuiAgentEvent> {
        self.sender.subscribe()
    }
}

impl Default for GuiAgentEventSender {
    fn default() -> Self {
        Self::new()
    }
}
