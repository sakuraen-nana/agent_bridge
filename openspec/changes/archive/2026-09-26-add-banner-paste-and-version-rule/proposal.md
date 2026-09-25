## Why

被控机启动横幅是使用方唯一需要抄录的东西，而它**粘进去一半能用**：`Token    : …` 那行碰巧符合 token 文档的 `key: value` 约定，已被识别；`局域网 IP: …` 那行的键名对不上 `host`，于是 client 报「未指定 host」并以码 2 退出。结果是"看起来抄对了，却不能用"，比完全不能用更费解——使用方得自己看出该把「局域网 IP」改写成 `host:`。

另一件事同类：`BRIDGE_VERSION` 自首次实现起停留在 `0.1.0`，历经多次行为变更从未推进。这不是漏改——仓库里**根本没有版本推进规则**，所以这次要把规则本身定下来，否则下次照旧。

## What Changes

- token 文档解析除既有 `key: value` 约定外，**SHALL 识别启动横幅的行形态**：横幅的 Token 行映射为 `token`（现状已可用，须保持），「局域网 IP」行映射为 `host`
- 该行可能列出多个逗号分隔的地址，取值规则：**优先 RFC1918 私网地址**（`10/8`、`172.16/12`、`192.168/16`），无则取该行第一个（与横幅自身调用示例所用地址一致）。多网卡机器上可避开 VPN 地址
- 仅认「局域网 IP」行，**不从 curl 示例行提取**（避免投机性解析）
- 既有解析语义不变：按行序后者覆盖前者，因此粘贴横幅后再写一行 `host:` 即可覆盖；多份横幅同贴时，后一份的 token 与 host 自然配对生效
- `bridge.local.md.example` 模板改为**主推「直接粘贴控制台输出」**，保留 `token:` / `host:` 手工字段供手填或覆盖 host
- 建立版本推进规则并写入 `AGENTS.md`：每个变更**归档时**把 `BRIDGE_VERSION` 的 minor 加一（缺陷修复类加 patch），归档清单增加「bump 版本」一项

**BREAKING**：无。既有 `key: value` 写法与后覆盖语义完全不变，新增的只是对另一种行形态的识别。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `agent-bridge`：
  - 「Token 文档契约」——解析规则扩展到启动横幅行形态（Token 行 → token，「局域网 IP」行 → host，多地址时私网优先）；模板改为主推粘贴

> 版本推进规则**不改主规格**：它是工程流程约定（写在 `AGENTS.md`），不是工具的行为契约；`hello` 返回服务版本这一契约本身不变。

## Impact

- 受影响代码：`src/agent_bridge/client.py`（`parse_token_doc` 扩展与私网地址判别）、`bridge.local.md.example`（模板）、`AGENTS.md`（版本规则）、`tests/`（新增用例）、`README.md`（token 文档一节的写法说明）
- 兼容性：既有 `token:` / `host:` 文档照旧可用；`--host` / `--token` 参数覆盖不变；不新增依赖，仍仅标准库
- 风险面：横幅行形态是被控端自己打印的，两侧同仓同版本，格式漂移风险低；「私网优先」在多私网网卡时仍是任选其一，覆盖手段是手工 `host:` 行
