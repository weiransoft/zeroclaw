#!/bin/bash

# GUI Agent 屏幕捕获测试脚本
# 测试 macOS 平台的屏幕捕获功能

set -e

echo "=========================================="
echo "GUI Agent 屏幕捕获测试"
echo "=========================================="

# 构建项目
echo "步骤 1: 构建项目..."
cd /Users/wangwei/claw/zeroclaw
cargo build --package zeroclaw --features gui-agent 2>&1 | tail -5

if [ $? -ne 0 ]; then
    echo "❌ 构建失败"
    exit 1
fi

echo "✅ 构建成功"

# 运行屏幕捕获测试
echo ""
echo "步骤 2: 运行屏幕捕获测试..."
cargo test --package zeroclaw --features gui-agent screen_capture -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ 屏幕捕获测试失败"
    exit 1
fi

echo "✅ 屏幕捕获测试成功"

# 测试区域捕获
echo ""
echo "步骤 3: 测试区域捕获..."
cargo test --package zeroclaw --features gui-agent capture_region -- --nocapture

if [ $? -ne 0 ]; then
    echo "❌ 区域捕获测试失败"
    exit 1
fi

echo "✅ 区域捕获测试成功"

echo ""
echo "=========================================="
echo "✅ 所有屏幕捕获测试通过"
echo "=========================================="
