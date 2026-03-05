//! 单元测试：安全模块
//!
//! 测试安全相关功能，包括审计日志、沙箱机制、配对验证、安全策略等功能

use zeroclaw::security::{SecurityPolicy, AutonomyLevel, SecretStore, PairingGuard};
use zeroclaw::security::audit::{AuditEvent, AuditEventType, Actor, Action, ExecutionResult, SecurityContext, AuditLogger};
use zeroclaw::config::AuditConfig;
use tempfile::tempdir;
use std::collections::HashMap;

#[test]
fn test_security_policy_default() {
    // 测试安全策略的默认值
    let policy = SecurityPolicy::default();
    
    assert_eq!(policy.autonomy, AutonomyLevel::Supervised);
    assert_eq!(policy.workspace_dir, std::path::PathBuf::from("."));
    assert_eq!(policy.workspace_only, true);
    assert_eq!(policy.allowed_commands.len(), 12); // git, npm, cargo, ls, cat, grep, find, echo, pwd, wc, head, tail
    assert_eq!(policy.forbidden_paths.len(), 18); // 系统目录和敏感路径
    assert_eq!(policy.max_actions_per_hour, 20);
    assert_eq!(policy.max_cost_per_day_cents, 500);
    assert_eq!(policy.require_approval_for_medium_risk, true);
    assert_eq!(policy.block_high_risk_commands, true);
}

#[test]
fn test_autonomy_levels() {
    // 测试自主级别枚举
    let read_only = AutonomyLevel::ReadOnly;
    let supervised = AutonomyLevel::Supervised;
    let full = AutonomyLevel::Full;
    
    assert!(matches!(read_only, AutonomyLevel::ReadOnly));
    assert!(matches!(supervised, AutonomyLevel::Supervised));
    assert!(matches!(full, AutonomyLevel::Full));
}

#[test]
fn test_audit_event_creation() {
    // 测试审计事件创建
    let mut event = AuditEvent::new(AuditEventType::CommandExecution);
    event.actor = Some(Actor {
        channel: "test-channel".to_string(),
        user_id: Some("test-user".to_string()),
        username: Some("test-username".to_string()),
    });
    
    assert!(matches!(event.event_type, AuditEventType::CommandExecution));
    assert!(event.actor.is_some());
    assert_eq!(event.actor.as_ref().unwrap().channel, "test-channel");
}

#[test]
fn test_audit_event_types() {
    // 测试不同类型的审计事件
    let command_event = AuditEventType::CommandExecution;
    let file_event = AuditEventType::FileAccess;
    let config_event = AuditEventType::ConfigChange;
    let auth_success = AuditEventType::AuthSuccess;
    let auth_failure = AuditEventType::AuthFailure;
    let violation_event = AuditEventType::PolicyViolation;
    let security_event = AuditEventType::SecurityEvent;
    
    assert!(matches!(command_event, AuditEventType::CommandExecution));
    assert!(matches!(file_event, AuditEventType::FileAccess));
    assert!(matches!(config_event, AuditEventType::ConfigChange));
    assert!(matches!(auth_success, AuditEventType::AuthSuccess));
    assert!(matches!(auth_failure, AuditEventType::AuthFailure));
    assert!(matches!(violation_event, AuditEventType::PolicyViolation));
    assert!(matches!(security_event, AuditEventType::SecurityEvent));
}

#[test]
fn test_secret_store_encryption_decryption() {
    // 测试密钥存储的加密解密功能
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path(), true); // 启用加密
    
    let secret_data = "my-secret-value";
    let encrypted = store.encrypt(secret_data).expect("Encryption should succeed");
    let decrypted = store.decrypt(&encrypted).expect("Decryption should succeed");
    
    assert_eq!(decrypted, secret_data);
    assert!(encrypted.starts_with("enc2:")); // 加密后的数据应该有前缀
}

#[test]
fn test_secret_store_different_inputs_produce_different_outputs() {
    // 测试不同的输入产生不同的加密输出
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path(), true);
    
    let secret1 = "secret-one";
    let secret2 = "secret-two";
    
    let encrypted1 = store.encrypt(secret1).expect("Encryption should succeed");
    let encrypted2 = store.encrypt(secret2).expect("Encryption should succeed");
    
    assert_ne!(encrypted1, encrypted2);
    assert_ne!(encrypted1, secret1);
    assert_ne!(encrypted2, secret2);
}

#[test]
fn test_secret_store_with_disabled_encryption() {
    // 测试禁用加密的情况
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path(), false); // 禁用加密
    
    let secret = "plain-text-secret";
    let encrypted = store.encrypt(secret).expect("Encryption should return plain text");
    let decrypted = store.decrypt(&encrypted).expect("Decryption should return plain text");
    
    assert_eq!(encrypted, secret);
    assert_eq!(decrypted, secret);
}

#[test]
fn test_pairing_guard_creation() {
    // 测试配对守卫的创建和功能
    let guard = PairingGuard::new(false, &[]); // 不需要配对，无现有令牌
    
    assert!(!guard.require_pairing());
}

#[test]
fn test_pairing_guard_with_pairing_required() {
    // 测试需要配对的情况
    let guard = PairingGuard::new(true, &[]); // 需要配对，无现有令牌
    
    assert!(guard.require_pairing());
    
    // 当没有现有令牌且需要配对时，应生成配对码
    let pairing_code = guard.pairing_code();
    assert!(pairing_code.is_some());
}

#[test]
fn test_pairing_guard_with_existing_tokens() {
    // 测试带有现有令牌的配对守卫
    let existing_tokens = vec!["existing-token".to_string()];
    let guard = PairingGuard::new(true, &existing_tokens);
    
    assert!(guard.require_pairing());
    
    // 当已有令牌存在时，不应生成新的配对码
    let pairing_code = guard.pairing_code();
    assert!(pairing_code.is_none());
}

#[test]
fn test_security_policy_settings() {
    // 测试安全策略的各种设置
    let mut policy = SecurityPolicy::default();
    policy.autonomy = AutonomyLevel::Full;
    policy.workspace_only = true;
    policy.allowed_commands = vec!["ls".to_string(), "cat".to_string()];
    policy.forbidden_paths = vec!["/etc".to_string(), "/root".to_string()];
    policy.max_actions_per_hour = 50;
    policy.max_cost_per_day_cents = 500;
    policy.require_approval_for_medium_risk = false;
    policy.block_high_risk_commands = false;
    
    assert_eq!(policy.autonomy, AutonomyLevel::Full);
    assert!(policy.workspace_only);
    assert_eq!(policy.allowed_commands.len(), 2);
    assert_eq!(policy.forbidden_paths.len(), 2);
    assert_eq!(policy.max_actions_per_hour, 50);
    assert_eq!(policy.max_cost_per_day_cents, 500);
    assert!(!policy.require_approval_for_medium_risk);
    assert!(!policy.block_high_risk_commands);
}

#[test]
fn test_audit_logger_creation() {
    // 测试审计日志记录器创建
    let temp_dir = tempdir().unwrap();
    let config = AuditConfig::default();
    let logger = AuditLogger::new(config, temp_dir.path().to_path_buf()).expect("AuditLogger should be created");
    
    // 记录一个事件
    let event = AuditEvent::new(AuditEventType::CommandExecution);
    logger.log(&event).expect("Logging should succeed");
    
    // 验证日志文件被创建
    let log_file_path = temp_dir.path().join("audit.log");
    assert!(log_file_path.exists());
    
    // 读取日志内容
    let log_content = std::fs::read_to_string(&log_file_path).expect("Log file should be readable");
    assert!(log_content.contains("command_execution"));
}