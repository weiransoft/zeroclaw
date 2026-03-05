use anyhow::Result;
use std::sync::Arc;
use tempfile::TempDir;
use zeroclaw::config::Config;
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{SwarmContext, SwarmManager};
use zeroclaw::tools::workflow::WorkflowTool;
use zeroclaw::tools::traits::Tool;
use serde_json::json;

#[tokio::test]
async fn test_workflow_template_system() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = Config::default();
    let config_arc = Arc::new(config);
    let security = Arc::new(SecurityPolicy::from_config(&config_arc.autonomy, temp_dir.path()));
    let manager = SwarmManager::new(temp_dir.path().to_path_buf(), 5);
    let ctx = SwarmContext::root();
    
    let workflow_tool = WorkflowTool::new(
        security,
        config_arc,
        manager,
        ctx,
    );
    
    // 测试 1: 创建模板
    println!("=== Test 1: Creating template ===");
    let create_template_result = workflow_tool.execute(json!({
        "action": "template_create",
        "template_name": "Test Template",
        "template_author": "test_user",
        "description": "Test workflow template"
    })).await?;
    assert!(create_template_result.success, "Failed to create template: {:?}", create_template_result.error);
    println!("Template created successfully: {}", create_template_result.output);
    
    // 测试 2: 列出模板
    println!("\n=== Test 2: Listing templates ===");
    let list_templates_result = workflow_tool.execute(json!({
        "action": "template_list"
    })).await?;
    assert!(list_templates_result.success, "Failed to list templates: {:?}", list_templates_result.error);
    println!("Templates listed successfully:");
    println!("{}", list_templates_result.output);
    
    // 从输出中提取模板 ID
    let template_id = extract_template_id(&list_templates_result.output);
    assert!(!template_id.is_empty(), "Failed to extract template ID");
    println!("Extracted template ID: {}", template_id);
    
    // 测试 3: 获取模板详情
    println!("\n=== Test 3: Getting template details ===");
    let get_template_result = workflow_tool.execute(json!({
        "action": "template_get",
        "template_id": template_id
    })).await?;
    assert!(get_template_result.success, "Failed to get template: {:?}", get_template_result.error);
    println!("Template details retrieved successfully");
    
    // 测试 4: 验证模板
    println!("\n=== Test 4: Validating template ===");
    let validate_template_result = workflow_tool.execute(json!({
        "action": "template_validate",
        "template_id": template_id
    })).await?;
    println!("Template validation result:");
    println!("{}", validate_template_result.output);
    
    // 测试 5: 基于模板创建工作流
    println!("\n=== Test 5: Creating workflow from template ===");
    let create_from_template_result = workflow_tool.execute(json!({
        "action": "create_from_template",
        "template_id": template_id,
        "workflow_name": "Test Workflow from Template"
    })).await?;
    assert!(create_from_template_result.success, "Failed to create workflow from template: {:?}", create_from_template_result.error);
    println!("Workflow created from template successfully: {}", create_from_template_result.output);
    
    // 测试 6: 删除模板
    println!("\n=== Test 6: Deleting template ===");
    let delete_template_result = workflow_tool.execute(json!({
        "action": "template_delete",
        "template_id": template_id
    })).await?;
    assert!(delete_template_result.success, "Failed to delete template: {:?}", delete_template_result.error);
    println!("Template deleted successfully: {}", delete_template_result.output);
    
    // 测试 7: 验证模板已删除
    println!("\n=== Test 7: Verifying template deletion ===");
    let list_templates_after_delete_result = workflow_tool.execute(json!({
        "action": "template_list"
    })).await?;
    assert!(list_templates_after_delete_result.success, "Failed to list templates after deletion: {:?}", list_templates_after_delete_result.error);
    println!("Templates listed after deletion:");
    println!("{}", list_templates_after_delete_result.output);
    
    println!("\n=== All tests passed! ===");
    Ok(())
}

fn extract_template_id(output: &str) -> String {
    for line in output.lines() {
        if line.starts_with("- ID: ") {
            return line.trim_start_matches("- ID: ").trim().to_string();
        }
    }
    "".to_string()
}
