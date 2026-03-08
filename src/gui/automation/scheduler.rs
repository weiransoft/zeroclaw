/// GUI Agent 任务调度器
/// 
/// 本模块提供任务调度功能,支持定时任务、一次性任务和条件触发任务。

use std::collections::HashMap;
use std::result;
use std::sync::Mutex;

use crate::gui::automation::executor::{AutomationExecutor, AutomationError};

/// 任务调度器结果类型
pub type Result<T> = result::Result<T, TaskSchedulerError>;

/// 任务调度器错误类型
#[derive(Debug)]
pub enum TaskSchedulerError {
    /// 任务执行失败
    TaskExecutionFailed(String),
    /// 任务未找到
    TaskNotFound(String),
    /// 参数无效
    InvalidParameter(String),
    /// 其他错误
    Other(String),
}

impl From<AutomationError> for TaskSchedulerError {
    fn from(err: AutomationError) -> Self {
        TaskSchedulerError::TaskExecutionFailed(format!("自动化执行失败: {}", err))
    }
}

impl std::fmt::Display for TaskSchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskSchedulerError::TaskExecutionFailed(msg) => write!(f, "任务执行失败: {}", msg),
            TaskSchedulerError::TaskNotFound(msg) => write!(f, "任务未找到: {}", msg),
            TaskSchedulerError::InvalidParameter(msg) => write!(f, "参数无效: {}", msg),
            TaskSchedulerError::Other(msg) => write!(f, "其他错误: {}", msg),
        }
    }
}

impl std::error::Error for TaskSchedulerError {}

/// 任务动作类型
#[derive(Debug, Clone)]
pub enum TaskAction {
    /// 点击屏幕位置
    Click { x: i32, y: i32 },
    /// 输入文本
    TypeText { text: String },
    /// 启动应用
    LaunchApp { path: String },
    /// 其他动作
    Custom { name: String, data: String },
}

/// 定时任务结构体
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    /// 任务 ID
    pub id: String,
    /// 任务类型
    pub task_type: TaskType,
    /// 任务动作
    pub action: TaskAction,
    /// 下次执行时间
    pub next_run: u64,
    /// 是否启用
    pub enabled: bool,
    /// 执行次数
    pub execution_count: u64,
    /// 最大执行次数 (0 表示无限)
    pub max_executions: u64,
}

/// 任务类型
#[derive(Debug, Clone)]
pub enum TaskType {
    /// 定时任务 (Cron)
    Cron { cron: String },
    /// 一次性任务
    Once { delay_ms: u64 },
    /// 条件触发任务
    Conditional { condition: String },
}

/// 任务调度器
/// 
/// 提供任务调度功能,支持定时任务、一次性任务和条件触发任务。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::automation::scheduler::{TaskScheduler, TaskAction, TaskType};
/// 
/// let scheduler = TaskScheduler::new();
/// scheduler.add_cron_task("task-001", "* * * * *", TaskAction::Click { x: 100, y: 100 }).unwrap();
/// ```

pub struct TaskScheduler {
    /// 任务列表
    tasks: Mutex<HashMap<String, ScheduledTask>>,
    /// 自动化执行器
    executor: AutomationExecutor,
}

impl TaskScheduler {
    /// 创建新的任务调度器实例
    /// 
    /// # 返回
    /// 
    /// * `TaskScheduler` - 任务调度器实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// ```
    pub fn new() -> Self {
        TaskScheduler {
            tasks: Mutex::new(HashMap::new()),
            executor: AutomationExecutor::new(),
        }
    }
    
    /// 添加定时任务 (Cron)
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// * `cron` - Cron 表达式
    /// * `action` - 任务动作
    /// 
    /// # 返回
    /// 
    /// * `Result<()>` - 添加结果
    /// 
    /// # 错误
    /// 
    /// * `TaskSchedulerError::InvalidParameter` - 参数无效
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_cron_task("task-001", "* * * * *", TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// ```
    pub fn add_cron_task(&self, id: &str, cron: &str, action: TaskAction) -> Result<()> {
        // 验证参数
        if id.is_empty() {
            return Err(TaskSchedulerError::InvalidParameter("任务 ID 不能为空".to_string()));
        }
        
        if cron.is_empty() {
            return Err(TaskSchedulerError::InvalidParameter("Cron 表达式不能为空".to_string()));
        }
        
        // 获取当前时间戳
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        // 创建任务
        let task = ScheduledTask {
            id: id.to_string(),
            task_type: TaskType::Cron { cron: cron.to_string() },
            action,
            next_run: now,
            enabled: true,
            execution_count: 0,
            max_executions: 0, // 0 表示无限
        };
        
        // 添加任务到列表
        let mut tasks = self.tasks.lock().map_err(|e| TaskSchedulerError::Other(format!("获取任务锁失败: {}", e)))?;
        tasks.insert(task.id.clone(), task);
        
        Ok(())
    }
    
    /// 添加一次性任务
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// * `delay_ms` - 延迟时间 (毫秒)
    /// * `action` - 任务动作
    /// 
    /// # 返回
    /// 
    /// * `Result<()>` - 添加结果
    /// 
    /// # 错误
    /// 
    /// * `TaskSchedulerError::InvalidParameter` - 参数无效
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_once_task("task-001", 1000, TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// ```
    pub fn add_once_task(&self, id: &str, delay_ms: u64, action: TaskAction) -> Result<()> {
        // 验证参数
        if id.is_empty() {
            return Err(TaskSchedulerError::InvalidParameter("任务 ID 不能为空".to_string()));
        }
        
        // 获取当前时间戳
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        // 计算下次执行时间
        let next_run = now + delay_ms / 1000;
        
        // 创建任务
        let task = ScheduledTask {
            id: id.to_string(),
            task_type: TaskType::Once { delay_ms },
            action,
            next_run,
            enabled: true,
            execution_count: 0,
            max_executions: 1, // 一次性任务只执行一次
        };
        
        // 添加任务到列表
        let mut tasks = self.tasks.lock().map_err(|e| TaskSchedulerError::Other(format!("获取任务锁失败: {}", e)))?;
        tasks.insert(task.id.clone(), task);
        
        Ok(())
    }
    
    /// 添加重复任务
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// * `interval_ms` - 间隔时间 (毫秒)
    /// * `max_executions` - 最大执行次数 (0 表示无限)
    /// * `action` - 任务动作
    /// 
    /// # 返回
    /// 
    /// * `Result<()>` - 添加结果
    /// 
    /// # 错误
    /// 
    /// * `TaskSchedulerError::InvalidParameter` - 参数无效
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_repeat_task("task-001", 1000, 10, TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// ```
    pub fn add_repeat_task(&self, id: &str, interval_ms: u64, max_executions: u64, action: TaskAction) -> Result<()> {
        // 验证参数
        if id.is_empty() {
            return Err(TaskSchedulerError::InvalidParameter("任务 ID 不能为空".to_string()));
        }
        
        // 获取当前时间戳
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        // 计算下次执行时间
        let next_run = now + interval_ms / 1000;
        
        // 创建任务
        let task = ScheduledTask {
            id: id.to_string(),
            task_type: TaskType::Once { delay_ms: interval_ms },
            action,
            next_run,
            enabled: true,
            execution_count: 0,
            max_executions,
        };
        
        // 添加任务到列表
        let mut tasks = self.tasks.lock().map_err(|e| TaskSchedulerError::Other(format!("获取任务锁失败: {}", e)))?;
        tasks.insert(task.id.clone(), task);
        
        Ok(())
    }
    
    /// 取消任务
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// 
    /// # 返回
    /// 
    /// * `bool` - 是否成功取消
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_cron_task("task-001", "* * * * *", TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// let canceled = scheduler.cancel_task("task-001");
    /// println!("任务已取消: {}", canceled);
    /// ```
    pub fn cancel_task(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.remove(id).is_some()
    }
    
    /// 列出所有任务
    /// 
    /// # 返回
    /// 
    /// * `Vec<ScheduledTask>` - 任务列表
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_cron_task("task-001", "* * * * *", TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// let tasks = scheduler.list_tasks();
    /// for task in tasks {
    ///     println!("任务: {:?}", task.id);
    /// }
    /// ```
    pub fn list_tasks(&self) -> Vec<ScheduledTask> {
        let tasks = self.tasks.lock().unwrap();
        tasks.values().cloned().collect()
    }
    
    /// 执行任务
    /// 
    /// # 参数
    /// 
    /// * `task` - 任务
    /// 
    /// # 返回
    /// 
    /// * `Result<()>` - 执行结果
    /// 
    /// # 错误
    /// 
    /// * `TaskSchedulerError::TaskExecutionFailed` - 任务执行失败
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// let action = TaskAction::Click { x: 100, y: 100 };
    /// scheduler.add_cron_task("task-001", "* * * * *", action.clone()).unwrap();
    /// 
    /// let tasks = scheduler.list_tasks();
    /// if let Some(task) = tasks.first() {
    ///     let result = scheduler.execute_task(task);
    ///     println!("任务执行结果: {:?}", result);
    /// }
    /// ```
    pub fn execute_task(&self, task: &ScheduledTask) -> Result<()> {
        // 检查任务是否可以继续执行
        if !self.can_continue_execution(task) {
            return Err(TaskSchedulerError::TaskExecutionFailed("任务已禁用或达到最大执行次数".to_string()));
        }
        
        // 根据任务类型执行相应的动作
        let result: Result<()> = match &task.action {
            TaskAction::Click { x, y } => {
                // 执行点击操作
                self.executor.mouse_click(*x, *y)?;
                Ok(())
            }
            TaskAction::TypeText { text } => {
                // 执行文本输入操作
                self.executor.type_text(text)?;
                Ok(())
            }
            TaskAction::LaunchApp { path } => {
                // 执行启动应用操作
                // 使用 AppleScript 启动应用
                let script = format!(
                    "tell application \"System Events\"\n    do shell script \"open '{}'\"\nend tell",
                    path
                );
                
                let output = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(script)
                    .output()
                    .map_err(|e| TaskSchedulerError::Other(format!("执行 AppleScript 失败: {}", e)))?;
                
                // 检查执行结果
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(TaskSchedulerError::TaskExecutionFailed(format!(
                        "启动应用失败: {}",
                        stderr
                    )));
                }
                
                Ok(())
            }
            TaskAction::Custom { name, data: _ } => {
                // 执行自定义操作
                // 根据名称执行不同的自定义操作
                match name.as_str() {
                    "screenshot" => {
                        // 截图操作
                        let capture = crate::gui::screen::capture::ScreenCapture::new();
                        let _ = capture.capture_screen()?;
                        Ok(())
                    }
                    _ => Err(TaskSchedulerError::InvalidParameter(format!(
                        "未知的自定义操作: {}",
                        name
                    ))),
                }
            }
        };
        
        // 更新执行次数
        let _ = self.update_execution_count(&task.id);
        
        result
    }
    
    /// 批量执行任务
    /// 
    /// # 参数
    /// 
    /// * `task_ids` - 任务 ID 列表
    /// 
    /// # 返回
    /// 
    /// * `Vec<Result<()>>` - 执行结果列表
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// let action = TaskAction::Click { x: 100, y: 100 };
    /// scheduler.add_cron_task("task-001", "* * * * *", action.clone()).unwrap();
    /// scheduler.add_cron_task("task-002", "* * * * *", action).unwrap();
    /// 
    /// let results = scheduler.execute_tasks(&["task-001", "task-002"]);
    /// for result in results {
    ///     println!("任务执行结果: {:?}", result);
    /// }
    /// ```
    pub fn execute_tasks(&self, task_ids: &[&str]) -> Vec<Result<()>> {
        let tasks = self.tasks.lock().unwrap();
        let mut results = Vec::new();
        
        for task_id in task_ids {
            if let Some(task) = tasks.get(*task_id) {
                let result = self.execute_task(task);
                results.push(result);
            } else {
                results.push(Err(TaskSchedulerError::TaskNotFound(task_id.to_string())));
            }
        }
        
        results
    }
    
    /// 启用任务
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// 
    /// # 返回
    /// 
    /// * `bool` - 是否成功启用
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_cron_task("task-001", "* * * * *", TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// let enabled = scheduler.enable_task("task-001");
    /// println!("任务已启用: {}", enabled);
    /// ```
    pub fn enable_task(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(id) {
            task.enabled = true;
            true
        } else {
            false
        }
    }
    
    /// 禁用任务
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// 
    /// # 返回
    /// 
    /// * `bool` - 是否成功禁用
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let scheduler = TaskScheduler::new();
    /// scheduler.add_cron_task("task-001", "* * * * *", TaskAction::Click { x: 100, y: 100 }).unwrap();
    /// let disabled = scheduler.disable_task("task-001");
    /// println!("任务已禁用: {}", disabled);
    /// ```
    pub fn disable_task(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(id) {
            task.enabled = false;
            true
        } else {
            false
        }
    }
    
    /// 检查任务是否可以继续执行
    /// 
    /// # 参数
    /// 
    /// * `task` - 任务
    /// 
    /// # 返回
    /// 
    /// * `bool` - 是否可以继续执行
    fn can_continue_execution(&self, task: &ScheduledTask) -> bool {
        // 检查任务是否启用
        if !task.enabled {
            return false;
        }
        
        // 检查是否达到最大执行次数
        if task.max_executions > 0 && task.execution_count >= task.max_executions {
            return false;
        }
        
        true
    }
    
    /// 更新执行次数
    /// 
    /// # 参数
    /// 
    /// * `id` - 任务 ID
    /// 
    /// # 返回
    /// 
    /// * `Result<u64>` - 更新后的执行次数
    fn update_execution_count(&self, id: &str) -> Result<u64> {
        let mut tasks = self.tasks.lock().map_err(|e| TaskSchedulerError::Other(format!("获取任务锁失败: {}", e)))?;
        
        if let Some(task) = tasks.get_mut(id) {
            task.execution_count += 1;
            Ok(task.execution_count)
        } else {
            Err(TaskSchedulerError::TaskNotFound(id.to_string()))
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}
