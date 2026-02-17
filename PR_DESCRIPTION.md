# 智能体群组功能实现

## 功能概述

本 PR 实现了 ZeroClaw 的智能体群组功能，允许多个专业智能体协作完成复杂任务。

## 主要功能

### 1. 智能体群组架构

- **SwarmManager**：智能体群组管理器
- **LaneQueue**：基于队列的并发控制
- **SwarmSqliteStore**：SQLite 持久化存储
- **SwarmChatManager**：智能体间通信管理
- **ConsensusManager**：共识机制管理
- **TaskDependencyManager**：任务依赖管理

### 2. 智能体通信

- **SwarmChatMessage**：智能体间消息类型
- **ChatMessageType**：消息类型（文本、工具调用、状态更新等）
- **ConsensusState**：共识状态
- **SwarmChatManager**：聊天管理器

### 3. 共识机制

- **ConsensusManager**：共识管理器
- **ConsensusProposal**：共识提案
- **TaskConsensus**：任务共识
- **ConsensusStatus**：共识状态
- **投票和异议处理**：智能体投票和异议处理
- **澄清工作流**：异议澄清和共识达成

### 4. 任务依赖管理

- **TaskDependencyManager**：任务依赖管理器
- **DependencyGraph**：依赖图
- **TaskNode**：任务节点
- **DependencyEdge**：依赖边
- **DependencyType**：依赖类型（顺序、并行、条件等）
- **TaskDependencyStatus**：任务依赖状态
- **关键路径分析**：计算任务关键路径
- **依赖分析**：分析任务依赖关系

### 5. 可观测性

- **ProgressTraceManager**：进度跟踪管理器
- **ProgressEntry**：进度条目
- **TraceEntry**：跟踪条目
- **ProgressStatus**：进度状态
- **ExportFilter**：导出过滤器
- **ExportResult**：导出结果
- **JSON 和 CSV 导出**：支持多种导出格式

### 6. 子智能体管理

- **SubagentsTool**：子智能体管理工具
- **SubagentRun**：子智能体运行
- **SessionsSpawnTool**：会话生成工具
- **DelegateTool**：委托工具

### 7. 配置支持

- **智能体配置**：支持多个专业智能体配置
- **最大工具迭代次数**：可配置的最大工具迭代次数
- **温度和深度**：每个智能体可配置温度和最大深度

## 配置示例

```toml
[agent]
max_tool_iterations = 50

[agents.tech_lead]
provider = "glm"
model = "glm-5"
system_prompt = "You are a Technical Lead and Software Architect..."
temperature = 0.3
max_depth = 5

[agents.backend_dev]
provider = "glm"
model = "glm-5"
system_prompt = "You are a Backend Developer..."
temperature = 0.2
max_depth = 4

[agents.frontend_dev]
provider = "glm"
model = "glm-5"
system_prompt = "You are a Frontend Developer..."
temperature = 0.3
max_depth = 4
```

## 数据库模式

### swarm_subagent_runs

```sql
CREATE TABLE IF NOT EXISTS swarm_subagent_runs (
    run_id TEXT PRIMARY KEY,
    agent_name TEXT NOT NULL,
    task TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at_unix INTEGER NOT NULL,
    completed_at_unix INTEGER,
    depth INTEGER NOT NULL DEFAULT 0,
    label TEXT,
    output TEXT,
    error TEXT
);
```

### swarm_chat_messages

```sql
CREATE TABLE IF NOT EXISTS swarm_chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    message_type TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp_unix INTEGER NOT NULL,
    FOREIGN KEY (run_id) REFERENCES swarm_subagent_runs(run_id)
);
```

### swarm_consensus

```sql
CREATE TABLE IF NOT EXISTS swarm_consensus (
    consensus_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL,
    completed_at_unix INTEGER,
    proposal TEXT,
    votes TEXT,
    disagreements TEXT,
    resolution TEXT
);
```

### swarm_task_dependencies

```sql
CREATE TABLE IF NOT EXISTS swarm_task_dependencies (
    dependency_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    depends_on TEXT NOT NULL,
    dependency_type TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL,
    completed_at_unix INTEGER
);
```

### swarm_progress

```sql
CREATE TABLE IF NOT EXISTS swarm_progress (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    task_id TEXT,
    status TEXT NOT NULL,
    progress_percent REAL,
    message TEXT,
    timestamp_unix INTEGER NOT NULL,
    metadata TEXT,
    FOREIGN KEY (run_id) REFERENCES swarm_subagent_runs(run_id)
);
```

### swarm_traces

```sql
CREATE TABLE IF NOT EXISTS swarm_traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    timestamp_unix INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES swarm_subagent_runs(run_id)
);
```

## 测试

### 单元测试

- `tests/agent_swarm_flow.rs`：智能体群组流程测试
- `tests/swarm_queue.rs`：队列测试
- `tests/swarm_registry_load.rs`：注册表加载测试

### 集成测试

- 智能体群组协作测试
- 共识机制测试
- 任务依赖管理测试
- 可观测性测试

## 文档

- `docs/agent-swarm.md`：智能体群组使用手册
- `AGENTS.md`：智能体配置参考

## 使用示例

### 基本使用

```bash
# 使用单个智能体
zeroclaw agent --message "请使用 delegate 工具让 tech_lead 设计系统架构"

# 使用多个智能体协作
zeroclaw agent --message "请使用 delegate 工具协调团队开发一个简单的 To-Do List 应用，包括后端 API 和前端界面"
```

### 高级使用

```bash
# 使用子智能体管理工具
zeroclaw agent --message "请使用 subagents 工具列出所有运行的子智能体"

# 使用会话生成工具
zeroclaw agent --message "请使用 sessions_spawn 工具生成新的子智能体会话"
```

## 性能优化

- **并发控制**：基于队列的并发控制，避免资源竞争
- **持久化存储**：SQLite 持久化存储，支持跨进程共享
- **进度跟踪**：实时进度跟踪和导出
- **依赖分析**：智能依赖分析和关键路径计算

## 安全考虑

- **权限控制**：基于安全策略的权限控制
- **输入验证**：所有输入参数验证
- **错误处理**：完善的错误处理和恢复机制
- **日志记录**：详细的日志记录和审计

## 已知问题

- macOS 辅助功能权限问题：由于 macOS 安全机制，某些自动化功能可能需要额外配置
- 沙盒环境限制：在沙盒环境中运行时，某些功能可能受限

## 未来改进

- 支持更多智能体类型
- 改进共识算法
- 优化依赖分析性能
- 添加更多导出格式
- 改进错误处理和恢复机制

## 相关 PR

- #1：SQLite 持久化实现
- #2：可观测性功能实现
- #3：智能体通信机制实现

## 测试结果

- ✅ Tech Lead 智能体测试通过
- ✅ Backend Developer 智能体测试通过
- ✅ Frontend Developer 智能体测试通过
- ✅ 智能体协作测试通过（To-Do List 应用开发）
- ✅ 并发控制测试通过
- ✅ 持久化存储测试通过
- ✅ 进度跟踪测试通过

## 检查清单

- [x] 代码实现完成
- [x] 单元测试通过
- [x] 集成测试通过
- [x] 文档完成
- [x] 配置示例完成
- [x] 数据库模式完成
- [x] 性能优化完成
- [x] 安全考虑完成

## 审核要点

1. **架构设计**：智能体群组架构是否合理
2. **代码质量**：代码是否符合 Rust 最佳实践
3. **测试覆盖**：测试是否充分
4. **文档完整性**：文档是否清晰完整
5. **性能考虑**：性能优化是否充分
6. **安全性**：安全考虑是否充分

## 变更类型

- [ ] 破坏性变更
- [ ] 功能新增
- [x] 功能改进
- [ ] Bug 修复
- [ ] 文档更新
- [ ] 性能优化
- [ ] 代码重构