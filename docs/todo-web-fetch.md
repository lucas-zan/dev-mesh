# Todo: 修复 Web 登录 fetch 接收者错误

- **Status**: 验证成功
- **Scope**: 共享 RelayClient 默认浏览器请求边界。

## Behavior

- 输入：默认 RelayClient 的登录请求，及登录后的 /me 请求。
- 输出：使用 globalThis 作为原生 fetch 接收者，保留 Cookie、路径、参数和可注入 Transport。
- 错误：仍按现有 RelayError 返回，不重试或修改账号配置。

## Test first

- 使用严格检查 this 的浏览器 fetch 替身，默认客户端应完成 POST /auth/login 和 GET /me；旧代码把客户端实例作为接收者，抛 Illegal invocation。
- 现有注入 Transport 的命令与错误测试保持通过。

## Implementation

- devmesh-protocol/ts/client.ts 默认 Transport 用包装函数调用 globalThis.fetch。
- 对应 tests/client.test.ts；不修改登录组件或服务端认证。

## Verification

- 协议与 Web 测试、TypeScript 检查与 Web build；不宣称真实浏览器登录验证。

## Done

- [x] 观察 RED。
- [x] 修复并完成相关验证。

## Evidence

- RED：默认客户端登录请求复现 TypeError: Illegal invocation；已有注入 Transport 测试通过。
- GREEN：协议 18 项 schema + 3 项 SDK 测试、11 项 Web 测试、协议类型检查及 Web production build 全部通过。
- 修改：默认传输函数改为 globalThis.fetch 的包装，保留浏览器接收者；未更改认证与数据库。
- 限制：未实际访问用户的 Colima 页面；需重建本地镜像并刷新页面确认真实登录。
