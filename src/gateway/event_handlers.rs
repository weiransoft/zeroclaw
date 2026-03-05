//! 事件驱动相关处理函数

use axum::{extract::State, Json, http::HeaderMap, response::IntoResponse};
use axum::http::{header, StatusCode};
use serde::Deserialize;

use crate::gateway::AppState;
use crate::workflow::{EventListener, WorkflowEvent, EventType};

/// 添加事件监听器请求
#[derive(Debug, Deserialize)]
pub struct AddEventListenerRequest {
    /// 事件类型
    pub event_type: EventType,
    /// 监听条件
    pub condition: Option<String>,
    /// 触发的工作流 ID
    pub workflow_id: String,
    /// 是否启用
    pub enabled: bool,
}

/// 更新事件监听器请求
#[derive(Debug, Deserialize)]
pub struct UpdateEventListenerRequest {
    /// 监听器 ID
    pub id: String,
    /// 事件类型
    pub event_type: EventType,
    /// 监听条件
    pub condition: Option<String>,
    /// 触发的工作流 ID
    pub workflow_id: String,
    /// 是否启用
    pub enabled: bool,
}

/// 移除事件监听器请求
#[derive(Debug, Deserialize)]
pub struct RemoveEventListenerRequest {
    /// 监听器 ID
    pub id: String,
}

/// 发布事件请求
#[derive(Debug, Deserialize)]
pub struct PublishEventRequest {
    /// 事件类型
    pub event_type: EventType,
    /// 事件来源
    pub source: String,
    /// 事件数据
    pub data: serde_json::Value,
    /// 相关工作流 ID（可选）
    pub workflow_id: Option<String>,
    /// 相关步骤 ID（可选）
    pub step_id: Option<String>,
}

/// 添加事件监听器
///
/// POST /event/listener/add
pub async fn handle_event_listener_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AddEventListenerRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Event listener add: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let event_bus = state.event_bus.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    
    let listener = EventListener {
        id: id.clone(),
        event_type: body.event_type,
        condition: body.condition,
        workflow_id: body.workflow_id,
        enabled: body.enabled,
        created_at: now,
        updated_at: now,
    };
    
    match event_bus.add_listener(listener).await {
        Ok(_) => {
            let result = serde_json::json!({
                "success": true,
                "message": "Event listener added successfully",
                "listener_id": id
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({
                "error": format!("Failed to add event listener: {}", e)
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// 移除事件监听器
///
/// POST /event/listener/remove
pub async fn handle_event_listener_remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RemoveEventListenerRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Event listener remove: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let event_bus = state.event_bus.clone();
    
    match event_bus.remove_listener(&body.id).await {
        Ok(_) => {
            let result = serde_json::json!({
                "success": true,
                "message": "Event listener removed successfully"
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({
                "error": format!("Failed to remove event listener: {}", e)
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// 列出事件监听器
///
/// GET /event/listener/list
pub async fn handle_event_listener_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Event listener list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let event_bus = state.event_bus.clone();
    
    match event_bus.get_listeners().await {
        Ok(listeners) => {
            let result = serde_json::json!({
                "success": true,
                "listeners": listeners
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({
                "error": format!("Failed to get event listeners: {}", e)
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// 更新事件监听器
///
/// POST /event/listener/update
pub async fn handle_event_listener_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdateEventListenerRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Event listener update: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let event_bus = state.event_bus.clone();
    let now = chrono::Utc::now();
    
    let listener = EventListener {
        id: body.id,
        event_type: body.event_type,
        condition: body.condition,
        workflow_id: body.workflow_id,
        enabled: body.enabled,
        created_at: now, // 注意：这里应该保留原始创建时间，后续可以优化
        updated_at: now,
    };
    
    match event_bus.update_listener(listener).await {
        Ok(_) => {
            let result = serde_json::json!({
                "success": true,
                "message": "Event listener updated successfully"
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({
                "error": format!("Failed to update event listener: {}", e)
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// 发布事件
///
/// POST /event/publish
pub async fn handle_event_publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PublishEventRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Event publish: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let event_bus = state.event_bus.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    
    let event = WorkflowEvent {
        id: id.clone(),
        event_type: body.event_type,
        source: body.source,
        data: body.data,
        timestamp: now,
        workflow_id: body.workflow_id,
        step_id: body.step_id,
    };
    
    match event_bus.publish_event(event).await {
        Ok(_) => {
            let result = serde_json::json!({
                "success": true,
                "message": "Event published successfully",
                "event_id": id
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({
                "error": format!("Failed to publish event: {}", e)
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}
