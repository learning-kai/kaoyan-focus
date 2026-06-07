# 开发者 AI 开发与 PR 流程

请开发者在让 AI 修改本仓库之前，先让 AI 阅读本文。

本文面向“开发者自己电脑上的 AI”，用于规范本地开发、整理改动、提交 PR、更新 PR 和发布前检查。它不是版本发布说明，也不是维护者合并所有 PR 的后台流程。

## 使用前准备

最低要求：

1. 本机已安装 `git`。
2. 本机已安装 Node.js、Rust 和 Tauri 所需 Windows 构建环境。
3. 如果要创建 PR，本机已安装 GitHub CLI：`gh`。
4. 如果要推送分支或创建 PR，已经完成 `gh auth login`。

AI 检查到 `gh` 不可用、未登录、登录账号不对或权限不足时，必须停止并说明，不得假装已经完成 GitHub 操作。

## 本文适用场景

适用于：

- 开发新功能。
- 修复 bug。
- 结构化重构。
- 文档更新。
- 整理本地提交。
- 创建或更新 PR。
- 发布前 dry run。

不适用于：

- 未经授权直接合并 PR。
- 未经授权删除远端分支。
- 未经授权发布 GitHub Release。
- 在没看代码上下文的前提下靠猜测写结论。

## 仓库硬性规则

1. 任何结论必须基于真实代码、真实 diff、真实命令输出。
2. 开始前必须运行 `git status --short`。
3. 工作区有无法确认归属的脏改动时，不能偷偷带进 PR，也不能擅自删除。
4. 日常开发建议从 `main` 或维护者指定分支拉出新分支；如果项目建立 `dev` 分支，应按维护者规则以 `dev` 为基线。
5. 不得提交 `.env*`、私钥、日志、构建产物、真实数据库或私人截图。
6. 不得随意改变 Tauri command 名称、SQLite 表结构、同步 payload 形状。
7. 如果改动了同步、飞书、学习模式状态机，必须跑 Rust 测试。
8. 如果改动了前端类型或页面，必须跑 `npm.cmd test`。
9. 如果改动了公开文档或 package 元数据，必须检查 Markdown 链接和 package 信息。
10. 没有开发者明确授权时，AI 不得合并 PR、关闭 PR、删除远端分支或发布 Release。

## 标准执行顺序

### 阶段 1：环境确认与仓库现状检查

执行：

```powershell
git status --short
git branch --show-current
git remote -v
npm.cmd --version
cargo --version
```

确认：

- 当前路径是桌面端项目根目录。
- 是否存在未归属脏改动。
- 当前分支是否适合开发。
- 是否需要先同步远端。

### 阶段 2：阅读项目文档

AI 必须阅读或重新核对：

- [项目文件结构说明.md](./项目文件结构说明.md)
- [项目完整链路说明.md](./项目完整链路说明.md)
- [项目开发规范（AI协作）.md](./项目开发规范（AI协作）.md)
- 当前文件

涉及用户功能时，还要读 [FEATURES.md](./FEATURES.md)。

### 阶段 3：建立任务分支

推荐分支命名：

```powershell
git checkout -b docs/github-readiness
```

如果当前已有用户未提交改动，必须先确认是否继续在当前分支上开发，不能擅自切分支导致改动混乱。

### 阶段 4：开发与本地整理

开发时要求：

- 小步修改。
- 每一组改动有清晰职责。
- 不夹带无关格式化。
- 不把文档、重构、功能修复混在一个不可审查的大提交里。

建议提交拆分：

- `docs: add GitHub project documentation`
- `docs: add AI collaboration workflow`
- `chore: update package metadata for public repository`

### 阶段 5：本地验证

按改动范围选择验证命令：

```powershell
npm.cmd test
npm.cmd run build
cd src-tauri
cargo test
```

发布流程相关改动建议 dry run：

```powershell
npm.cmd run release:auto -- --dry-run --repo <owner>/<repo>
```

如果 dry run 因缺少真实仓库名失败，必须在回执里说明使用了什么占位值、失败点是什么。

### 阶段 6：提交前检查

执行：

```powershell
git status --short
git diff --stat
git diff --check
```

检查：

- 是否有私钥、日志、`.env*`、构建产物。
- 是否误改无关文件。
- Markdown 链接是否为相对路径。
- package 元数据是否和 README 一致。
- 文档是否存在乱码。

### 阶段 7：提交与推送

示例：

```powershell
git add README.md LICENSE CONTRIBUTING.md SECURITY.md
git add 项目文件结构说明.md 项目完整链路说明.md "项目开发规范（AI协作）.md" 开发者AI开发与PR提交流程.md
git add .github package.json package-lock.json
git commit -m "docs: prepare project for GitHub publishing"
git push -u origin docs/github-readiness
```

只有用户明确要求提交或推送时，AI 才执行这些命令。

### 阶段 8：创建 PR

创建 PR 前：

- 确认目标分支。
- 确认本地测试结果。
- 确认没有敏感文件。

PR 标题建议：

```text
docs: prepare project for GitHub publishing
```

PR 正文至少包含：

```markdown
## 本次改动

- 新增 README、LICENSE、贡献指南和安全说明。
- 新增 AI/维护者文档四件套。
- 新增 GitHub issue / PR 模板。
- 补充 package 公开仓库元数据。

## 风险与影响

- 不改业务逻辑。
- 不改数据库结构。
- 不改 Tauri command。

## 测试情况

- [ ] npm.cmd test
- [ ] npm.cmd run build
- [ ] cargo test
- [ ] release dry run
```

### 阶段 9：PR 后续

如果需要根据 review 修改：

- 先拉取最新远端分支。
- 只处理 review 指出的范围。
- 修改后重新跑相关测试。
- 回复 reviewer 时说明改了什么和测试结果。

## 最终反馈给开发者时必须说明

AI 完成任务后必须说明：

- 改了哪些文件。
- 关键内容是什么。
- 跑了哪些命令，结果如何。
- 哪些检查没有跑，原因是什么。
- 是否还有需要人工补充的内容，例如截图、真实 GitHub 仓库地址、Release 资产。

## 一句话执行要求

先读文档和真实代码，再小步修改；先检查脏工作区，再提交；先验证，再声称完成；没有授权，不合并、不发布、不删除。
