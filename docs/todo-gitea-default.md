# Todo: 安装引导默认 Gitea 地址

- **Status**: 验证成功
- **Scope**: 安装配置引导的 Gitea 地址默认值。

## Behavior

- 显示默认地址 http://git.meditrust.xyz；回车使用，输入其他地址覆盖。
- Gitea Token 继续在安装引导中隐藏输入并保存系统凭据，不写入默认值或仓库。
- 已有 config.json 保留。

## Test first

- 仓库引导测试覆盖空地址使用默认、显式地址覆盖、Token 通过 secret 输入接口。
- 先观察旧代码空地址报错的 RED，再设置提示默认值。

## Verification

- 配置引导与安装注册 Python 测试，文档一致性。

## Done

- [x] RED。
- [x] 修改和相关测试通过。

## Evidence

- RED：空地址报错，显式地址提示中没有默认地址。
- GREEN：配置引导 12 项、安装注册 5 项通过；默认值/覆盖、隐藏 Token 输入和已有配置保留覆盖。未访问真实 Gitea。
