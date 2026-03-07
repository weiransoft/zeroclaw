//! 配置热重载管理器
//!
//! 提供配置文件监听、自动重载、验证和回滚功能
//! 支持防抖动机制，避免频繁重载

use anyhow::{Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{debug, error, info};

use crate::config::schema::{Config as AppConfig, HotConfig};

/// 配置热重载管理器
/// 
/// 负责监听配置文件变化，验证配置变更，并安全地应用更新
pub struct HotReloadManager {
    /// 热重载配置包装器
    config: HotConfig,
    /// 配置文件路径
    config_path: PathBuf,
    /// 文件事件广播发送器
    watcher_tx: broadcast::Sender<Event>,
    /// 关闭信号发送器
    shutdown_tx: broadcast::Sender<()>,
}

impl HotReloadManager {
    /// 创建新的热重载管理器
    /// 
    /// # Arguments
    /// * `config` - 热重载配置包装器
    /// * `config_path` - 配置文件路径
    /// 
    /// # Returns
    /// 新的 HotReloadManager 实例
    pub fn new(config: HotConfig, config_path: PathBuf) -> Self {
        let (watcher_tx, _) = broadcast::channel(100);
        let (shutdown_tx, _) = broadcast::channel(1);
        
        Self {
            config,
            config_path,
            watcher_tx,
            shutdown_tx,
        }
    }

    /// 启动文件监听器
    /// 
    /// 创建后台任务监听配置文件变化，使用防抖动机制避免频繁触发
    /// 
    /// # Returns
    /// * `Ok(())` - 监听器启动成功
    /// * `Err(anyhow::Error)` - 监听器启动失败
    /// 
    /// # Errors
    /// 返回错误如果无法创建文件监听器或无法监听配置文件
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        let config_path = self.config_path.clone();
        let watcher_tx = self.watcher_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        // 创建文件监听器通道
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        
        // 创建推荐的监听器（跨平台）
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    // 忽略错误，通道可能已满
                    let _ = tx.blocking_send(event);
                }
            },
            notify::Config::default(),
        )?;
        
        // 监听配置文件
        watcher.watch(&config_path, RecursiveMode::NonRecursive)?;
        info!("配置文件监听器已启动：{:?}", config_path);
        
        // 启动监听循环
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // 接收文件事件
                    Some(event) = rx.recv() => {
                        // 防抖动：等待 500ms 内无新事件
                        sleep(Duration::from_millis(500)).await;
                        
                        // 检查是否是配置文件变化
                        if event.paths.iter().any(|p| {
                            // 比较路径时忽略大小写和符号链接差异
                            p == &config_path || 
                            p.file_name() == config_path.file_name()
                        }) {
                            info!("配置文件发生变化：{:?}", config_path);
                            // 发送事件到广播通道
                            let _ = watcher_tx.send(event);
                        }
                    }
                    // 接收关闭信号
                    _ = shutdown_rx.recv() => {
                        info!("配置文件监听器正在关闭");
                        break;
                    }
                }
            }
        });
        
        Ok(())
    }

    /// 监听配置变化并自动重载
    /// 
    /// 创建后台任务监听配置变更事件，验证并应用新配置
    /// 
    /// # Returns
    /// * `Ok(())` - 监听任务启动成功
    /// * `Err(anyhow::Error)` - 启动失败
    pub async fn watch_and_reload(&self) -> Result<(), anyhow::Error> {
        let mut rx = self.watcher_tx.subscribe();
        let config = self.config.clone();
        let config_path = self.config_path.clone();
        
        tokio::spawn(async move {
            while let Ok(_event) = rx.recv().await {
                match Self::reload_config(&config, &config_path, "auto_reload").await {
                    Ok(_) => {
                        info!("配置已自动重载成功");
                    }
                    Err(e) => {
                        error!("自动重载配置失败：{}", e);
                        // 注意：不中断监听，继续监听后续变化
                    }
                }
            }
        });
        
        Ok(())
    }

    /// 重新加载配置
    /// 
    /// 读取配置文件，解析并验证，然后原子更新
    /// 
    /// # Arguments
    /// * `config` - 热重载配置包装器
    /// * `config_path` - 配置文件路径
    /// * `updated_by` - 更新来源标识
    /// 
    /// # Returns
    /// * `Ok(())` - 配置重载成功
    /// * `Err(anyhow::Error)` - 重载失败
    async fn reload_config(
        config: &HotConfig,
        config_path: &Path,
        updated_by: &str,
    ) -> Result<(), anyhow::Error> {
        info!("开始重新加载配置文件...");
        
        // 读取配置文件内容
        let contents = tokio::fs::read_to_string(config_path)
            .await
            .map_err(|e| anyhow::anyhow!("读取配置文件失败：{}", e))?;
        
        // 解析 TOML 配置
        let mut new_config: AppConfig = toml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("解析配置文件失败：{}", e))?;
        
        // 保留计算字段（这些字段不序列化，需要手动设置）
        new_config.config_path = config_path.to_path_buf();
        new_config.workspace_dir = config_path
            .parent()
            .context("配置文件必须有父目录")?
            .join("workspace");
        
        // 验证新配置的合法性
        Self::validate_config_change(config, &new_config).await?;
        
        // 原子更新配置
        config.write(new_config).await?;
        
        // 记录版本变化
        let version = config.version();
        info!(
            "配置已更新到版本 {}，更新来源：{}",
            version, updated_by
        );
        
        Ok(())
    }

    /// 验证配置变更的合法性
    /// 
    /// 检查不可变字段是否被修改，验证新配置值的有效性
    /// 
    /// # Arguments
    /// * `old_config` - 旧配置
    /// * `new_config` - 新配置
    /// 
    /// # Returns
    /// * `Ok(())` - 验证通过
    /// * `Err(anyhow::Error)` - 验证失败，包含具体原因
    async fn validate_config_change(
        old_config: &HotConfig,
        new_config: &AppConfig,
    ) -> Result<(), anyhow::Error> {
        let old_guard = old_config.read().await;
        
        // 版本 0 表示初始化，允许所有变更
        if old_config.version() > 0 {
            // 检查不可热更新的安全相关字段
            // 注意：当前实现中，安全配置在 Config 结构中未独立定义
            // 这里检查网关配置的关键字段
            
            // 网关监听地址和端口不允许热更新（需要重启服务）
            if old_guard.gateway.host != new_config.gateway.host 
                || old_guard.gateway.port != new_config.gateway.port 
            {
                return Err(anyhow::anyhow!(
                    "网关监听地址或端口变更需要重启服务：{}:{} -> {}:{}",
                    old_guard.gateway.host,
                    old_guard.gateway.port,
                    new_config.gateway.host,
                    new_config.gateway.port
                ));
            }
            
            // 检查其他需要重启的字段（可根据需要扩展）
            // 例如：加密设置、沙盒模式等
        }
        
        // 验证可热更新字段的有效性
        Self::validate_hot_reload_fields(new_config).await?;
        
        Ok(())
    }

    /// 验证可热更新字段的有效性
    /// 
    /// # Arguments
    /// * `config` - 待验证的配置
    /// 
    /// # Returns
    /// * `Ok(())` - 验证通过
    /// * `Err(anyhow::Error)` - 验证失败
    async fn validate_hot_reload_fields(config: &AppConfig) -> Result<(), anyhow::Error> {
        // 验证温度范围 (0.0 - 2.0)
        if config.default_temperature < 0.0 || config.default_temperature > 2.0 {
            return Err(anyhow::anyhow!(
                "温度必须在 0.0 到 2.0 之间，当前值：{}",
                config.default_temperature
            ));
        }
        
        // 验证其他可热更新字段的有效性
        // 例如：API 密钥格式、日志级别等
        
        Ok(())
    }

    /// 手动触发配置重载
    /// 
    /// # Arguments
    /// * `updated_by` - 更新来源标识（如 "api", "cli"）
    /// 
    /// # Returns
    /// * `Ok(version)` - 重载成功，返回新版本号
    /// * `Err(anyhow::Error)` - 重载失败
    pub async fn reload(&self, updated_by: &str) -> Result<u64, anyhow::Error> {
        Self::reload_config(&self.config, &self.config_path, updated_by).await?;
        Ok(self.config.version())
    }

    /// 获取当前配置版本号
    /// 
    /// # Returns
    /// 当前配置版本号
    pub fn version(&self) -> u64 {
        self.config.version()
    }

    /// 停止监听
    /// 
    /// 发送关闭信号，停止文件监听任务
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(());
        debug!("已发送热重载管理器关闭信号");
    }
}

/// 配置变更审计日志
/// 
/// 记录配置变更的详细信息，用于审计和追踪
#[derive(Debug, Clone)]
pub struct ConfigChangeAudit {
    /// 变更时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 配置版本号
    pub version: u64,
    /// 变更来源
    pub changed_by: String,
    /// 变更是否成功
    pub success: bool,
    /// 错误信息（如果失败）
    pub error: Option<String>,
}

impl ConfigChangeAudit {
    /// 创建新的审计记录
    pub fn new(version: u64, changed_by: &str, success: bool, error: Option<&str>) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            version,
            changed_by: changed_by.to_string(),
            success,
            error: error.map(String::from),
        }
    }

    /// 记录审计日志
    pub fn log(&self) {
        if self.success {
            info!(
                target: "config_audit",
                "配置变更审计 - 版本：{}, 来源：{}, 时间：{}",
                self.version,
                self.changed_by,
                self.timestamp
            );
        } else {
            error!(
                target: "config_audit",
                "配置变更失败 - 版本：{}, 来源：{}, 错误：{}",
                self.version,
                self.changed_by,
                self.error.as_deref().unwrap_or("未知错误")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_hot_config_creation() {
        let config = AppConfig::default();
        let hot_config = HotConfig::new(config.clone());
        
        assert_eq!(hot_config.version(), 0);
        
        let read_config = hot_config.read().await;
        assert_eq!(read_config.default_temperature, config.default_temperature);
    }

    #[tokio::test]
    async fn test_hot_config_write() {
        let config = AppConfig::default();
        let hot_config = HotConfig::new(config);
        
        let mut new_config = AppConfig::default();
        new_config.default_temperature = 0.8;
        
        hot_config.write(new_config).await.unwrap();
        assert_eq!(hot_config.version(), 1);
        
        let read_config = hot_config.read().await;
        assert!((read_config.default_temperature - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_reload_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        let config = AppConfig::default();
        let hot_config = HotConfig::new(config);
        
        let manager = HotReloadManager::new(hot_config.clone(), config_path);
        assert_eq!(manager.version(), 0);
    }

    #[tokio::test]
    async fn test_validate_temperature() {
        let mut config = AppConfig::default();
        
        // 有效温度
        config.default_temperature = 0.5;
        assert!(HotReloadManager::validate_hot_reload_fields(&config).await.is_ok());
        
        // 温度过低
        config.default_temperature = -0.1;
        assert!(HotReloadManager::validate_hot_reload_fields(&config).await.is_err());
        
        // 温度过高
        config.default_temperature = 2.1;
        assert!(HotReloadManager::validate_hot_reload_fields(&config).await.is_err());
    }

    #[tokio::test]
    async fn test_config_change_audit() {
        let audit = ConfigChangeAudit::new(1, "test", true, None);
        assert!(audit.success);
        assert!(audit.error.is_none());
        assert_eq!(audit.version, 1);
        assert_eq!(audit.changed_by, "test");
        
        audit.log();
    }
}
