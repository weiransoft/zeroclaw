// Copyright 2026 ZeroClaw Project. All rights reserved.
// 向量存储接口和实现 - 用于上下文向量检索

use crate::memory::vector::cosine_similarity;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 向量嵌入类型
pub type Embedding = Vec<f32>;

/// 向量存储条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    /// 条目 ID
    pub id: String,
    /// 向量嵌入
    pub embedding: Embedding,
    /// 关联的文本内容
    pub content: String,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 创建时间戳
    pub timestamp: i64,
}

/// 向量存储接口
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// 存储后端名称
    fn name(&self) -> &str;

    /// 添加向量条目
    async fn add(&self, entry: VectorEntry) -> anyhow::Result<()>;

    /// 批量添加向量条目
    async fn add_batch(&self, entries: Vec<VectorEntry>) -> anyhow::Result<()>;

    /// 根据向量相似度搜索
    async fn search(
        &self,
        query_embedding: &Embedding,
        limit: usize,
        threshold: f32,
    ) -> anyhow::Result<Vec<(VectorEntry, f32)>>;

    /// 删除向量条目
    async fn delete(&self, id: &str) -> anyhow::Result<bool>;

    /// 获取所有条目数量
    async fn count(&self) -> anyhow::Result<usize>;

    /// 健康检查
    async fn health_check(&self) -> bool;
}

/// 内存向量存储实现（用于开发和测试）
pub struct InMemoryVectorStore {
    /// 存储名称
    name: String,
    /// 向量条目存储
    entries: Arc<RwLock<HashMap<String, VectorEntry>>>,
}

impl InMemoryVectorStore {
    /// 创建新的内存向量存储
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    fn name(&self) -> &str {
        &self.name
    }

    async fn add(&self, entry: VectorEntry) -> anyhow::Result<()> {
        let mut entries = self.entries.write().await;
        entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    async fn add_batch(&self, entries: Vec<VectorEntry>) -> anyhow::Result<()> {
        let mut store = self.entries.write().await;
        for entry in entries {
            store.insert(entry.id.clone(), entry);
        }
        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &Embedding,
        limit: usize,
        threshold: f32,
    ) -> anyhow::Result<Vec<(VectorEntry, f32)>> {
        let entries = self.entries.read().await;
        
        // 计算所有条目的相似度
        let mut scored_entries: Vec<(VectorEntry, f32)> = entries
            .values()
            .filter_map(|entry| {
                let similarity = cosine_similarity(&entry.embedding, query_embedding);
                if similarity >= threshold {
                    Some((entry.clone(), similarity))
                } else {
                    None
                }
            })
            .collect();

        // 按相似度降序排序
        scored_entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 返回前 limit 个结果
        scored_entries.truncate(limit);
        Ok(scored_entries)
    }

    async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let mut entries = self.entries.write().await;
        Ok(entries.remove(id).is_some())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let entries = self.entries.read().await;
        Ok(entries.len())
    }

    async fn health_check(&self) -> bool {
        true
    }
}

/// 向量检索器 - 用于上下文向量检索
pub struct ContextVectorRetriever {
    /// 向量存储
    vector_store: Arc<dyn VectorStore>,
    /// 默认搜索结果数量
    default_limit: usize,
    /// 默认相似度阈值
    default_threshold: f32,
}

impl ContextVectorRetriever {
    /// 创建新的向量检索器
    pub fn new(vector_store: Arc<dyn VectorStore>) -> Self {
        Self {
            vector_store,
            default_limit: 10,
            default_threshold: 0.5,
        }
    }

    /// 设置默认搜索结果数量
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.default_limit = limit;
        self
    }

    /// 设置默认相似度阈值
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.default_threshold = threshold;
        self
    }

    /// 根据查询向量检索相关上下文
    pub async fn retrieve(
        &self,
        query_embedding: &Embedding,
    ) -> anyhow::Result<Vec<(VectorEntry, f32)>> {
        self.retrieve_with_limit(query_embedding, self.default_limit)
            .await
    }

    /// 根据查询向量检索相关上下文（指定数量）
    pub async fn retrieve_with_limit(
        &self,
        query_embedding: &Embedding,
        limit: usize,
    ) -> anyhow::Result<Vec<(VectorEntry, f32)>> {
        self.vector_store
            .search(query_embedding, limit, self.default_threshold)
            .await
    }

    /// 添加上下文条目到向量存储
    pub async fn add_context(
        &self,
        id: &str,
        content: &str,
        embedding: Embedding,
        metadata: Option<HashMap<String, String>>,
    ) -> anyhow::Result<()> {
        let entry = VectorEntry {
            id: id.to_string(),
            embedding,
            content: content.to_string(),
            metadata: metadata.unwrap_or_default(),
            timestamp: chrono::Local::now().timestamp(),
        };

        self.vector_store.add(entry).await
    }

    /// 批量添加上下文条目
    pub async fn add_context_batch(
        &self,
        entries: Vec<(&str, &str, Embedding, Option<HashMap<String, String>>)>,
    ) -> anyhow::Result<()> {
        let vector_entries: Vec<VectorEntry> = entries
            .into_iter()
            .map(|(id, content, embedding, metadata)| VectorEntry {
                id: id.to_string(),
                embedding,
                content: content.to_string(),
                metadata: metadata.unwrap_or_default(),
                timestamp: chrono::Local::now().timestamp(),
            })
            .collect();

        self.vector_store.add_batch(vector_entries).await
    }

    /// 获取向量存储中的条目数量
    pub async fn count(&self) -> anyhow::Result<usize> {
        self.vector_store.count().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_vector_store() {
        let store = Arc::new(InMemoryVectorStore::new("test"));
        
        // 添加测试条目
        let entry = VectorEntry {
            id: "test-1".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            content: "Test content".to_string(),
            metadata: HashMap::new(),
            timestamp: chrono::Local::now().timestamp(),
        };
        
        store.add(entry).await.unwrap();
        
        // 搜索
        let results = store
            .search(&vec![1.0, 0.0, 0.0], 10, 0.0)
            .await
            .unwrap();
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.id, "test-1");
        assert!((results[0].1 - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_context_vector_retriever() {
        let store = Arc::new(InMemoryVectorStore::new("test"));
        let retriever = ContextVectorRetriever::new(store.clone());

        // 添加测试上下文
        retriever
            .add_context(
                "ctx-1",
                "Rust programming context",
                vec![1.0, 0.5, 0.0],
                None,
            )
            .await
            .unwrap();

        retriever
            .add_context(
                "ctx-2",
                "Python programming context",
                vec![0.0, 1.0, 0.5],
                None,
            )
            .await
            .unwrap();

        // 检索
        let results = retriever
            .retrieve(&vec![1.0, 0.5, 0.0])
            .await
            .unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].0.id, "ctx-1");
    }

    #[tokio::test]
    async fn test_vector_similarity_threshold() {
        let store = Arc::new(InMemoryVectorStore::new("test"));
        let retriever = ContextVectorRetriever::new(store.clone())
            .with_threshold(0.8);

        // 添加不相关的上下文
        retriever
            .add_context(
                "ctx-1",
                "Unrelated context",
                vec![0.0, 0.0, 1.0],
                None,
            )
            .await
            .unwrap();

        // 检索（应该没有结果，因为相似度低于阈值）
        let results = retriever
            .retrieve(&vec![1.0, 0.0, 0.0])
            .await
            .unwrap();

        assert!(results.is_empty());
    }
}
