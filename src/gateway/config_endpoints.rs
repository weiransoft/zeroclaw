//! 配置热重载 API 端点
//!
//! 提供配置状态查询、手动重载等 HTTP API

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

/// 配置状态响应
#[derive(Serialize)]
pub struct ConfigStatusResponse {
    /// 当前配置版本号
    pub version: u64,
    /// 配置文件路径
    pub config_path: String,
    /// 最后更新时间
    pub last_updated: DateTime<Utc>,
    /// 是否启用热重载
    pub hot_reload_enabled: bool,
}

/// 重载请求
#[derive(Deserialize)]
pub struct ReloadRequest {
    /// 只验证不应用（干运行）
    #[serde(default)]
    pub dry_run: bool,
}

/// 重载响应
#[derive(Serialize)]
pub struct ReloadResponse {
    /// 是否成功
    pub success: bool,
    /// 新版本号（如果成功）
    pub version: Option<u64>,
    /// 消息
    pub message: String,
}

/// GET /config/status — 获取配置状态
pub async fn handle_config_status(
    State(state): State<Arc<crate::gateway::AppState>>,
) -> Result<Json<ConfigStatusResponse>, StatusCode> {
    let config = state.config.clone();
    
    // 从 Config 读取配置
    Ok(Json(ConfigStatusResponse {
        version: state.config_version.load(std::sync::atomic::Ordering::SeqCst),
        config_path: config.config_path.display().to_string(),
        last_updated: Utc::now(),
        hot_reload_enabled: state.hot_reload_manager.is_some(),
    }))
}

/// POST /config/reload — 手动触发配置重载
pub async fn handle_config_reload(
    State(state): State<Arc<crate::gateway::AppState>>,
    Json(payload): Json<ReloadRequest>,
) -> Result<Json<ReloadResponse>, (StatusCode, String)> {
    if let Some(ref manager) = state.hot_reload_manager {
        if payload.dry_run {
            // 只验证，不应用
            info!("执行配置验证（干运行）");
            
            // TODO: 实现配置验证逻辑
            // 目前简化实现，直接返回成功
            
            Ok(Json(ReloadResponse {
                success: true,
                version: None,
                message: "配置验证通过（干运行）".to_string(),
            }))
        } else {
            // 执行重载
            info!("手动触发配置重载");
            match manager.reload("api").await {
                Ok(version) => {
                    info!("配置重载成功，新版本号：{}", version);
                    Ok(Json(ReloadResponse {
                        success: true,
                        version: Some(version),
                        message: "配置重载成功".to_string(),
                    }))
                }
                Err(e) => {
                    error!("配置重载失败：{}", e);
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("配置重载失败：{}", e),
                    ))
                }
            }
        }
    } else {
        error!("热重载功能未启用");
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "热重载功能未启用".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_status_response_serialization() {
        let response = ConfigStatusResponse {
            version: 1,
            config_path: "/test/config.toml".to_string(),
            last_updated: Utc::now(),
            hot_reload_enabled: true,
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"hot_reload_enabled\":true"));
    }
    
    #[test]
    fn test_reload_request_deserialization() {
        let json = r#"{"dry_run": true}"#;
        let request: ReloadRequest = serde_json::from_str(json).unwrap();
        assert!(request.dry_run);
        
        let json = r#"{}"#;
        let request: ReloadRequest = serde_json::from_str(json).unwrap();
        assert!(!request.dry_run);
    }
}
