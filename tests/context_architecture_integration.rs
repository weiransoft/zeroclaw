// Copyright 2026 ZeroClaw Project. All rights reserved.
// 双层上下文架构集成测试

use zeroclaw::context::{
    ContextFilter, TaskType,
    GlobalContext, GlobalContextManager, InMemoryBackend, CacheConfig,
    TaskContextManager, TaskDefinition,
    ContextVectorRetriever, InMemoryVectorStore, VectorStore,
    SqliteContextStore,
};
use zeroclaw::context::llm_client::{LLMClient, LLMResponse, ChatMessage, ModelInfo, Result as LLMResult};
use zeroclaw::memory::none::NoneMemory;
use std::sync::Arc;
use std::env;
use std::time::Duration;

/// 简单的测试用 LLM 客户端
#[derive(Debug, Clone)]
struct TestLLMClient;

impl TestLLMClient {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl LLMClient for TestLLMClient {
    async fn generate(&self, _prompt: &str) -> LLMResult<LLMResponse> {
        Ok(LLMResponse {
            content: "test response".to_string(),
            model: "test".to_string(),
            usage: zeroclaw::context::llm_client::UsageStats {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            finish_reason: "stop".to_string(),
        })
    }

    async fn chat(&self, _messages: &[ChatMessage]) -> LLMResult<LLMResponse> {
        Ok(LLMResponse {
            content: "test chat response".to_string(),
            model: "test".to_string(),
            usage: zeroclaw::context::llm_client::UsageStats {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            finish_reason: "stop".to_string(),
        })
    }

    async fn embed(&self, _text: &str) -> LLMResult<Vec<f32>> {
        Ok(vec![0.0; 128])
    }

    fn get_model_info(&self) -> ModelInfo {
        ModelInfo {
            name: "test".to_string(),
            max_context_length: 2000,
            supports_embeddings: true,
            supports_chat: true,
        }
    }
}

/// 测试双层上下文架构的完整流程
#[tokio::test]
async fn test_dual_layer_context_architecture() {
    // 1. 创建全局上下文管理器
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 1000,
        ttl: Duration::from_secs(3600),
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let global_manager = Arc::new(GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    ));

    // 2. 初始化全局上下文
    let _global_ctx = global_manager
        .get_or_create("test-user")
        .await
        .unwrap();

    // 3. 更新全局上下文
    let updated_ctx = global_manager
        .update("test-user", |ctx| {
            ctx.user_profile = "喜欢 Rust 编程".to_string();
        })
        .await
        .unwrap();

    assert_eq!(updated_ctx.user_profile, "喜欢 Rust 编程");
    assert_eq!(updated_ctx.version, 2); // 初始版本为 1，更新后为 2

    // 4. 创建任务上下文管理器
    let task_def = TaskDefinition {
        task_id: "task-1".to_string(),
        task_type: TaskType::Technical,
        description: "实现 Rust 向量存储功能".to_string(),
        priority: 5,
        token_budget: 10000,
    };

    let memory_backend = Box::new(NoneMemory::new());
    let task_manager = TaskContextManager::new(
        task_def.clone(),
        global_manager.clone(),
        memory_backend,
    );

    // 5. 添加任务对话
    task_manager
        .add_conversation("用户", "我需要实现一个向量存储")
        .await;

    task_manager
        .add_conversation("助手", "好的，我会创建 VectorStore trait")
        .await;

    // 6. 添加中间结果
    task_manager
        .add_result("design", "设计了 VectorStore 接口")
        .await;

    // 7. 同步任务到全局上下文
    let sync_result = task_manager.sync_to_global().await;
    assert!(sync_result.is_ok());
}

/// 测试上下文过滤器
#[test]
fn test_context_filter() {
    let filter = ContextFilter::new();
    
    let global = GlobalContext {
        user_id: "test-user".to_string(),
        user_profile: "Rust 开发者".to_string(),
        domain_knowledge: "Rust, 系统编程".to_string(),
        historical_experience: "5 年编程经验".to_string(),
        version: 1,
        last_updated: chrono::Local::now(),
    };

    // 测试按任务类型过滤
    let filtered = filter.filter_by_task_type(&global, &TaskType::Technical);
    
    // 验证过滤后的上下文仍然存在
    assert!(!filtered.user_id.is_empty());
}

/// 测试向量检索集成
#[tokio::test]
async fn test_vector_retrieval_integration() {
    // 创建向量存储
    let vector_store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new("test"));
    let retriever = ContextVectorRetriever::new(vector_store)
        .with_limit(5)
        .with_threshold(0.5);

    // 添加上下文向量
    retriever
        .add_context(
            "ctx-1",
            "Rust 编程语言基础",
            vec![1.0, 0.8, 0.2],
            None,
        )
        .await
        .unwrap();

    retriever
        .add_context(
            "ctx-2",
            "Python 机器学习",
            vec![0.2, 1.0, 0.8],
            None,
        )
        .await
        .unwrap();

    retriever
        .add_context(
            "ctx-3",
            "系统架构设计",
            vec![0.8, 0.2, 1.0],
            None,
        )
        .await
        .unwrap();

    // 检索相关上下文
    let results = retriever
        .retrieve(&vec![1.0, 0.8, 0.2])
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].0.id, "ctx-1");
    assert!(results[0].1 > 0.5);
}

/// 测试版本控制集成
#[tokio::test]
async fn test_version_control_integration() {
    let temp_dir = env::temp_dir().join(format!("test_version_{}", std::process::id()));
    let version_store = SqliteContextStore::new(&temp_dir).unwrap();

    // 创建全局上下文
    let mut context = GlobalContext {
        user_id: "version-test-user".to_string(),
        user_profile: "初始配置".to_string(),
        domain_knowledge: "基础知识".to_string(),
        historical_experience: "初始经验".to_string(),
        version: 1,
        last_updated: chrono::Local::now(),
    };

    // 保存版本 1
    version_store
        .save_global_version(&context, 1, Some("初始版本"))
        .unwrap();

    // 更新并保存版本 2
    context.user_profile = "更新的配置".to_string();
    context.version = 2;
    context.last_updated = chrono::Local::now();
    version_store
        .save_global_version(&context, 2, Some("更新配置"))
        .unwrap();

    // 检索版本 1
    let version_1 = version_store
        .get_global_version("version-test-user", 1)
        .unwrap()
        .unwrap();

    assert_eq!(version_1.user_profile, "初始配置");
    assert_eq!(version_1.version, 1);

    // 检索版本 2
    let version_2 = version_store
        .get_global_version("version-test-user", 2)
        .unwrap()
        .unwrap();

    assert_eq!(version_2.user_profile, "更新的配置");
    assert_eq!(version_2.version, 2);

    // 获取版本历史
    let history = version_store
        .get_version_history("global", "version-test-user")
        .unwrap();

    assert_eq!(history.len(), 2);
}

/// 测试完整的上下文管理工作流
#[tokio::test]
async fn test_complete_context_workflow() {
    // 1. 初始化所有组件
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 1000,
        ttl: Duration::from_secs(3600),
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let global_manager = Arc::new(GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    ));

    let temp_dir = env::temp_dir().join(format!("test_workflow_{}", std::process::id()));
    let version_store = SqliteContextStore::new(&temp_dir).unwrap();

    let vector_store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new("workflow"));
    let vector_retriever = ContextVectorRetriever::new(vector_store);

    // 2. 创建用户全局上下文
    global_manager
        .get_or_create("workflow-user")
        .await
        .unwrap();

    // 3. 创建任务
    let task_def = TaskDefinition {
        task_id: "workflow-task-1".to_string(),
        task_type: TaskType::Complex,
        description: "开发分布式系统".to_string(),
        priority: 8,
        token_budget: 20000,
    };

    let memory_backend = Box::new(NoneMemory::new());
    let task_manager = TaskContextManager::new(
        task_def,
        global_manager.clone(),
        memory_backend,
    );

    // 4. 模拟任务执行过程
    task_manager
        .add_conversation("用户", "需要设计一个分布式架构")
        .await;

    task_manager
        .add_conversation("助手", "建议使用微服务架构")
        .await;

    task_manager
        .add_result("architecture", "微服务设计文档")
        .await;

    // 5. 保存任务上下文到向量存储
    let task_ctx = task_manager.get_context().await;
    vector_retriever
        .add_context(
            &task_ctx.task_id,
            &task_ctx.task_definition.description,
            vec![0.7, 0.8, 0.9],
            None,
        )
        .await
        .unwrap();

    // 6. 同步到全局上下文
    task_manager
        .sync_to_global()
        .await
        .unwrap();

    // 7. 获取全局上下文并保存版本
    let global_ctx = global_manager.get_or_create("workflow-user").await.unwrap();
    version_store
        .save_global_version(&global_ctx, 1, Some("工作流测试"))
        .unwrap();

    // 8. 验证所有组件正常工作
    let updated_global = global_manager.get_or_create("workflow-user").await.unwrap();
    assert!(!updated_global.historical_experience.is_empty());

    let version_count = version_store
        .get_version_history("global", "workflow-user")
        .unwrap()
        .len();
    assert_eq!(version_count, 1);

    let vector_count = vector_retriever.count().await.unwrap();
    assert_eq!(vector_count, 1);
}
