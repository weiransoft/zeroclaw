/// GUI Agent 使用示例
/// 
/// 本文件展示如何在实际项目中使用 GUI Agent 的多模态感知能力。

#[cfg(test)]
mod gui_agent_examples {
    
    /// 示例 1: 基本的屏幕理解
    /// 
    /// 展示如何使用多模态感知器进行屏幕理解
    #[tokio::test]
    async fn example_1_basic_screen_understanding() {
        // 注意：这是示例代码，展示 API 使用方式
        // 实际使用时需要创建真实的 Provider 实例
        
        use crate::gui::perceptor::{MultimodalPerceptor, MultimodalPerceptorConfig};
        use crate::gui::screen::image::analyzer::ImageAnalyzer;
        use crate::gui::screen::ocr::OcrClient;
        
        // 1. 准备组件（示例中使用占位符）
        // let provider = OllamaProvider::new("http://localhost:11434");
        // let memory = Arc::new(SqliteMemory::new("memory.db").await.unwrap());
        // let context = Arc::new(ContextManager::new());
        
        // 2. 创建多模态感知器
        // let perceptor = MultimodalPerceptor::new(
        //     provider,
        //     ImageAnalyzer::new(),
        //     OcrClient::new(),
        //     memory,
        //     context,
        //     MultimodalPerceptorConfig::default(),
        // );
        
        // 3. 捕获屏幕截图（由 desktop 端完成）
        // let screen_image = capture_screen();
        
        // 4. 进行屏幕理解
        // let understanding = perceptor.understand_screen(&screen_image).await.unwrap();
        
        // 5. 处理识别结果
        // println!("识别到 {} 个 UI 元素", understanding.elements.len());
        // for element in &understanding.elements {
        //     println!("  - {:?}: {} (置信度：{:.2})", 
        //              element.element_type, 
        //              element.description,
        //              element.confidence);
        // }
        
        // 断言：示例代码仅用于展示 API 使用
        assert!(true, "示例代码展示完毕");
    }
    
    /// 示例 2: 使用 Provider 进行屏幕理解
    /// 
    /// 展示如何使用 ZeroClaw 的 Provider trait
    #[tokio::test]
    async fn example_2_using_provider_trait() {
        use crate::gui::screen::image::llm::{understand_screen, find_ui_element};
        use crate::providers::Provider;
        
        // 1. 创建 Provider（示例中使用 Ollama）
        // let provider = OllamaProvider::new("http://localhost:11434");
        // let model = "qwen3.5:0.8b";
        
        // 2. 准备屏幕截图
        // let screen_image = capture_screen();
        
        // 3. 进行屏幕理解
        // let elements = understand_screen(&provider, model, &screen_image).await.unwrap();
        
        // 4. 查找特定元素
        // let button = find_ui_element(
        //     &provider,
        //     model,
        //     &screen_image,
        //     "提交按钮"
        // ).await.unwrap();
        
        // if let Some(element) = button {
        //     println!("找到提交按钮：位置 {:?}", element.bounding_box);
        // }
        
        // 断言：示例代码仅用于展示 API 使用
        assert!(true, "Provider trait 使用示例展示完毕");
    }
    
    /// 示例 3: 记忆系统集成
    /// 
    /// 展示如何将识别结果存储到记忆系统
    #[tokio::test]
    async fn example_3_memory_integration() {
        use crate::gui::perceptor::RecognitionResult;
        use crate::memory::{Memory, MemoryCategory};
        
        // 1. 创建记忆系统
        // let memory = Arc::new(SqliteMemory::new("memory.db").await.unwrap());
        
        // 2. 准备识别结果
        // let recognition_result = RecognitionResult {
        //     elements: vec![/* UI 元素 */],
        //     text_regions: vec![/* 文本区域 */],
        //     timestamp: chrono::Utc::now().timestamp(),
        //     confidence: 0.95,
        //     screen_hash: "abc123".to_string(),
        // };
        
        // 3. 存储到记忆系统
        // let key = format!("gui_recognition:{}", recognition_result.screen_hash);
        // memory.store(
        //     &key,
        //     &serde_json::to_string(&recognition_result).unwrap(),
        //     MemoryCategory::Custom("gui_recognition".to_string()),
        // ).await.unwrap();
        
        // 4. 从记忆系统检索
        // let results = memory.recall("登录页面", 10).await.unwrap();
        // for result in results {
        //     println!("找到相关记忆：{}", result.key);
        // }
        
        // 断言：示例代码仅用于展示 API 使用
        assert!(true, "记忆系统集成示例展示完毕");
    }
    
    /// 示例 4: 完整的 GUI Agent 工作流
    /// 
    /// 展示从屏幕截图到动作执行的完整流程
    #[tokio::test]
    async fn example_4_complete_workflow() {
        // 这是一个概念示例，展示完整的工作流
        
        // 步骤 1: Desktop 端捕获屏幕
        // let screen_image = desktop.capture_screen();
        
        // 步骤 2: 发送到后端进行识别
        // let understanding = perceptor.understand_screen(&screen_image).await?;
        
        // 步骤 3: 任务规划（识别登录表单）
        // let username_input = understanding.elements.iter()
        //     .find(|e| e.element_type == UiElementType::Input && 
        //               e.description.contains("用户名"));
        // let password_input = understanding.elements.iter()
        //     .find(|e| e.element_type == UiElementType::Input && 
        //               e.description.contains("密码"));
        // let submit_button = understanding.elements.iter()
        //     .find(|e| e.element_type == UiElementType::Button && 
        //               e.description.contains("登录"));
        
        // 步骤 4: 生成操作序列
        // let actions = vec![
        //     Action::Click { 
        //         x: username_input.bounding_box[0] + username_input.bounding_box[2] / 2,
        //         y: username_input.bounding_box[1] + username_input.bounding_box[3] / 2,
        //     },
        //     Action::Type { text: "user@example.com" },
        //     Action::Click { 
        //         x: password_input.bounding_box[0] + password_input.bounding_box[2] / 2,
        //         y: password_input.bounding_box[1] + password_input.bounding_box[3] / 2,
        //     },
        //     Action::Type { text: "password123" },
        //     Action::Click { 
        //         x: submit_button.bounding_box[0] + submit_button.bounding_box[2] / 2,
        //         y: submit_button.bounding_box[1] + submit_button.bounding_box[3] / 2,
        //     },
        // ];
        
        // 步骤 5: 发送操作指令给 Desktop 端执行
        // desktop.execute_actions(actions).await?;
        
        // 步骤 6: 等待执行结果并验证
        // let result = desktop.wait_for_completion().await?;
        // assert!(result.success);
        
        // 断言：示例代码仅用于展示完整工作流
        assert!(true, "完整工作流示例展示完毕");
    }
    
    /// 示例 5: 错误处理和重试
    /// 
    /// 展示如何处理识别失败和重试
    #[tokio::test]
    async fn example_5_error_handling_and_retry() {
        use crate::gui::perceptor::MultimodalPerceptorError;
        
        // 1. 创建感知器
        // let perceptor = create_perceptor();
        
        // 2. 准备截图
        // let screen_image = capture_screen();
        
        // 3. 带重试的识别
        // let max_retries = 3;
        // let mut attempt = 0;
        // let mut understanding = None;
        
        // while attempt < max_retries {
        //     match perceptor.understand_screen(&screen_image).await {
        //         Ok(result) => {
        //             understanding = Some(result);
        //             break;
        //         }
        //         Err(MultimodalPerceptorError::LlmInferenceFailed(e)) => {
        //             attempt += 1;
        //             if attempt >= max_retries {
        //                 eprintln!("LLM 推理失败，已达最大重试次数：{}", e);
        //                 return Err(anyhow::anyhow!("识别失败"));
        //             }
        //             // 等待一段时间后重试
        //             tokio::time::sleep(Duration::from_secs(1)).await;
        //         }
        //         Err(e) => {
        //             eprintln!("识别失败：{}", e);
        //             return Err(anyhow::anyhow!("识别失败：{}", e));
        //         }
        //     }
        // }
        
        // 4. 处理识别结果
        // let understanding = understanding.unwrap();
        // println!("识别成功：{} 个元素", understanding.elements.len());
        
        // 断言：示例代码仅用于展示错误处理
        assert!(true, "错误处理和重试示例展示完毕");
    }
    
    /// 示例 6: 性能优化 - 缓存识别结果
    /// 
    /// 展示如何缓存识别结果避免重复识别
    #[tokio::test]
    async fn example_6_caching_for_performance() {
        use std::collections::HashMap;
        use tokio::sync::RwLock;
        
        // 1. 创建缓存
        // let cache = Arc::new(RwLock::new(HashMap::new()));
        
        // 2. 计算屏幕哈希
        // let screen_hash = compute_hash(&screen_image);
        
        // 3. 检查缓存
        // {
        //     let cache_read = cache.read().await;
        //     if let Some(cached_result) = cache_read.get(&screen_hash) {
        //         // 使用缓存结果
        //         println!("使用缓存的识别结果");
        //         return cached_result.clone();
        //     }
        // }
        
        // 4. 进行识别
        // let understanding = perceptor.understand_screen(&screen_image).await?;
        
        // 5. 存入缓存
        // {
        //     let mut cache_write = cache.write().await;
        //     cache_write.insert(screen_hash, understanding.clone());
        // }
        
        // 断言：示例代码仅用于展示缓存优化
        assert!(true, "缓存优化示例展示完毕");
    }
}

/// 辅助函数示例
mod helper_examples {
    
    /// 计算图像哈希（用于缓存和去重）
    pub fn compute_image_hash(image: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        image.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
    
    /// 将 Base64 图像数据转换为字节数组
    pub fn base64_to_image(base64_data: &str) -> Result<Vec<u8>, base64::DecodeError> {
        use base64::{Engine, engine::GeneralPurpose, engine::general_purpose::PAD};
        GeneralPurpose::new(&base64::alphabet::STANDARD, PAD).decode(base64_data)
    }
    
    /// 将图像字节数组转换为 Base64
    pub fn image_to_base64(image: &[u8]) -> String {
        use base64::{Engine, engine::GeneralPurpose, engine::general_purpose::PAD};
        GeneralPurpose::new(&base64::alphabet::STANDARD, PAD).encode(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_helper_functions() {
        // 测试图像哈希计算
        let image_data = b"fake image data";
        let hash1 = helper_examples::compute_image_hash(image_data);
        let hash2 = helper_examples::compute_image_hash(image_data);
        assert_eq!(hash1, hash2, "相同图像应该有相同的哈希");
        
        // 测试 Base64 编解码
        let original = b"Hello, World!";
        let base64 = helper_examples::image_to_base64(original);
        let decoded = helper_examples::base64_to_image(&base64).unwrap();
        assert_eq!(original.to_vec(), decoded, "Base64 编解码应该可逆");
    }
}
