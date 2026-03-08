#!/bin/bash

# GUI Agent macOS 完整测试脚本
# 测试 zeroclaw 在 macOS 环境的 GUI Agent 功能

set -e

echo "=========================================="
echo "GUI Agent macOS 完整测试"
echo "=========================================="
echo ""

# 检查 macOS 环境
echo "步骤 0: 检查 macOS 环境..."
if [ "$(uname)" != "Darwin" ]; then
    echo "❌ 当前不是 macOS 环境"
    exit 1
fi

echo "✅ macOS 环境检查通过"

# 检查依赖
echo ""
echo "步骤 1: 检查依赖..."
if ! command -v screencapture &> /dev/null; then
    echo "❌ screencapture 命令不存在"
    exit 1
fi
echo "✅ screencapture 检查通过"

# 构建项目
echo ""
echo "步骤 2: 构建项目..."
cd /Users/wangwei/claw/zeroclaw
cargo build --package zeroclaw --features gui-agent 2>&1 | tail -5

if [ $? -ne 0 ]; then
    echo "❌ 构建失败"
    exit 1
fi

echo "✅ 构建成功"

# 运行单元测试
echo ""
echo "步骤 3: 运行单元测试..."
echo "=========================================="
echo "3.1 屏幕捕获测试"
echo "=========================================="
cargo test --package zeroclaw --features gui-agent --lib screen_capture -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ 屏幕捕获测试失败"
    exit 1
fi

echo "✅ 屏幕捕获测试通过"

echo ""
echo "=========================================="
echo "3.2 窗口管理测试"
echo "=========================================="
cargo test --package zeroclaw --features gui-agent --lib window_manager -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ 窗口管理测试失败"
    exit 1
fi

echo "✅ 窗口管理测试通过"

echo ""
echo "=========================================="
echo "3.3 自动化控制测试"
echo "=========================================="
cargo test --package zeroclaw --features gui-agent --lib automation_executor -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ 自动化控制测试失败"
    exit 1
fi

echo "✅ 自动化控制测试通过"

echo ""
echo "=========================================="
echo "3.4 HTTP Gateway 测试"
echo "=========================================="
cargo test --package zeroclaw --features gui-agent --lib http_gateway -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ HTTP Gateway 测试失败"
    exit 1
fi

echo "✅ HTTP Gateway 测试通过"

# 运行所有测试
echo ""
echo "步骤 4: 运行所有 GUI Agent 测试..."
cargo test --package zeroclaw --features gui-agent --lib gui_agent -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ GUI Agent 测试失败"
    exit 1
fi

echo "✅ GUI Agent 测试通过"

echo ""
echo "=========================================="
echo "✅ 所有 GUI Agent macOS 测试通过"
echo "=========================================="
echo ""
echo "测试总结:"
echo "- 屏幕捕获: ✅"
echo "- 窗口管理: ✅"
echo "- 自动化控制: ✅"
echo "- HTTP Gateway: ✅"
echo ""
echo "注意: 某些测试可能需要辅助功能权限"
echo "请在系统设置 -> 隐私与安全性 -> 辅助功能中允许终端访问"
