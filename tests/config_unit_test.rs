//! 单元测试：配置模块
//!
//! 测试配置结构体的创建、验证和默认值设置功能

use zeroclaw::config::{Config, SoulConfig, SecurityConfig, AgentConfig};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_config_default_values() {
    let config = Config::default();
    
    // 验证默认提供者和模型
    assert_eq!(config.default_provider, Some("openrouter".to_string()));
    assert_eq!(config.default_model, Some("anthropic/claude-sonnet-4".to_string()));
    assert_eq!(config.default_temperature, 0.7);
    
    // 验证灵魂配置默认启用
    assert!(config.soul.enabled);
    assert_eq!(config.soul.preset, "clara");
    
    // 验证工作空间目录设置
    assert!(config.workspace_dir.ends_with("workspace"));
}

#[test]
fn test_soul_config_defaults() {
    let soul_config = SoulConfig::default();
    
    assert!(soul_config.enabled);
    assert_eq!(soul_config.preset, "clara");
    assert!(soul_config.custom.is_none());
}

#[test]
fn test_soul_config_custom_preset() {
    let mut soul_config = SoulConfig::default();
    soul_config.preset = "zeroclaw".to_string();
    
    assert_eq!(soul_config.preset, "zeroclaw");
    assert!(soul_config.enabled);
}

#[test]
fn test_config_with_temp_workspace() {
    let temp_dir = tempdir().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    
    let mut config = Config::default();
    config.workspace_dir = workspace_path;
    
    // 验证配置可以正常序列化和反序列化
    let serialized = toml::to_string(&config).expect("Failed to serialize config");
    let deserialized: Config = toml::from_str(&serialized).expect("Failed to deserialize config");
    
    assert_eq!(config.default_provider, deserialized.default_provider);
    assert_eq!(config.soul.enabled, deserialized.soul.enabled);
    assert_eq!(config.soul.preset, deserialized.soul.preset);
}

#[test]
fn test_agent_config_creation() {
    let agent_config = AgentConfig {
        compact_context: true,
        max_tool_iterations: 10,
        max_tokens: Some(1000),
    };
    
    assert!(agent_config.compact_context);
    assert_eq!(agent_config.max_tool_iterations, 10);
    assert_eq!(agent_config.max_tokens, Some(1000));
}

#[test]
fn test_config_serialization_roundtrip() {
    let mut config = Config::default();
    config.default_provider = Some("anthropic".to_string());
    config.default_model = Some("claude-3-opus".to_string());
    config.default_temperature = 0.8;
    
    // 序列化
    let serialized = serde_json::to_string(&config).unwrap();
    
    // 反序列化
    let deserialized: Config = serde_json::from_str(&serialized).unwrap();
    
    // 验证值保持一致
    assert_eq!(config.default_provider, deserialized.default_provider);
    assert_eq!(config.default_model, deserialized.default_model);
    assert_eq!(config.default_temperature, deserialized.default_temperature);
    assert_eq!(config.soul.enabled, deserialized.soul.enabled);
    assert_eq!(config.soul.preset, deserialized.soul.preset);
}