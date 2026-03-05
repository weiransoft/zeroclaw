//! 数据存储 API 端点
//! 
//! 提供聊天、工作流、知识、经验、团队、灵魂等实体的 CRUD API 端点

use crate::config::Config;
use crate::store::*;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// 应用状态扩展 - 包含所有存储
#[derive(Clone)]
pub struct StoreAppState {
    pub config: Arc<std::sync::Mutex<Config>>,
    pub chat_store: Arc<ChatStore>,
    pub workflow_store: Arc<WorkflowStore>,
    pub knowledge_store: Arc<KnowledgeStore>,
    pub experience_store: Arc<ExperienceStore>,
    pub team_store: Arc<TeamStore>,
    pub soul_store: Arc<SoulStore>,
}

impl StoreAppState {
    /// 从工作区目录创建存储状态
    pub fn from_workspace(workspace_dir: &PathBuf, config: Arc<std::sync::Mutex<Config>>) -> Result<Self> {
        let chat_store = Arc::new(ChatStore::new(workspace_dir)?);
        let workflow_store = Arc::new(WorkflowStore::new(workspace_dir)?);
        let knowledge_store = Arc::new(KnowledgeStore::new(workspace_dir)?);
        let experience_store = Arc::new(ExperienceStore::new(workspace_dir)?);
        let team_store = Arc::new(TeamStore::new(workspace_dir)?);
        let soul_store = Arc::new(SoulStore::new(workspace_dir)?);
        
        Ok(Self {
            config,
            chat_store,
            workflow_store,
            knowledge_store,
            experience_store,
            team_store,
            soul_store,
        })
    }
}

/// 创建存储 API 路由
pub fn create_store_routes() -> Router<StoreAppState> {
    Router::new()
        // 聊天 API
        .route("/chat/sessions", get(list_chat_sessions))
        .route("/chat/sessions", post(create_chat_session))
        .route("/chat/sessions/:id", get(get_chat_session))
        .route("/chat/sessions/:id", put(update_chat_session))
        .route("/chat/sessions/:id", delete(delete_chat_session))
        .route("/chat/sessions/:id/messages", get(list_chat_messages))
        .route("/chat/sessions/:id/messages", post(add_chat_message))
        
        // 工作流 API
        .route("/workflows", get(list_workflows))
        .route("/workflows", post(create_workflow))
        .route("/workflows/:id", get(get_workflow))
        .route("/workflows/:id", put(update_workflow))
        .route("/workflows/:id", delete(delete_workflow))
        .route("/workflows/:id/steps", post(add_workflow_step))
        .route("/workflows/steps/:id", put(update_workflow_step))
        .route("/workflow-templates", get(list_workflow_templates))
        .route("/workflow-templates", post(create_workflow_template))
        .route("/workflow-templates/:id", get(get_workflow_template))
        
        // 知识 API
        .route("/knowledge/categories", get(list_knowledge_categories))
        .route("/knowledge/categories", post(create_knowledge_category))
        .route("/knowledge/categories/:id", get(get_knowledge_category))
        .route("/knowledge/categories/:id", put(update_knowledge_category))
        .route("/knowledge/categories/:id", delete(delete_knowledge_category))
        .route("/knowledge/items", get(list_knowledge_items))
        .route("/knowledge/items", post(create_knowledge_item))
        .route("/knowledge/items/:id", get(get_knowledge_item))
        .route("/knowledge/items/:id", put(update_knowledge_item))
        .route("/knowledge/items/:id", delete(delete_knowledge_item))
        .route("/knowledge/search", get(search_knowledge))
        
        // 经验 API
        .route("/experiences", get(list_experiences))
        .route("/experiences", post(create_experience))
        .route("/experiences/:id", get(get_experience))
        .route("/experiences/:id", put(update_experience))
        .route("/experiences/:id", delete(delete_experience))
        .route("/experiences/:id/rate", post(rate_experience))
        .route("/experiences/search", get(search_experiences))
        
        // 团队 API
        .route("/teams", get(list_teams))
        .route("/teams", post(create_team))
        .route("/teams/:id", get(get_team))
        .route("/teams/:id", put(update_team))
        .route("/teams/:id", delete(delete_team))
        .route("/teams/:id/members", post(add_team_member))
        .route("/teams/members/:id", put(update_team_member))
        .route("/teams/members/:id", delete(remove_team_member))
        
        // 灵魂 API
        .route("/souls", get(list_souls))
        .route("/souls", post(create_soul))
        .route("/souls/:id", get(get_soul))
        .route("/souls/:id", put(update_soul))
        .route("/souls/:id", delete(delete_soul))
        .route("/souls/:id/activate", post(activate_soul))
        .route("/souls/:id/traits", post(add_soul_trait))
        .route("/souls/traits/:id", put(update_soul_trait))
        .route("/souls/traits/:id", delete(delete_soul_trait))
        .route("/souls/:id/memories", post(add_soul_memory))
        .route("/souls/:id/memories", get(get_soul_memories))
        .route("/souls/memories/:id", delete(delete_soul_memory))
}

// ========================================
// 聊天 API 处理器
// ========================================

#[derive(Deserialize)]
struct CreateChatSessionRequest {
    name: Option<String>,
    agent_id: Option<String>,
}

async fn create_chat_session(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateChatSessionRequest>,
) -> impl IntoResponse {
    match state.chat_store.create_session(req.name.as_deref(), req.agent_id) {
        Ok(session) => (StatusCode::OK, Json(session)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListChatSessionsQuery {
    limit: Option<usize>,
}

async fn list_chat_sessions(
    State(state): State<StoreAppState>,
    Query(query): Query<ListChatSessionsQuery>,
) -> impl IntoResponse {
    match state.chat_store.list_sessions(query.limit) {
        Ok(sessions) => (StatusCode::OK, Json(sessions)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_chat_session(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.chat_store.get_session(&id) {
        Ok(Some(session)) => (StatusCode::OK, Json(session)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateChatSessionRequest {
    name: Option<String>,
    agent_id: Option<String>,
}

async fn update_chat_session(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateChatSessionRequest>,
) -> impl IntoResponse {
    match state.chat_store.update_session(&id, req.name.as_deref(), req.agent_id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_chat_session(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.chat_store.delete_session(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListChatMessagesQuery {
    limit: Option<usize>,
}

async fn list_chat_messages(
    State(state): State<StoreAppState>,
    Path(session_id): Path<String>,
    Query(query): Query<ListChatMessagesQuery>,
) -> impl IntoResponse {
    match state.chat_store.get_messages(&session_id, query.limit) {
        Ok(messages) => (StatusCode::OK, Json(messages)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct AddChatMessageRequest {
    role: String,
    content: String,
    tool_calls: Option<serde_json::Value>,
}

async fn add_chat_message(
    State(state): State<StoreAppState>,
    Path(session_id): Path<String>,
    Json(req): Json<AddChatMessageRequest>,
) -> impl IntoResponse {
    match state.chat_store.add_message(&session_id, &req.role, &req.content, req.tool_calls) {
        Ok(message) => (StatusCode::OK, Json(message)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ========================================
// 工作流 API 处理器
// ========================================

#[derive(Deserialize)]
struct CreateWorkflowRequest {
    name: String,
    description: String,
    roles: Vec<String>,
}

async fn create_workflow(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> impl IntoResponse {
    match state.workflow_store.create_workflow(&req.name, &req.description, req.roles, None) {
        Ok(workflow) => (StatusCode::OK, Json(workflow)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListWorkflowsQuery {
    limit: Option<usize>,
}

async fn list_workflows(
    State(state): State<StoreAppState>,
    Query(query): Query<ListWorkflowsQuery>,
) -> impl IntoResponse {
    match state.workflow_store.list_workflows(query.limit) {
        Ok(workflows) => (StatusCode::OK, Json(workflows)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_workflow(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.workflow_store.get_workflow(&id) {
        Ok(Some(workflow)) => (StatusCode::OK, Json(workflow)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Workflow not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateWorkflowRequest {
    name: Option<String>,
    description: Option<String>,
    status: Option<String>,
    roles: Option<Vec<String>>,
}

async fn update_workflow(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> impl IntoResponse {
    match state.workflow_store.update_workflow(&id, req.name.as_deref(), req.description.as_deref(), req.status.as_deref(), req.roles) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_workflow(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.workflow_store.delete_workflow(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct AddWorkflowStepRequest {
    name: String,
    description: String,
    order: i32,
}

async fn add_workflow_step(
    State(state): State<StoreAppState>,
    Path(workflow_id): Path<String>,
    Json(req): Json<AddWorkflowStepRequest>,
) -> impl IntoResponse {
    match state.workflow_store.add_step(&workflow_id, &req.name, &req.description, req.order) {
        Ok(step) => (StatusCode::OK, Json(step)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateWorkflowStepRequest {
    name: Option<String>,
    description: Option<String>,
    status: Option<String>,
    assigned_to: Option<String>,
}

async fn update_workflow_step(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkflowStepRequest>,
) -> impl IntoResponse {
    match state.workflow_store.update_step(&id, req.name.as_deref(), req.description.as_deref(), req.status.as_deref(), req.assigned_to.as_deref(), None) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct CreateWorkflowTemplateRequest {
    name: String,
    description: String,
    categories: Vec<String>,
    applicable_scenarios: Vec<String>,
}

async fn create_workflow_template(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateWorkflowTemplateRequest>,
) -> impl IntoResponse {
    match state.workflow_store.create_template(&req.name, &req.description, req.categories, req.applicable_scenarios) {
        Ok(template) => (StatusCode::OK, Json(template)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn list_workflow_templates(
    State(state): State<StoreAppState>,
) -> impl IntoResponse {
    match state.workflow_store.list_templates() {
        Ok(templates) => (StatusCode::OK, Json(templates)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_workflow_template(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.workflow_store.get_template(&id) {
        Ok(Some(template)) => (StatusCode::OK, Json(template)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Template not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ========================================
// 知识 API 处理器
// ========================================

#[derive(Deserialize)]
struct CreateKnowledgeCategoryRequest {
    name: String,
    description: String,
    parent_id: Option<String>,
}

async fn create_knowledge_category(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateKnowledgeCategoryRequest>,
) -> impl IntoResponse {
    match state.knowledge_store.create_category(&req.name, &req.description, req.parent_id) {
        Ok(category) => (StatusCode::OK, Json(category)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListKnowledgeCategoriesQuery {
    parent_id: Option<String>,
}

async fn list_knowledge_categories(
    State(state): State<StoreAppState>,
    Query(query): Query<ListKnowledgeCategoriesQuery>,
) -> impl IntoResponse {
    match state.knowledge_store.list_categories(query.parent_id.as_deref()) {
        Ok(categories) => (StatusCode::OK, Json(categories)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_knowledge_category(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.knowledge_store.get_category(&id) {
        Ok(Some(category)) => (StatusCode::OK, Json(category)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Category not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateKnowledgeCategoryRequest {
    name: Option<String>,
    description: Option<String>,
    parent_id: Option<String>,
}

async fn update_knowledge_category(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateKnowledgeCategoryRequest>,
) -> impl IntoResponse {
    match state.knowledge_store.update_category(&id, req.name.as_deref(), req.description.as_deref(), req.parent_id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_knowledge_category(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.knowledge_store.delete_category(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct CreateKnowledgeItemRequest {
    title: String,
    content: String,
    summary: Option<String>,
    category_id: Option<String>,
    tags: Vec<String>,
    source: Option<String>,
    author: Option<String>,
}

async fn create_knowledge_item(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateKnowledgeItemRequest>,
) -> impl IntoResponse {
    match state.knowledge_store.create_item(&req.title, &req.content, req.summary, req.category_id, req.tags, req.source, req.author) {
        Ok(item) => (StatusCode::OK, Json(item)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListKnowledgeItemsQuery {
    category_id: Option<String>,
    limit: Option<usize>,
}

async fn list_knowledge_items(
    State(state): State<StoreAppState>,
    Query(query): Query<ListKnowledgeItemsQuery>,
) -> impl IntoResponse {
    match state.knowledge_store.list_items(query.category_id.as_deref(), query.limit) {
        Ok(items) => (StatusCode::OK, Json(items)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_knowledge_item(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.knowledge_store.get_item(&id) {
        Ok(Some(item)) => (StatusCode::OK, Json(item)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Item not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateKnowledgeItemRequest {
    title: Option<String>,
    content: Option<String>,
    summary: Option<String>,
    category_id: Option<String>,
    tags: Option<Vec<String>>,
}

async fn update_knowledge_item(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateKnowledgeItemRequest>,
) -> impl IntoResponse {
    match state.knowledge_store.update_item(&id, req.title.as_deref(), req.content.as_deref(), req.summary, req.category_id, req.tags) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_knowledge_item(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.knowledge_store.delete_item(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct SearchKnowledgeQuery {
    q: String,
    limit: Option<usize>,
}

async fn search_knowledge(
    State(state): State<StoreAppState>,
    Query(query): Query<SearchKnowledgeQuery>,
) -> impl IntoResponse {
    match state.knowledge_store.search_items(&query.q, query.limit) {
        Ok(items) => (StatusCode::OK, Json(items)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ========================================
// 经验 API 处理器
// ========================================

#[derive(Deserialize)]
struct CreateExperienceRequest {
    title: String,
    description: String,
    content: String,
    tags: Vec<String>,
    category: Option<String>,
    difficulty_level: Option<String>,
}

async fn create_experience(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateExperienceRequest>,
) -> impl IntoResponse {
    match state.experience_store.create_experience(&req.title, &req.description, &req.content, req.tags, req.category, req.difficulty_level, None) {
        Ok(experience) => (StatusCode::OK, Json(experience)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListExperiencesQuery {
    category: Option<String>,
    limit: Option<usize>,
}

async fn list_experiences(
    State(state): State<StoreAppState>,
    Query(query): Query<ListExperiencesQuery>,
) -> impl IntoResponse {
    match state.experience_store.list_experiences(query.category.as_deref(), query.limit) {
        Ok(experiences) => (StatusCode::OK, Json(experiences)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_experience(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.experience_store.get_experience(&id) {
        Ok(Some(experience)) => (StatusCode::OK, Json(experience)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Experience not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateExperienceRequest {
    title: Option<String>,
    description: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    category: Option<String>,
    difficulty_level: Option<String>,
}

async fn update_experience(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateExperienceRequest>,
) -> impl IntoResponse {
    match state.experience_store.update_experience(&id, req.title.as_deref(), req.description.as_deref(), req.content.as_deref(), req.tags, req.category, req.difficulty_level) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_experience(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.experience_store.delete_experience(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct RateExperienceRequest {
    rating: f64,
}

async fn rate_experience(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<RateExperienceRequest>,
) -> impl IntoResponse {
    match state.experience_store.rate_experience(&id, req.rating) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct SearchExperiencesQuery {
    q: String,
    limit: Option<usize>,
}

async fn search_experiences(
    State(state): State<StoreAppState>,
    Query(query): Query<SearchExperiencesQuery>,
) -> impl IntoResponse {
    match state.experience_store.search_experiences(&query.q, query.limit) {
        Ok(experiences) => (StatusCode::OK, Json(experiences)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ========================================
// 团队 API 处理器
// ========================================

#[derive(Deserialize)]
struct CreateTeamRequest {
    name: String,
    description: String,
    owner_id: String,
}

async fn create_team(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateTeamRequest>,
) -> impl IntoResponse {
    match state.team_store.create_team(&req.name, &req.description, &req.owner_id) {
        Ok(team) => (StatusCode::OK, Json(team)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListTeamsQuery {
    user_id: Option<String>,
    limit: Option<usize>,
}

async fn list_teams(
    State(state): State<StoreAppState>,
    Query(query): Query<ListTeamsQuery>,
) -> impl IntoResponse {
    let result = if let Some(user_id) = query.user_id {
        state.team_store.list_teams_by_user(&user_id, query.limit)
    } else {
        Ok(vec![])
    };
    
    match result {
        Ok(teams) => (StatusCode::OK, Json(teams)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_team(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.team_store.get_team(&id) {
        Ok(Some(team)) => (StatusCode::OK, Json(team)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Team not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateTeamRequest {
    name: Option<String>,
    description: Option<String>,
}

async fn update_team(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTeamRequest>,
) -> impl IntoResponse {
    match state.team_store.update_team(&id, req.name.as_deref(), req.description.as_deref()) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_team(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.team_store.delete_team(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct AddTeamMemberRequest {
    user_id: String,
    role: String,
}

async fn add_team_member(
    State(state): State<StoreAppState>,
    Path(team_id): Path<String>,
    Json(req): Json<AddTeamMemberRequest>,
) -> impl IntoResponse {
    match state.team_store.add_member(&team_id, &req.user_id, &req.role) {
        Ok(member) => (StatusCode::OK, Json(member)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateTeamMemberRequest {
    role: Option<String>,
}

async fn update_team_member(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTeamMemberRequest>,
) -> impl IntoResponse {
    match state.team_store.update_member(&id, req.role.as_deref()) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn remove_team_member(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.team_store.remove_member(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ========================================
// 灵魂 API 处理器
// ========================================

#[derive(Deserialize)]
struct CreateSoulRequest {
    name: String,
    description: String,
    personality: String,
}

async fn create_soul(
    State(state): State<StoreAppState>,
    Json(req): Json<CreateSoulRequest>,
) -> impl IntoResponse {
    match state.soul_store.create_soul(&req.name, &req.description, &req.personality) {
        Ok(soul) => (StatusCode::OK, Json(soul)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct ListSoulsQuery {
    limit: Option<usize>,
}

async fn list_souls(
    State(state): State<StoreAppState>,
    Query(query): Query<ListSoulsQuery>,
) -> impl IntoResponse {
    match state.soul_store.list_souls(query.limit) {
        Ok(souls) => (StatusCode::OK, Json(souls)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_soul(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.soul_store.get_soul(&id) {
        Ok(Some(soul)) => (StatusCode::OK, Json(soul)),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Soul not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateSoulRequest {
    name: Option<String>,
    description: Option<String>,
    personality: Option<String>,
}

async fn update_soul(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSoulRequest>,
) -> impl IntoResponse {
    match state.soul_store.update_soul(&id, req.name.as_deref(), req.description.as_deref(), req.personality.as_deref()) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_soul(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.soul_store.delete_soul(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn activate_soul(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.soul_store.activate_soul(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct AddSoulTraitRequest {
    name: String,
    value: f64,
    description: String,
}

async fn add_soul_trait(
    State(state): State<StoreAppState>,
    Path(soul_id): Path<String>,
    Json(req): Json<AddSoulTraitRequest>,
) -> impl IntoResponse {
    match state.soul_store.add_trait(&soul_id, &req.name, req.value, &req.description) {
        Ok(trait_) => (StatusCode::OK, Json(trait_)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateSoulTraitRequest {
    value: Option<f64>,
    description: Option<String>,
}

async fn update_soul_trait(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSoulTraitRequest>,
) -> impl IntoResponse {
    match state.soul_store.update_trait(&id, req.value, req.description.as_deref()) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_soul_trait(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.soul_store.delete_trait(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct AddSoulMemoryRequest {
    memory_type: String,
    content: String,
    importance: f64,
}

async fn add_soul_memory(
    State(state): State<StoreAppState>,
    Path(soul_id): Path<String>,
    Json(req): Json<AddSoulMemoryRequest>,
) -> impl IntoResponse {
    match state.soul_store.add_memory(&soul_id, &req.memory_type, &req.content, req.importance) {
        Ok(memory) => (StatusCode::OK, Json(memory)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct GetSoulMemoriesQuery {
    memory_type: Option<String>,
    limit: Option<usize>,
}

async fn get_soul_memories(
    State(state): State<StoreAppState>,
    Path(soul_id): Path<String>,
    Query(query): Query<GetSoulMemoriesQuery>,
) -> impl IntoResponse {
    match state.soul_store.get_memories(&soul_id, query.memory_type.as_deref(), query.limit) {
        Ok(memories) => (StatusCode::OK, Json(memories)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn delete_soul_memory(
    State(state): State<StoreAppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.soul_store.delete_memory(&id) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}
