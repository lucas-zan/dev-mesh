# Todo: 统一原生审批截止时间

- **Status**: 验证成功
- **Scope**: DM-04/05/06/07 的现有审批/输入超时配置。

## Behavior

- Input: Worker 配置的 approval_timeout_seconds、input_timeout_seconds。
- Output: Runtime 公共事件 deadline、Worker 等待上限及 Durable 原生睡眠使用同一配置。
- Constraints/errors: 保留默认 900/1800 秒；非法显式配置明确报错，不静默回退；不新增远程审批。

## Test first

- Codex 审批与输入事件分别采用自定义 17/23 秒。
- Durable 审批事件截止时间与原生 sleep.until 使用同一时间戳；零/非法超时拒绝。
- Expected RED: NativeState 无配置字段，Durable 固定 900000 毫秒。

## Implementation

Daemon 通过受管环境传递配置；Rust launch 解析一次并保存在 NativeState，Projector 生成 deadline；Durable Host 构造时接收审批秒数并统一计算事件和 sleep。

## Verification

- Codex、Durable、公共 native_wire、配置测试；TS build；fmt/Clippy。

## Done

- [x] RED 已观察。
- [x] 聚焦与回归通过。

## Evidence 2026-11-26

- RED: NativeState 缺配置字段；Durable approvalWindow 不存在且固定 900000ms。
- GREEN: Codex 自定义 17/23 秒、Durable 4 项、Rust Durable 2 项与 native_wire 3 项通过；TS build/Clippy 通过。
- Limitation: 本机真实模型和浏览器交互未验收；不据此标记总计划完成。
