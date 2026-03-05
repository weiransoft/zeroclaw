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
async fn test_scrum_workflow_template() -> Result<()> {
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
    
    // 创建 Scrum 工作流模板
    println!("=== Creating Scrum Workflow Template ===");
    let create_template_result = workflow_tool.execute(json!({
        "action": "template_create",
        "template_name": "Scrum Sprint Workflow",
        "template_author": "system",
        "description": "Scrum workflow template with roles, ceremonies, and activities"
    })).await?;
    assert!(create_template_result.success, "Failed to create Scrum template: {:?}", create_template_result.error);
    println!("Scrum template created successfully: {}", create_template_result.output);
    
    // 列出模板，确认创建成功
    println!("\n=== Listing Templates ===");
    let list_templates_result = workflow_tool.execute(json!({
        "action": "template_list"
    })).await?;
    assert!(list_templates_result.success, "Failed to list templates: {:?}", list_templates_result.error);
    println!("Templates:");
    println!("{}", list_templates_result.output);
    
    // 从输出中提取模板 ID
    let template_id = extract_template_id(&list_templates_result.output);
    assert!(!template_id.is_empty(), "Failed to extract template ID");
    println!("Extracted Scrum template ID: {}", template_id);
    
    // 使用模板创建工作流
    println!("\n=== Creating Workflow from Scrum Template ===");
    let create_workflow_result = workflow_tool.execute(json!({
        "action": "create_from_template",
        "template_id": template_id,
        "workflow_name": "Spring 2026 Product Sprint"
    })).await?;
    assert!(create_workflow_result.success, "Failed to create workflow from Scrum template: {:?}", create_workflow_result.error);
    println!("Workflow created from Scrum template successfully: {}", create_workflow_result.output);
    
    // 测试生成模板变体
    println!("\n=== Generating Template Variant ===");
    let generate_variant_result = workflow_tool.execute(json!({
        "action": "template_generate",
        "prompt": "Create a Scrum workflow template optimized for remote teams with daily standups via video conferencing",
        "requester": "test_user"
    })).await?;
    assert!(generate_variant_result.success, "Failed to generate template variant: {:?}", generate_variant_result.error);
    println!("Template variant generated successfully: {}", generate_variant_result.output);
    
    println!("\n=== All Scrum workflow template tests passed! ===");
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
