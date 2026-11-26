# Todo: Codex 多问题本地输入

- **Status**: 验证成功
- **Scope**: DM-05 本地 user-input 响应；保留既有单问题 --text。

## Behavior

- Input: Codex requestUserInput 含多个问题；本机 `inputs reply --answers-json` 传 question ID 到字符串数组的对象。
- Output: 对应原生请求 ID 的 answers 响应，各问题完整绑定。
- Constraints/errors: 缺问题、未知问题或错误值形状拒绝；不将未回答问题默认为批准；单问题普通文本兼容。

## Test first

- 多问题完整回答、缺少一个问题、额外问题、单问题文本。
- Expected RED: 当前多问题一律 unsupported_capability。

## Implementation

在 Codex Adapter 解析并验证答案对象；CLI 仅传递显式 JSON 输入，不改变远程审批规则。

## Verification

- `cargo test -p devmesh-agent --test codex --offline`
- Runtime/MCP 相关回归及 fmt/Clippy。

## Done

- [x] RED 已观察。
- [x] 聚焦与回归通过。

## Evidence 2026-11-26

- RED: 多问题回复报 unsupported_capability；缺少 input_reply；输入请求被错误当作批准。
- GREEN: Codex 7 项、CLI 3 项测试通过；structured answers 仅走本机接口，默认不改变原生权限。
- Limitation: 本机真实模型和浏览器交互未验收；不据此标记总计划完成。
