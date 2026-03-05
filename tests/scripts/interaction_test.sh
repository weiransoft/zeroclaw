#!/bin/bash

set -e

# 测试 zeroclaw 交互模式和性能

echo "=== ZeroClaw 交互模式测试 ==="
echo ""

# 测试 1: 基础响应时间测试
echo "1. 基础响应时间测试"
echo "------------------------"
time_output=$(time (echo "hello" | ./target/debug/zeroclaw agent --message "hello" 2>&1))
echo "响应时间:"
echo "$time_output"
echo ""

# 测试 2: 工具调用性能测试
echo "2. 工具调用性能测试"
echo "------------------------"
time_output=$(time (echo "list files" | ./target/debug/zeroclaw agent --message "list files in current directory" 2>&1))
echo "响应时间:"
echo "$time_output"
echo ""

# 测试 3: 内存检索测试
echo "3. 内存检索测试"
echo "------------------------"
# 先存储一些内存
./target/debug/zeroclaw agent --message "remember that my favorite color is blue" > /dev/null 2>&1
sleep 1
# 然后检索
./target/debug/zeroclaw agent --message "what is my favorite color?" 2>&1

# 测试 4: 并发请求测试
echo "4. 并发请求测试"
echo "------------------------"
for i in {1..3}; do
    echo "并发请求 $i"
    ./target/debug/zeroclaw agent --message "echo hello $i" > test_output_$i.txt 2>&1 &
done

# 等待所有请求完成
wait

# 显示结果
for i in {1..3}; do
    echo "请求 $i 结果:"
    cat test_output_$i.txt
    echo ""
done

# 清理测试文件
rm -f test_output_*.txt

echo "=== 测试完成 ==="
