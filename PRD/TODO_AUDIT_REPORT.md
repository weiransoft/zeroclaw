# ZeroClaw 代码 TODO 和临时代码审计报告

**审计日期**: 2026-02-23
**审计范围**: `/Users/wangwei/claw/zeroclaw/src`

---

## 一、TODO 任务清单

### 1. Gateway 模块 (`src/gateway/mod.rs`)

| 行号 | TODO 内容 | 严重程度 | 说明 |
|------|-----------|----------|------|
| 1090 | 实现阶段转换逻辑 | 🔴 高 | `handle_workflow_phase_transition` 仅返回模拟响应 |
| 2529 | 实现轨迹评估逻辑 | 🟡 中 | `handle_trace_evaluate` 需要接入评估系统 |
| 2637 | 实现告警系统 | 🟡 中 | `handle_alerts_list` 需要接入告警管理 |
| 2655 | 实现告警忽略逻辑 | 🟡 中 | `handle_alerts_acknowledge` 需要实现忽略功能 |
| 2670 | 实现失败模式分析 | 🟡 中 | `handle_failure_patterns` 需要接入分析系统 |

### 2. Providers 模块 (`src/providers/traced.rs`)

| 行号 | TODO 内容 | 严重程度 | 说明 |
|------|-----------|----------|------|
| 101 | 计算实际成本 | 🟢 低 | 成本计算目前返回 `None` |

---

## 二、模拟/临时代码实现

### 1. Gateway 测试模拟对象

**位置**: `src/gateway/mod.rs` (测试模块)

#### MockMemory (第 2856-2900 行)
```rust
#[derive(Default)]
struct MockMemory;

#[async_trait]
impl Memory for MockMemory {
    // 所有方法返回空实现
    async fn store(...) -> anyhow::Result<()> { Ok(()) }
    async fn recall(...) -> anyhow::Result<Vec<MemoryEntry>> { Ok(Vec::new()) }
    // ...
}
```

**用途**: 测试时模拟内存存储
**风险**: 低（仅用于测试）

#### MockProvider (第 2902-3100+ 行)
```rust
#[derive(Default)]
struct MockProvider {
    calls: AtomicUsize,
    response: String,
}

impl Provider for MockProvider {
    // 返回预设响应，不实际调用 LLM
}
```

**用途**: 测试时模拟 LLM 提供商
**风险**: 低（仅用于测试）

#### MockTool (第 3118-3135 行)
```rust
struct MockTool {
    result: serde_json::Value,
}

impl Tool for MockTool {
    fn name(&self) -> &str { "mock_tool" }
    async fn invoke(&self, _: &str) -> anyhow::Result<serde_json::Value> {
        Ok(self.result.clone())
    }
}
```

**用途**: 测试时模拟工具调用
**风险**: 低（仅用于测试）

### 2. Providers 测试模拟对象

**位置**: `src/providers/traced.rs` (测试模块)

#### MockProvider (第 333-380 行)
```rust
struct MockProvider {
    response: String,
}

impl Provider for MockProvider {
    // 返回固定响应
}
```

**用途**: 测试 traced provider 的装饰器功能
**风险**: 低（仅用于测试）

---

## 三、未实现的路由处理器

### Gateway 模块中已注释掉的 API 端点

**位置**: `src/gateway/mod.rs`

以下路由处理器已注释掉，需要实现：

#### 智能体团队 API
- `handle_agent_groups_list` - GET /agent-groups
- `handle_agent_groups_create` - POST /agent-groups
- `handle_agent_groups_get` - GET /agent-groups/{id}
- `handle_agent_groups_update` - PUT /agent-groups/{id}
- `handle_agent_groups_delete` - DELETE /agent-groups/{id}

#### 角色-智能体映射 API
- `handle_role_mappings_list` - GET /role-mappings
- `handle_role_mappings_create` - POST /role-mappings
- `handle_role_mappings_get` - GET /role-mappings/{role}
- `handle_role_mappings_update` - PUT /role-mappings/{role}
- `handle_role_mappings_delete` - DELETE /role-mappings/{role}

#### Swarm 智能体群聊 API
- `handle_swarm_tasks_list` - GET /swarm/tasks
- `handle_swarm_tasks_create` - POST /swarm/tasks
- `handle_swarm_tasks_get` - GET /swarm/tasks/{id}
- `handle_swarm_tasks_delete` - DELETE /swarm/tasks/{id}
- `handle_swarm_messages_list` - GET /swarm/tasks/{id}/messages
- `handle_swarm_messages_send` - POST /swarm/tasks/{id}/messages
- `handle_swarm_consensus_get` - GET /swarm/tasks/{id}/consensus
- `handle_swarm_consensus_vote` - POST /swarm/tasks/{id}/consensus

---

## 四、简化/占位实现

### 1. 工作流阶段转换

**位置**: `src/gateway/mod.rs:1090`

```rust
async fn handle_workflow_phase_transition(...) -> impl IntoResponse {
    // TODO: 实现阶段转换逻辑
    let result = serde_json::json!({
        "success": true, 
        "message": "Phase transition not yet implemented"
    });
    (StatusCode::OK, Json(result))
}
```

**问题**: 仅返回成功响应，不实际执行阶段转换
**建议**: 接入 WorkflowEngine 实现实际转换逻辑

### 2. 轨迹评估

**位置**: `src/gateway/mod.rs:2529`

```rust
async fn handle_trace_evaluate(...) -> impl IntoResponse {
    // TODO: 实现轨迹评估逻辑
    let result = serde_json::json!({
        "trace_id": id,
        "evaluated": false,
        "message": "Evaluation not yet implemented"
    });
    (StatusCode::OK, Json(result))
}
```

**问题**: 评估逻辑未实现
**建议**: 接入 EvaluationEngine 或 LLM 评估

### 3. 告警系统

**位置**: `src/gateway/mod.rs:2637-2670`

```rust
async fn handle_alerts_list(...) -> impl IntoResponse {
    // TODO: 实现告警系统
    let alerts: Vec<serde_json::Value> = vec![];
    (StatusCode::OK, Json(alerts))
}
```

**问题**: 告警系统完全未实现
**建议**: 需要设计并实现完整的告警管理模块

---

## 五、建议优先级

### 🔴 高优先级（影响核心功能）

1. **实现工作流阶段转换逻辑** - 影响工作流核心功能
2. **实现轨迹评估逻辑** - 影响可观测性

### 🟡 中优先级（影响完整功能）

3. **实现告警系统** - 影响运维能力
4. **实现 Swarm API 处理器** - 影响智能体群聊功能
5. **实现角色映射 API** - 影响角色管理

### 🟢 低优先级（测试代码，可保留）

6. MockMemory / MockProvider / MockTool - 仅用于测试，无需修改

---

## 六、代码质量建议

1. **添加 #[deprecated] 标记**: 对于临时实现，添加标记提醒开发者
2. **使用 feature flags**: 对于未实现功能，使用 feature flags 控制编译
3. **返回 501 Not Implemented**: 对于占位实现，返回标准 HTTP 状态码
4. **添加文档注释**: 明确说明哪些是临时实现

---

## 七、修复建议示例

### 对于占位实现，建议改为：

```rust
async fn handle_workflow_phase_transition(...) -> impl IntoResponse {
    // FIXME: 实现实际的工作流阶段转换逻辑
    // 参考: WorkflowEngine::transition_phase()
    let result = serde_json::json!({
        "success": false,
        "error": "NOT_IMPLEMENTED",
        "message": "Phase transition is not yet implemented. Please implement WorkflowEngine::transition_phase()"
    });
    (StatusCode::NOT_IMPLEMENTED, Json(result))
}
```

---

**报告生成完成**
