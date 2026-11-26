# Todo: 消息列表虚拟渲染

- **Status**: 验证成功
- **Scope**: DM-10 工作记录可变高度虚拟列表。

## Behavior

- Input: 有序事件、滚动窗口位置、已测行高。
- Output: 只挂载可见行和少量前后缓冲，空白占位保持总滚动高度；工具展开后重新测量。
- Constraints/errors: 单条长消息不得截断；历史分页与完整事件数据保留；无需新增服务或第三方框架。

## Test first

- 1000 行仅挂载顶部窗口；滚动中段选取正确范围；已测变高行影响范围与总高度；空列表。
- Expected RED: 无 VirtualEvents，当前 SessionPanel 挂载所有事件。

## Implementation

可变高度计算函数与 React 组件共用；ResizeObserver 测量行高，父 timeline 的滚动计算可见范围。保留原消息呈现与工具展开。

## Verification

- Web 模型/SSR 测试、TS/Vite 构建；浏览器滚动与视觉验收仍需真实环境。

## Done

- [x] RED 已观察。
- [x] 测试与构建通过。

## Evidence 2026-11-26

- RED: VirtualEvents 模块缺失；原列表全量挂载。
- GREEN: Web 11 项测试及构建通过；1000 行有界渲染、可变高度、窗口和空列表已测。
- Limitation: 本机真实模型和浏览器交互未验收；不据此标记总计划完成。
