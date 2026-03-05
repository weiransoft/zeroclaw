# ZeroClaw 产品需求文档 (PRD)

## 文档信息

| 属性 | 值 |
|------|-----|
| **产品名称** | ZeroClaw |
| **版本** | v0.1.0 |
| **文档版本** | v2.0 |
| **最后更新** | 2026-03-05 |
| **产品负责人** | theonlyhennygod |
| **产品定位** | 零开销、零妥协的 AI 助手基础设施 |

## 更新履历

| 版本 | 日期 | 更新人 | 更新内容 | 审核状态 |
|------|------|--------|----------|----------|
| v1.0 | 2026-02-13 | theonlyhennygod | 初始版本创建，定义核心架构 | 已审核 |
| v2.0 | 2026-03-05 | Product Manager (AI) | 全面更新反映最新功能：22+ 服务商、8 渠道、完整记忆系统、硬件支持、安全增强 | 待审核 |

---

## 1. 产品概述

### 1.1 产品愿景

ZeroClaw 是一个超轻量级、高性能、完全自主的 AI 助手基础设施，支持在任何硬件上部署（从 10 美元的边缘设备到云服务器），提供零开销、零妥协的 AI 代理能力。

### 1.2 核心价值主张

- **零开销 (Zero Overhead)**: <5MB 内存占用，<10ms 启动时间，3.4MB 二进制大小
- **零妥协 (Zero Compromise)**: 完整功能集，22+ AI 服务商支持，8 个通信渠道，持久化记忆
- **100% Rust**: 内存安全，零成本抽象，跨平台支持 (ARM, x86, RISC-V)
- **100% 不可知论 (100% Agnostic)**: 可插拔架构，所有子系统都是 trait，无供应商锁定

### 1.3 目标用户

| 用户类型 | 需求场景 | 核心价值 |
|---------|---------|---------|
| **个人开发者** | 个人 AI 助手、自动化工作流 | 低成本、易部署、隐私保护 |
| **小团队** | 团队协作机器人、客服自动化 | 多渠道支持、可扩展、低成本 |
| **边缘计算场景** | IoT 设备、嵌入式 AI 代理 | 超低资源占用、离线运行 |
| **企业用户** | 私有化 AI 基础设施 | 数据安全、可定制、无锁定 |

### 1.4 竞品对比

| 产品 | 内存占用 | 启动时间 | 二进制大小 | 成本 | 语言 |
|------|---------|---------|-----------|------|------|
| **ZeroClaw** | **<5MB** | **<10ms** | **3.4MB** | **$10 硬件** | **Rust** |
| OpenClaw | >1GB | >500s | ~28MB | Mac Mini $599 | TypeScript |
| NanoBot | >100MB | >30s | N/A | Linux SBC $50 | Python |
| PicoClaw | <10MB | <1s | ~8MB | Linux Board $10 | Go |

**性能优势**:
- 内存占用比 OpenClaw 小 **99%**
- 启动速度快 **400 倍**
- 硬件成本降低 **98%**

---

## 2. 功能需求

### 2.1 核心功能模块

#### 2.1.1 AI 服务商层 (Provider)

**功能描述**: 支持 22+ 主流 AI 服务商，提供统一的 API 抽象层

**支持的服务商**:
- **主流服务商**: OpenRouter, Anthropic, OpenAI, Ollama
- **高性能推理**: Groq, Fireworks, Together AI
- **专业模型**: Venice (无审查), xAI (Grok), DeepSeek
- **企业级**: AWS Bedrock, Cohere, Mistral, Perplexity
- **自定义**: 任何 OpenAI 兼容 API

**验收标准**:
- [ ] 支持通过配置切换服务商，无需代码修改
- [ ] 所有服务商使用统一的 trait 接口
- [ ] 支持自定义 HTTPS 端点
- [ ] 失败自动重试和降级
- [ ] 成本追踪和用量统计

#### 2.1.2 通信渠道层 (Channel)

**功能描述**: 支持 8 种通信渠道，实现多渠道 AI 交互

**支持的渠道**:
1. **CLI**: 本地命令行交互
2. **Telegram**: 即时通讯机器人
3. **Discord**: 社区/游戏社区机器人
4. **Slack**: 企业团队协作
5. **iMessage**: macOS 原生消息
6. **Matrix**: 去中心化通讯
7. **WhatsApp**: 全球主流通讯
8. **Email**: 邮件自动化

**验收标准**:
- [ ] 所有渠道使用统一的 trait 接口
- [ ] 支持多渠道并行运行
- [ ] 渠道配置热重载
- [ ] 渠道健康检查 (`zeroclaw channel doctor`)
- [ ] 渠道白名单和访问控制

#### 2.1.3 记忆系统 (Memory)

**功能描述**: 全栈搜索式记忆引擎，支持持久化、向量检索、混合搜索

**记忆后端**:
- **SQLite**: 混合搜索 (FTS5 + 向量余弦相似度)
- **Lucid**: CLI 同步 + SQLite 回退
- **Markdown**: 人类可读的纯文本存储
- **None**: 无状态模式

**核心特性**:
- **向量数据库**: SQLite BLOB 存储，余弦相似度搜索
- **关键词搜索**: FTS5 虚拟表，BM25 评分
- **混合合并**: 自定义加权合并函数
- **嵌入缓存**: SQLite 缓存表，LRU 淘汰
- **安全重建**: 原子性重建 FTS5 + 重新嵌入

**配置示例**:
```toml
[memory]
backend = "sqlite"
auto_save = true
embedding_provider = "openai"
vector_weight = 0.7
keyword_weight = 0.3
```

**验收标准**:
- [ ] 支持 4 种记忆后端切换
- [ ] 向量搜索响应时间 <100ms
- [ ] 混合搜索准确率 >85%
- [ ] 记忆自动保存和召回
- [ ] 支持记忆管理工具 (store/recall/forget)

#### 2.1.4 工具系统 (Tool)

**功能描述**: 提供 AI 代理可调用的工具集

**内置工具**:
- `shell`: 沙盒化 shell 命令执行
- `file_read`: 安全文件读取
- `file_write`: 安全文件写入
- `memory_store`: 记忆存储
- `memory_recall`: 记忆召回
- `memory_forget`: 记忆删除
- `browser_open`: 浏览器打开 (Brave + 白名单)
- `browser`: 原生浏览器自动化 (可选 fantoccini)
- `composio`: 第三方集成 (可选)

**验收标准**:
- [ ] 所有工具使用统一的 trait 接口
- [ ] 工具执行沙盒隔离
- [ ] 文件系统访问限制在工作区
- [ ] 浏览器访问白名单控制
- [ ] 支持自定义工具扩展

#### 2.1.5 可观测性 (Observability)

**功能描述**: 多层次的系统观测能力

**实现**:
- **NoopObserver**: 零开销模式
- **LogObserver**: tracing 日志
- **MultiObserver**: 多路分发
- **OpenTelemetry**: OTLP trace + metrics 导出
- **Prometheus**: 指标暴露

**验收标准**:
- [ ] 支持多种 Observer 切换
- [ ] 指标包含请求延迟、 token 用量、错误率
- [ ] 支持分布式追踪
- [ ] 日志级别可配置
- [ ] 性能开销 <5%

#### 2.1.6 运行时 (Runtime)

**功能描述**: 支持多种运行时环境

**支持的运行时**:
- **Native**: 原生执行 (Mac/Linux/Raspberry Pi)
- **Docker**: 沙盒化容器执行
- **WASM**: 边缘运行时 (计划中)

**验收标准**:
- [ ] 运行时配置可切换
- [ ] 不支持的运行时明确报错
- [ ] Docker 运行时自动拉取镜像
- [ ] 运行时资源限制可配置

#### 2.1.7 安全系统 (Security)

**功能描述**: 多层安全防护

**安全特性**:
1. **网关配对**: 6 位数一次性代码，Bearer Token 认证
2. **文件系统隔离**: 工作区限制，14 个系统目录 +4 个敏感文件禁止访问
3. **隧道访问**: 仅通过 Tailscale/Cloudflare/ngrok 暴露
4. **命令白名单**: 允许的命令列表
5. **速率限制**: 请求频率控制
6. **加密存储**: ChaCha20-Poly1305 AEAD 加密密钥
7. **路径遍历防护**: 符号链接检测，规范路径检查
8. **Landlock 沙盒**: Linux 内核级隔离 (可选)

**验收标准**:
- [ ] 通过社区安全清单所有项目
- [ ] 默认绑定 127.0.0.1，拒绝 0.0.0.0
- [ ] 所有 webhook 请求需要认证
- [ ] 文件系统访问日志记录
- [ ] 敏感操作需要确认

#### 2.1.8 身份系统 (Identity)

**功能描述**: 支持多种身份格式

**支持的格式**:
- **OpenClaw**: Markdown 格式
- **AIEOS v1.1**: JSON 格式

**验收标准**:
- [ ] 支持身份格式切换
- [ ] 身份配置热重载
- [ ] 身份验证失败友好提示

#### 2.1.9 隧道系统 (Tunnel)

**功能描述**: 支持多种内网穿透方案

**支持的隧道**:
- **Cloudflare Tunnel**: Cloudflare 内网穿透
- **Tailscale**: Tailscale 私有网络
- **ngrok**: ngrok 内网穿透
- **Custom**: 自定义隧道二进制

**验收标准**:
- [ ] 隧道配置可切换
- [ ] 隧道状态监控
- [ ] 隧道断开自动重连

#### 2.1.10 心跳引擎 (Heartbeat)

**功能描述**: 周期性任务执行引擎

**功能**:
- 从 HEARTBEAT.md 读取任务
- 周期性自动执行
- 任务执行日志

**验收标准**:
- [ ] 支持 cron 表达式配置
- [ ] 任务执行失败告警
- [ ] 任务历史可查询

#### 2.1.11 技能系统 (Skills)

**功能描述**: 可插拔的技能包系统

**功能**:
- TOML manifest + SKILL.md 指令
- 社区技能包支持
- 技能热加载

**验收标准**:
- [ ] 支持技能安装/卸载
- [ ] 技能配置独立
- [ ] 技能冲突检测

#### 2.1.12 集成系统 (Integrations)

**功能描述**: 50+ 集成，9 个类别

**集成类别**:
- 项目管理、文档、设计、沟通、代码托管、云服务等

**验收标准**:
- [ ] 集成信息查询 (`zeroclaw integrations info`)
- [ ] 集成配置可插拔
- [ ] 集成状态监控

#### 2.1.13 硬件支持 (Hardware)

**功能描述**: 硬件发现和外设通信

**支持的功能**:
- **USB 设备发现**: 枚举 USB 设备 (VID/PID)
- **设备 introspect**: 识别已知开发板
- **串口通信**: STM32/Nucleo 通信
- **GPIO 控制**: Raspberry Pi GPIO (可选 rppal)
- **内存读取**: Nucleo 内存读取 (可选 probe-rs)

**验收标准**:
- [ ] 硬件发现命令 (`zeroclaw hardware discover`)
- [ ] 支持 Arduino Uno, ESP32, Nucleo 等开发板
- [ ] 串口通信稳定
- [ ] GPIO 控制响应快速

#### 2.1.14 MCP 集成 (MCP)

**功能描述**: Model Context Protocol 集成

**功能**:
- MCP Store 管理
- Tool Adapter 适配
- 桌面应用集成

**验收标准**:
- [ ] MCP 配置管理
- [ ] MCP 工具调用
- [ ] 桌面应用通信

#### 2.1.15 工作流系统 (Workflow)

**功能描述**: 可视化工作流编排

**功能**:
- 工作流模板
- 多智能体协作
- 任务编排

**验收标准**:
- [ ] 工作流创建和执行
- [ ] 工作流模板库
- [ ] 多智能体调度

#### 2.1.16 SOUL 系统 (SOUL)

**功能描述**: 系统优化和用户层

**功能**:
- 用户体验优化
- 系统资源管理
- 性能调优

**验收标准**:
- [ ] SOUL 配置管理
- [ ] 性能监控
- [ ] 资源优化

---

### 2.2 CLI 命令

**核心命令**:
```bash
# 快速入门
zeroclaw onboard --api-key sk-... --provider openrouter  # 快速设置
zeroclaw onboard --interactive                            # 交互式向导
zeroclaw onboard --channels-only                          # 仅修复渠道/白名单

# 代理交互
zeroclaw agent -m "Hello, ZeroClaw!"  # 单次消息
zeroclaw agent                         # 交互模式

# 网关服务
zeroclaw gateway                # 默认 127.0.0.1:8080
zeroclaw gateway --port 0       # 随机端口 (安全加固)

# 守护进程
zeroclaw daemon                 # 启动自主运行时

# 状态检查
zeroclaw status                 # 系统状态
zeroclaw doctor                 # 系统诊断
zeroclaw channel doctor         # 渠道健康检查

# 集成管理
zeroclaw integrations info Telegram  # 查看集成详情

# 服务管理
zeroclaw service install    # 安装守护进程
zeroclaw service status     # 查看服务状态

# 数据迁移
zeroclaw migrate openclaw --dry-run  # 预览迁移
zeroclaw migrate openclaw            # 执行迁移

#  Cron 任务
zeroclaw cron list
zeroclaw cron add "0 * * * *" "command"
zeroclaw cron once "30m" "command"

# 技能管理
zeroclaw skill list
zeroclaw skill install <source>
zeroclaw skill remove <name>

# 硬件发现
zeroclaw hardware discover
zeroclaw hardware introspect /dev/ttyACM0
```

**验收标准**:
- [ ] 所有命令有 --help 文档
- [ ] 错误提示友好且具体
- [ ] 支持全局安装和开发模式运行
- [ ] 低内存设备编译优化提示

---

## 3. 非功能需求

### 3.1 性能指标

| 指标 | 目标值 | 测量方法 |
|------|-------|---------|
| **内存占用** | <5MB | `/usr/bin/time -l` |
| **启动时间** | <10ms (0.8GHz) | `time` 命令 |
| **二进制大小** | 3.4MB | `ls -lh` |
| **向量搜索延迟** | <100ms | 基准测试 |
| **渠道消息延迟** | <500ms | 端到端测试 |
| **工具执行延迟** | <1s | 工具基准测试 |

### 3.2 可靠性

- **服务可用性**: >99.9%
- **数据持久化**: SQLite WAL 模式，原子写入
- **故障恢复**: 自动重启，状态恢复
- **错误处理**: 所有异常路径处理，友好错误提示

### 3.3 安全性

**安全清单** (全部通过):
- [x] 网关不公开暴露 (绑定 127.0.0.1)
- [x] 配对认证必需 (6 位代码 + Bearer Token)
- [x] 文件系统作用域限制 (workspace_only=true)
- [x] 仅通过隧道访问 (拒绝公网绑定)
- [x] 路径遍历攻击防护
- [x] 命令注入阻断
- [x] 敏感文件访问禁止
- [x] 密钥加密存储 (ChaCha20-Poly1305)
- [x] 速率限制
- [x] 日志不包含敏感信息

### 3.4 兼容性

**操作系统**:
- Linux (x86_64, ARM64)
- macOS (Intel, Apple Silicon)
- Windows (x86_64, ARM64)
- Raspberry Pi OS (ARMv6, ARMv7, ARM64)

**硬件架构**:
- ARM (Raspberry Pi, Apple Silicon)
- x86 (Intel, AMD)
- RISC-V (计划中)

### 3.5 可维护性

- **代码覆盖率**: >80%
- **测试数量**: >1000 个测试
- **文档完整性**: 所有模块有文档
- **配置热重载**: 支持无需重启修改配置

---

## 4. 用户故事

### 4.1 个人开发者

**故事 1**: 作为个人开发者，我希望快速部署一个 AI 助手
- **场景**: 在 Raspberry Pi 上部署 ZeroClaw
- **操作**: 
  1. `git clone` 克隆代码
  2. `cargo build --release` 编译
  3. `zeroclaw onboard --api-key ...` 配置
  4. `zeroclaw daemon` 启动
- **验收**: 5 分钟内完成部署，内存占用<5MB

**故事 2**: 作为开发者，我希望通过 Telegram 与 AI 交互
- **场景**: 在手机上通过 Telegram 发送消息给 AI
- **操作**:
  1. 配置 Telegram Bot Token
  2. 发送消息到 Bot
  3. AI 自动回复
- **验收**: 消息延迟<500ms，支持多轮对话

### 4.2 小团队

**故事 3**: 作为团队，我希望在 Slack 中集成 AI 助手
- **场景**: 团队在 Slack 频道中与 AI 协作
- **操作**:
  1. 配置 Slack Bot
  2. 邀请 Bot 加入频道
  3. @mention AI 提问
- **验收**: AI 正确理解上下文，回答准确

**故事 4**: 作为客服团队，我希望自动化常见问题回复
- **场景**: 客户通过 Email 咨询常见问题
- **操作**:
  1. 配置 Email 渠道
  2. AI 自动读取邮件
  3. 基于记忆系统检索答案
  4. 自动回复客户
- **验收**: 常见问题自动回复率>80%

### 4.3 边缘计算

**故事 5**: 作为 IoT 开发者，我希望在 ESP32 上运行 AI 代理
- **场景**: ESP32 设备通过串口与 ZeroClaw 通信
- **操作**:
  1. 配置串口通信
  2. ESP32 发送传感器数据
  3. AI 分析数据并返回决策
- **验收**: 串口通信稳定，延迟<100ms

### 4.4 企业用户

**故事 6**: 作为企业 IT，我希望私有化部署 AI 基础设施
- **场景**: 在企业内网部署 ZeroClaw
- **操作**:
  1. 配置 Tailscale 隧道
  2. 配置企业身份系统
  3. 配置审计日志
  4. 部署到 Kubernetes
- **验收**: 数据不出内网，符合安全合规

---

## 5. 验收标准

### 5.1 功能验收

**核心功能**:
- [ ] 所有 22+ 服务商正常工作
- [ ] 所有 8 个渠道正常工作
- [ ] 记忆系统持久化正常
- [ ] 工具执行安全且正确
- [ ] 可观测性指标准确
- [ ] 安全特性全部启用

**性能验收**:
- [ ] 内存占用<5MB
- [ ] 启动时间<10ms
- [ ] 二进制大小<4MB
- [ ] 向量搜索<100ms
- [ ] 消息延迟<500ms

**安全验收**:
- [ ] 通过所有安全清单项目
- [ ] 渗透测试无高危漏洞
- [ ] 敏感数据加密存储
- [ ] 访问控制有效

### 5.2 测试覆盖

**单元测试**:
- [ ] 核心逻辑覆盖率>80%
- [ ] 所有测试通过
- [ ] 测试执行时间<1 分钟

**集成测试**:
- [ ] 渠道集成测试通过
- [ ] 服务商集成测试通过
- [ ] 数据库操作测试通过

**端到端测试**:
- [ ] 完整用户流程测试通过
- [ ] 性能基准测试达标
- [ ] 安全测试通过

---

## 6. 依赖关系

### 6.1 核心依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| tokio | 1.42 | 异步运行时 |
| reqwest | 0.12 | HTTP 客户端 |
| serde | 1.0 | 序列化 |
| clap | 4.5 | CLI 解析 |
| rusqlite | 0.38 | SQLite 数据库 |
| axum | 0.8 | HTTP 服务器 |
| tracing | 0.1 | 日志 |
| prometheus | 0.14 | 指标 |

### 6.2 可选依赖

| 依赖 | 特性 | 用途 |
|------|------|------|
| fantoccini | browser-native | 浏览器自动化 |
| nusb | hardware | USB 设备发现 |
| tokio-serial | hardware | 串口通信 |
| probe-rs | probe | Nucleo 内存读取 |
| rppal | peripheral-rpi | Raspberry Pi GPIO |
| landlock | sandbox-landlock | Linux 沙盒 |
| duckdb | duckdb | 分析型 trace 存储 |
| pdf-extract | rag-pdf | PDF 解析 |

---

## 7. 风险与缓解

### 7.1 技术风险

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| **内存占用超标** | 高 | 低 | 持续性能监控，LTO 优化 |
| **服务商 API 变更** | 中 | 中 | 抽象层隔离，快速适配 |
| **安全漏洞** | 高 | 低 | 定期安全审计，最小权限 |
| **跨平台兼容性** | 中 | 中 | CI/CD 多平台测试 |

### 7.2 业务风险

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| **竞品价格战** | 低 | 高 | 保持成本优势，聚焦边缘场景 |
| **用户增长缓慢** | 中 | 中 | 社区建设，文档优化 |
| **生态不完善** | 中 | 中 | 技能市场，社区贡献 |

---

## 8. 路线图

### 8.1 短期 (Q1 2026)

- [ ] 完善硬件支持 (ESP32, Nucleo)
- [ ] 增加更多 AI 服务商
- [ ] 优化移动端体验
- [ ] 完善技能市场

### 8.2 中期 (Q2 2026)

- [ ] WASM 运行时支持
- [ ] 多智能体协作优化
- [ ] 企业级特性增强 (SSO, 审计)
- [ ] 性能再优化 (目标<3MB 内存)

### 8.3 长期 (2026+)

- [ ] RISC-V 架构支持
- [ ] 去中心化 AI 网络
- [ ] 自主学习和优化
- [ ] 边缘 AI 生态建设

---

## 9. 附录

### 9.1 快速开始

```bash
# 克隆项目
git clone https://github.com/zeroclaw-labs/zeroclaw.git
cd zeroclaw

# 编译
cargo build --release --locked

# 安装
cargo install --path . --force --locked

# 快速配置
zeroclaw onboard --api-key sk-... --provider openrouter

# 开始使用
zeroclaw agent -m "Hello, ZeroClaw!"
```

### 9.2 低内存设备编译

```bash
# Raspberry Pi 3 (1GB RAM)
CARGO_BUILD_JOBS=1 cargo build --release
```

### 9.3 资源链接

- **GitHub**: https://github.com/zeroclaw-labs/zeroclaw
- **文档**: ./docs/
- **示例**: ./examples/
- **技能**: ./skills/

---

## 10. 审核记录

| 版本 | 审核日期 | 审核人 | 审核意见 | 状态 |
|------|---------|--------|----------|------|
| v1.0 | 2026-02-13 | theonlyhennygod | 初始版本通过 | ✅ 已批准 |
| v2.0 | 待审核 | 待指定 | 待审核 | ⏳ 待审核 |

**审核要点**:
- [ ] 产品定位清晰
- [ ] 功能需求完整
- [ ] 验收标准可测试
- [ ] 技术可行性已评估
- [ ] 安全风险已识别

---

*本文档最后更新：2026-03-05*
