#!/bin/bash

# SSE 测试脚本
# 测试 GUI Agent 的 SSE 流式 API

echo "=== SSE 测试脚本 ==="
echo ""

# 检查 curl 是否支持 SSE (-N 参数)
if ! curl -N --help 2>&1 | grep -q "no-buffer\|N"; then
    echo "警告: 当前 curl 版本可能不支持 SSE 测试"
    echo "尝试使用基本的 curl 测试..."
fi

echo "1. 测试全屏捕获 SSE 端点..."
echo "   端点: http://localhost:3000/gui/capture/screen/stream"
echo "   命令: curl -N http://localhost:3000/gui/capture/screen/stream"
echo ""

echo "2. 测试区域捕获 SSE 端点..."
echo "   端点: http://localhost:3000/gui/capture/region/stream"
echo "   命令: curl -N http://localhost:3000/gui/capture/region/stream?x=0&y=0&width=100&height=100"
echo ""

echo "3. 测试窗口捕获 SSE 端点..."
echo "   端点: http://localhost:3000/gui/capture/window/stream"
echo "   命令: curl -N http://localhost:3000/gui/capture/window/stream?window_id=12345"
echo ""

echo "=== 测试说明 ==="
echo "1. 确保 GUI Agent 服务器正在运行在端口 3000"
echo "2. 使用 'curl -N' 命令测试 SSE 流式端点"
echo "3. 按 Ctrl+C 停止测试"
echo ""
echo "运行测试前，请先启动 GUI Agent 服务器:"
echo "  cd /Users/wangwei/claw/zeroclaw"
echo "  cargo run --bin gui-gateway"
echo ""
echo "然后在另一个终端运行此测试脚本:"
