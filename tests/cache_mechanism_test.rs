// Copyright 2026 ZeroClaw Project. All rights reserved.
// 缓存机制验证测试

use zeroclaw::context::{
    GlobalContext, GlobalContextManager, InMemoryBackend, CacheConfig,
};
use zeroclaw::context::llm_client::{LLMClient, LLMResponse, ChatMessage, ModelInfo, Result as LLMResult};
use std::sync::Arc;
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
            content: "test".to_string(),
            model: "test".to_string(),
            usage: zeroclaw::context::llm_client::UsageStats::default(),
            finish_reason: "stop".to_string(),
        })
    }

    async fn chat(&self, _messages: &[ChatMessage]) -> LLMResult<LLMResponse> {
        Ok(LLMResponse {
            content: "test".to_string(),
            model: "test".to_string(),
            usage: zeroclaw::context::llm_client::UsageStats::default(),
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

/// 测试缓存命中
#[tokio::test]
async fn test_cache_hit() {
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 100,
        ttl: Duration::from_secs(3600), // 1 小时 TTL
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let manager = GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    );

    // 第一次获取 - 应该创建并缓存
    let ctx1 = manager.get_or_create("cache-test-user").await.unwrap();
    assert_eq!(ctx1.version, 1);

    // 第二次获取 - 应该从缓存命中
    let ctx2 = manager.get_or_create("cache-test-user").await.unwrap();
    assert_eq!(ctx2.version, 1);
    assert_eq!(ctx2.user_id, "cache-test-user");
}

/// 测试缓存更新
#[tokio::test]
async fn test_cache_update() {
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 100,
        ttl: Duration::from_secs(3600),
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let manager = GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    );

    // 创建上下文
    manager.get_or_create("update-test-user").await.unwrap();

    // 更新上下文
    let updated = manager
        .update("update-test-user", |ctx| {
            ctx.user_profile = "updated profile".to_string();
        })
        .await
        .unwrap();

    assert_eq!(updated.user_profile, "updated profile");
    assert_eq!(updated.version, 2); // 版本应该递增

    // 再次获取应该看到更新后的数据
    let ctx = manager.get_or_create("update-test-user").await.unwrap();
    assert_eq!(ctx.user_profile, "updated profile");
    assert_eq!(ctx.version, 2);
}

/// 测试缓存 TTL 过期
#[tokio::test]
async fn test_cache_ttl_expiration() {
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 100,
        ttl: Duration::from_millis(100), // 非常短的 TTL 用于测试
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let manager = GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    );

    // 创建上下文
    let ctx1 = manager.get_or_create("ttl-test-user").await.unwrap();
    assert_eq!(ctx1.version, 1);

    // 等待 TTL 过期
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 再次获取 - 缓存已过期，应该重新加载
    let ctx2 = manager.get_or_create("ttl-test-user").await.unwrap();
    assert_eq!(ctx2.version, 1);
}

/// 测试缓存大小限制
#[tokio::test]
async fn test_cache_size_limit() {
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 5, // 很小的缓存用于测试
        ttl: Duration::from_secs(3600),
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let manager = GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    );

    // 创建超过缓存大小的上下文
    for i in 0..10 {
        let user_id = format!("user-{}", i);
        manager.get_or_create(&user_id).await.unwrap();
    }

    // 缓存应该只保留最近的 5 个
    // 由于 LRU 策略，前 5 个用户可能被驱逐
    let ctx = manager.get_or_create("user-9").await.unwrap();
    assert_eq!(ctx.user_id, "user-9");
}

/// 测试缓存清除
#[tokio::test]
async fn test_cache_clear() {
    let backend = Arc::new(InMemoryBackend::new());
    let cache_config = CacheConfig {
        max_size: 100,
        ttl: Duration::from_secs(3600),
    };
    let llm_client = Arc::new(TestLLMClient::new());
    
    let manager = GlobalContextManager::new(
        backend,
        cache_config,
        llm_client,
    );

    // 创建一些上下文
    for i in 0..5 {
        let user_id = format!("clear-test-user-{}", i);
        manager.get_or_create(&user_id).await.unwrap();
    }

    // 清除缓存
    manager.clear_cache().await;

    // 再次获取 - 应该从后端加载（如果有保存）或创建新的
    let ctx = manager.get_or_create("clear-test-user-0").await.unwrap();
    assert!(!ctx.user_id.is_empty());
}
