//! 单元测试：工具模块
//!
//! 测试工具相关功能，包括工具特性、具体工具实现、工具结果等

use zeroclaw::tools::{Tool, ToolResult, ToolSpec};
use zeroclaw::tools::shell::ShellTool;
use zeroclaw::security::SecurityPolicy;
use zeroclaw::runtime::RuntimeAdapter;
use std::sync::Arc;

use std::path::{Path, PathBuf};

// Mock RuntimeAdapter for testing
struct MockRuntimeAdapter;

impl RuntimeAdapter for MockRuntimeAdapter {
    fn name(&self) -> &str {
        "mock_runtime"
    }

    fn has_shell_access(&self) -> bool {
        true
    }

    fn has_filesystem_access(&self) -> bool {
        true
    }

    fn storage_path(&self) -> PathBuf {
        PathBuf::from("/tmp/mock_storage")
    }

    fn supports_long_running(&self) -> bool {
        false
    }

    fn build_shell_command(
        &self,
        command: &str,
        workspace_dir: &Path,
    ) -> anyhow::Result<tokio::process::Command> {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(workspace_dir);
        Ok(cmd)
    }
}

#[tokio::test]
async fn test_tool_result_creation() {
    // 测试工具结果的创建
    let success_result = ToolResult {
        success: true,
        output: "Success output".to_string(),
        error: None,
    };
    
    assert!(success_result.success);
    assert_eq!(success_result.output, "Success output");
    assert!(success_result.error.is_none());
    
    let error_result = ToolResult {
        success: false,
        output: "".to_string(),
        error: Some("Error message".to_string()),
    };
    
    assert!(!error_result.success);
    assert_eq!(error_result.output, "");
    assert_eq!(error_result.error, Some("Error message".to_string()));
}

#[tokio::test]
async fn test_tool_spec_creation() {
    // 测试工具规格的创建
    let spec = ToolSpec {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "param1": { "type": "string" }
            }
        }),
    };
    
    assert_eq!(spec.name, "test_tool");
    assert_eq!(spec.description, "A test tool");
    assert!(spec.parameters.is_object());
}

#[tokio::test]
async fn test_shell_tool_creation() {
    // 测试Shell工具的创建
    let security = Arc::new(SecurityPolicy::default());
    let runtime = Arc::new(MockRuntimeAdapter);
    
    let shell_tool = ShellTool::new(security, runtime);
    
    // 验证工具名称和描述
    assert_eq!(shell_tool.name(), "shell");
    assert_eq!(shell_tool.description(), "Execute a shell command in the workspace directory");
}

#[tokio::test]
async fn test_shell_tool_parameters_schema() {
    // 测试Shell工具的参数模式
    let security = Arc::new(SecurityPolicy::default());
    let runtime = Arc::new(MockRuntimeAdapter);
    
    let shell_tool = ShellTool::new(security, runtime);
    let schema = shell_tool.parameters_schema();
    
    // 验证模式结构
    assert!(schema.is_object());
    assert!(schema.get("type").unwrap().as_str().unwrap() == "object");
    
    let properties = schema.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("command"));
    assert!(properties.contains_key("approved"));
    
    let command_prop = properties.get("command").unwrap().as_object().unwrap();
    assert!(command_prop.get("type").unwrap().as_str().unwrap() == "string");
    
    let approved_prop = properties.get("approved").unwrap().as_object().unwrap();
    assert!(approved_prop.get("type").unwrap().as_str().unwrap() == "boolean");
}

#[tokio::test]
async fn test_shell_tool_spec() {
    // 测试Shell工具的规格
    let security = Arc::new(SecurityPolicy::default());
    let runtime = Arc::new(MockRuntimeAdapter);
    
    let shell_tool = ShellTool::new(security, runtime);
    let spec = shell_tool.spec();
    
    assert_eq!(spec.name, "shell");
    assert_eq!(spec.description, "Execute a shell command in the workspace directory");
    assert!(spec.parameters.is_object());
}

#[tokio::test]
async fn test_security_policy_tool_integration() {
    // 测试安全策略与工具的集成
    let policy = SecurityPolicy::default();
    
    // 验证默认安全策略
    assert_eq!(policy.allowed_commands.len(), 12); // git, npm, cargo, ls, cat, grep, find, echo, pwd, wc, head, tail
    assert_eq!(policy.forbidden_paths.len(), 18); // 系统目录和敏感路径
    assert!(policy.require_approval_for_medium_risk);
    assert!(policy.block_high_risk_commands);
}

#[tokio::test]
async fn test_tool_trait_implementation() {
    // 测试工具特性的实现
    let security = Arc::new(SecurityPolicy::default());
    let runtime = Arc::new(MockRuntimeAdapter);
    
    let shell_tool = ShellTool::new(security, runtime);
    
    // 验证基本方法
    assert_eq!(shell_tool.name(), "shell");
    assert!(!shell_tool.description().is_empty());
    
    // 验证参数模式结构
    let schema = shell_tool.parameters_schema();
    assert!(schema.is_object());
    assert!(schema.get("type").is_some());
}

#[tokio::test]
async fn test_tool_result_serialization() {
    // 测试工具结果的序列化
    let result = ToolResult {
        success: true,
        output: "serialized output".to_string(),
        error: Some("test error".to_string()),
    };
    
    // 序列化和反序列化
    let serialized = serde_json::to_string(&result).unwrap();
    let deserialized: ToolResult = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.success, result.success);
    assert_eq!(deserialized.output, result.output);
    assert_eq!(deserialized.error, result.error);
}

#[tokio::test]
async fn test_tool_spec_serialization() {
    // 测试工具规格的序列化
    let spec = ToolSpec {
        name: "serialize_test".to_string(),
        description: "Test serialization".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "test_param": { "type": "string" }
            }
        }),
    };
    
    // 序列化和反序列化
    let serialized = serde_json::to_string(&spec).unwrap();
    let deserialized: ToolSpec = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.name, spec.name);
    assert_eq!(deserialized.description, spec.description);
    assert_eq!(deserialized.parameters, spec.parameters);
}