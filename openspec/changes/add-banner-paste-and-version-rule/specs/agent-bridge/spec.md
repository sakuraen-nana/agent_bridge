## MODIFIED Requirements

### Requirement: Token 文档契约
工具 SHALL 在仓库根提供 `bridge.local.md.example` 模板（字段：`token:` 必填、`host:` 选填记录最近一次服务器 IP、`updated:` 填写日期，含填写说明），模板 SHALL 说明可直接粘贴被控端控制台输出的启动横幅。`bridge.local.md` MUST 通过 .gitignore 排除在 git 之外，且 MUST NOT 进入任何同步渠道。client SHALL 缺省从工具自身目录（与调用时的工作目录无关）读取该文件的 token 与缺省 host；文档 SHALL 允许含多组同名 `token` / `host` 字段（供一台开发机记录多台被控机），解析语义为后者覆盖前者。

在 `key: value` 行之外，解析 SHALL 识别被控端启动横幅的行形态——横幅的 Token 行 SHALL 映射为 token，横幅的「局域网 IP」行 SHALL 映射为 host。该行列出多个地址时，host SHALL 取其中的 RFC1918 私网地址（`10/8`、`172.16/12`、`192.168/16`）；若其中不含私网地址，SHALL 取该行第一个地址。解析 MUST NOT 从横幅的调用示例行提取地址。横幅行与 `key: value` 行 SHALL 遵循同一覆盖语义：按行序，后出现者覆盖先出现者。

#### Scenario: git 状态不含 token 文档
- **WHEN** 用户按模板创建并填写 bridge.local.md 后执行 `git status`
- **THEN** 该文件不出现在任何待提交列表中

#### Scenario: 模板字段齐全
- **WHEN** 查看仓库中的 bridge.local.md.example
- **THEN** 包含 token / host / updated 字段与填写说明，并说明可直接粘贴被控端控制台输出的启动横幅

#### Scenario: 多组 token 按后者覆盖解析
- **WHEN** token 文档含多组同名字段（如先记 184 的 token/host，后记另一台被控机的）
- **THEN** 缺省取最后一组；调用方可用 `--host` / `--token` 参数覆盖以指向其他组

#### Scenario: 粘贴控制台输出即可用
- **WHEN** 把被控机启动横幅整段粘贴进 token 文档（其中 IP 行写作「局域网 IP: …」而非 `host:`）
- **THEN** client 从该横幅取到 token 与 host，无需任何手工改写即可完成 hello / exec / download

#### Scenario: 多个地址时优先私网地址
- **WHEN** 横幅的「局域网 IP」行列出多个地址（如同时含 VPN 地址与私网地址）
- **THEN** host 取其中的 RFC1918 私网地址；若该行不含私网地址，则取第一个地址

#### Scenario: 手工字段覆盖粘贴值
- **WHEN** 文档中粘贴了横幅，其后又写有 `host:` 行
- **THEN** 以该 `host:` 行指定的地址为准（按行序后者覆盖前者）
