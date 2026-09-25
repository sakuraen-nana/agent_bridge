---
name: git-commit
description: 生成符合本项目规范的 git commit 消息（源自 VSCode 插件 RedJue/git-commit-plugin 的 Angular 提交规范）。在执行任何 git commit（agent 自动提交或协助用户撰写提交信息）之前必须使用，保证 agent 自动提交与用户通过插件手动提交的格式一致。
metadata:
  author: se77
  version: "1.0"
  source: https://github.com/RedJue/git-commit-plugin
---

# Git 提交信息规范（git-commit-plugin / Angular 规范）

用户平时使用 VSCode 插件 [git-commit-plugin](https://github.com/RedJue/git-commit-plugin) 手动生成提交信息。该插件遵循 Angular 团队提交规范。agent 自动提交时**必须**产出与插件完全一致的格式，保证 `git log` 风格统一。

## 1. 格式模板

插件默认 Angular 模板为：

```
<icon><space><type>(<scope>):<space><subject><enter><body><enter><footer>
```

（插件另有 git-cz 模板变体，本项目统一采用上述默认模板，不使用变体。）

渲染为实际消息：

```
<emoji> <type>(<scope>): <subject>

<body>

<footer>
```

- 首行 = `<emoji> + 空格 + <type>(<scope>): <subject>`，即插件 `ShowEmoji: true`（默认开启）时的输出；冒号为英文 `:`，后接一个空格。
- 首行与 body 之间、body 与 footer 之间各空一行。
- `<body>`、`<footer>` 可选；只有首行时直接单行提交。
- 示例：`✨ feat(client): 支持 --timeout 覆盖超时`

## 2. 提交类型（12 种，含 emoji）

类型与 emoji 映射来自插件源码 `src/config/commit-type.ts`，描述文案取自插件中文语言包（`package.nls.zh-cn.json`）。emoji **不得替换或省略**（`↩` 为 U+21A9 箭头字符，插件中 revert 即如此，照用即可）：

| emoji | type | 用途 |
|-------|--------|------|
| 🎉 | init | 项目初始化 |
| ✨ | feat | 添加新特性 |
| 🐞 | fix | 修复 bug |
| 📃 | docs | 仅仅修改文档 |
| 🌈 | style | 仅仅修改了空格、格式缩进、逗号等等，不改变代码逻辑 |
| 🦄 | refactor | 代码重构，没有加新功能或者修复 bug |
| 🎈 | perf | 优化相关，比如提升性能、体验 |
| 🧪 | test | 增加测试用例 |
| 🔧 | build | 依赖相关的内容（构建、依赖包） |
| 🐎 | ci | ci 配置相关，例如对 k8s、docker 的配置文件的修改 |
| 🐳 | chore | 改变构建流程、或者增加依赖库、工具等 |
| ↩ | revert | 回滚到上一个版本 |

选型要点：

- 新功能（含新增 UI、接口、参数）→ `feat`；修 bug → `fix`。
- 一次提交只覆盖一类改动；无关改动拆成多个提交，各自使用对应 type。随功能附带的小幅测试/文档调整跟随主 type，不单独拆分。
- 改动同时涉及多种类型时，按本次改动的**主要意图**选一个 type。

## 3. 字段规则

**scope**（作用域）

- 提交影响的模块/目录/组件名，如 `client`、`server`、`bootstrap`。
- 影响多个模块时使用 `*`。
- 无明确 scope 时可省略括号：`✨ feat: xxx`。

**subject**（简短描述）

- 祈使语气、现在时（用 "change"，而不是 "changed" 或 "changes"）。
- 英文时首字母不大写，结尾不加句号。
- 语言跟随仓库既有提交：中文为主。
- 尽量简短（插件默认上限 20 字符，`MaxSubjectCharacters` 可配置）；一行写不下时把细节放进 body。

**body**（正文，可选）

- 同样使用祈使语气、现在时。
- 说明变更动机，以及与之前行为的对比。

**footer**（页脚，可选）

- 不兼容变更：以 `BREAKING CHANGE:` 开头（后接空格或换行），说明影响与迁移方式。
- 关联 issue：`Closes #123`、`Fixes #123`。
- 本项目提交信息**不得包含任何协作者署名**：不加 `Co-Authored-By:` 行，不署工具名或 AI 名，只描述变更本身。

## 4. 提交流程（agent 自动提交）

1. `git status` + `git diff` 了解改动全貌；拿不准近期风格时先 `git log --oneline -10` 参考。
2. 按 §2 选 type，按 §3 组装各字段。
3. 使用 heredoc 提交，保证 emoji 与多行格式正确：

```bash
git commit -m "$(cat <<'EOF'
✨ feat(client): 支持 --timeout 覆盖超时

未显式传入时沿用服务端缺省 1800 秒；显式传入可覆盖单次调用的超时设置。

Closes #12
EOF
)"
```

4. 单行提交（无 body/footer）：

```bash
git commit -m "🐞 fix(server): 修复端口占用时的错误提示"
```

## 5. 边界情况

- **merge commit**：保留 git 默认生成的合并消息，不做规范改写。
- **revert**：执行 `git revert` 后，把默认消息改写为 `↩ revert(<scope>): 回滚 <原提交主题>`，footer 保留 `This reverts commit <hash>.`。
- **type 拿不准**：优先询问用户，或参考 `git log` 中同类改动的既有 type。
