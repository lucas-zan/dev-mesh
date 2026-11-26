# Todo: 单个 Agent 也需确认

- **Status**: 验证成功
- **Scope**: 自动检测到唯一 Agent 时的交互确认。

## Behavior

- 显示名称、版本、路径；只有明确回答 y/yes 才复用。
- n/no 或直接回车进入原有 Agent 选择菜单；其他输入明确报错。
- 非交互 auto 即使只有一个也要求 --runtime；显式 --runtime 视为用户选择，无需提问。
- 不改变安装、配置、下载和宿主注册行为。

## Test first

- 确认、拒绝/空输入后另选、非交互显式选择、多 Agent 及非法确认输入；先观察 RED。

## Verification

- Agent 选择与安装/注册聚焦 Python 测试，文档同步；不执行真实安装。

## Done

- [x] RED 已观察。
- [x] 修改与聚焦回归通过。

## Evidence

- RED：4 个行为用例失败，旧实现未询问确认、拒绝不生效且非交互自动复用。
- GREEN：Agent 选择 11 项与安装注册 5 项通过，Python 编译检查通过。
- 覆盖明确同意、拒绝、空输入、非法输入、非交互显式选择和多 Agent 选择；包含既有真实缓存 SDK Host 启动回归。未执行真实安装。
