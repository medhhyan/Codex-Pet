# Codex Pet Island — 后续工作交接

更新日期：2026-08-20  
当前安装包：`C:\Users\medhh\Documents\Codex Pet Island_0.1.36_x64-setup.exe`

## 项目概览

这是一个 Windows 桌面悬浮小工具，使用 **Tauri 2 + React + TypeScript**。它读取本机 Codex 会话日志和本地用量记录，在透明、置顶、可拖动的灵动岛中展示 Codex 的工作状态及任务概览。

窗口为 360×160，无系统边框、始终置顶，支持托盘隐藏/恢复、单实例和开机自启。

## 当前功能

- 三种总体状态：
  - `搬砖中`：任意近期任务仍在执行；
  - `任务完成`：没有执行中任务且有刚完成任务时显示，完成提示保留 8 秒；
  - `休息中`：没有有效执行中的 Codex 任务。
- 任务概览：按项目聚合显示进行中、已完成、待处理的项目；同一项目不会重复显示。
- 已完成项目可以在灵动岛内点击整行移除。移除记录会持久化，后续轮询不会再把该任务加回来。
- 本周使用比例、最近同步时间、下次重置时间来自 Codex 的本机写入记录；不再显示“今日 Token”。
- 托盘菜单包含恢复、动态效果开关、开机启动和退出。
- 透明度：鼠标移入更深，移开更浅。

## 当前视觉状态

- 采用浅蓝色玻璃背景和白色文字。
- 熊猫在左侧，状态/项目位于中间，使用信息位于右侧。
- 状态区只显示 `搬砖中`、`任务完成`、`休息中`，不显示额外说明文字。
- 已完成项目右侧显示圆形 `×`。圆标的背景为真正透明，直接透出灵动岛背景；因此鼠标移入时随整体变深、移开时随整体变浅。白色 `×` 用伪元素居中绘制。

## 关键文件

| 位置 | 作用 |
| --- | --- |
| `src/App.tsx` | 主页、总体状态、原生窗口拖动、完成状态确认、前端乐观移除任务。 |
| `src/components/TaskOverview.tsx` | 任务汇总、项目列表、已完成项目的点击移除按钮。 |
| `src/components/UsagePanel.tsx` | 本周用量、最近同步、下次重置显示。 |
| `src/styles.css` | 整个灵动岛布局、鼠标移入/移开透明效果、任务圆形 × 样式。 |
| `src/lib/pet-api.ts` | 前端调用 Tauri 命令和订阅状态事件。 |
| `src-tauri/src/app.rs` | Tauri 应用、托盘、单实例、自启、轮询、设置及已完成任务移除持久化。 |
| `src-tauri/src/codex_adapter.rs` | 读取 Codex JSONL 会话日志、判断任务状态、项目标题与用量。 |
| `src-tauri/tauri.conf.json` | 窗口尺寸、产品名、打包版本。当前版本为 `0.1.36`。 |
| `src/App.test.tsx` | 前端状态、透明模式、项目显示、点击移除、拖动的回归测试。 |

## 数据来源与规则

- 任务及状态从 Codex 本机 JSONL 会话日志解析。
- 用量从本机 rate-limit / usage 记录解析；只有 Codex 写入新的本地值时才更新，因此显示的是“最近一次写入本地的本周使用比例”。
- `rate_limits.primary.used_percent` 是本周使用比例；`rate_limits.primary.resets_at` 是下次重置时间。
- 工作状态优先级：只要存在 `working` 项目，总体状态必须是“搬砖中”；已完成提示不会覆盖仍在执行的项目。
- 任务行优先使用用户项目标题；找不到可靠标题时显示 `Codex 任务`，绝不显示 UUID、技术标签或会话内部 ID。

## 构建与测试

### 前端测试

```powershell
& 'C:\Users\medhh\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' 'node_modules\vitest\vitest.mjs' run src/App.test.tsx
```

### 重新生成前端页面

系统环境中 `npm` 命令不可用，因此不要依赖 Tauri 的默认 `beforeBuildCommand`。先手动运行：

```powershell
& 'C:\Users\medhh\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' 'node_modules\typescript\bin\tsc' -b
& 'C:\Users\medhh\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' 'node_modules\vite\bin\vite.js' build
```

### 打包 NSIS 安装程序

前端页面生成后，使用下列命令（不要省略 `--config`，它避免不存在的 `npm` 导致失败）：

```powershell
& 'C:\Users\medhh\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' 'node_modules\@tauri-apps\cli\tauri.js' build --bundles nsis --config '{"build":{"beforeBuildCommand":"cmd /c exit 0"}}'
```

产物位于：

```text
src-tauri\target\release\bundle\nsis\Codex Pet Island_<版本>_x64-setup.exe
```

打包后复制到：

```text
C:\Users\medhh\Documents\Codex Pet Island_<版本>_x64-setup.exe
```

**重要：** 每次前端改动后必须先手动重建 `dist`，再执行打包。此前 0.1.30 曾因跳过该步骤而出现“版本号已更新但界面仍是旧版”的问题。

## 已知注意事项

- 安装新版前，应从托盘菜单退出旧版桌宠，避免旧进程继续显示。
- Tauri 打包偶尔会在命令输出尚未完整显示时仍在后台进行 Rust 链接；确认 `src-tauri\target\release\bundle\nsis` 中对应版本的 `.exe` 已生成且时间戳稳定后再复制。
- 打包会提示 `codex_pet_island.pdb` 名称冲突，以及 `__TAURI_BUNDLE_TYPE` 警告；当前 NSIS 安装包仍可正常生成。这些警告尚未影响本项目使用。
- 任务同步依赖 Codex 的本机日志格式。若 Codex 改动日志格式或目录，需要优先检查 `src-tauri/src/codex_adapter.rs`。
- 已完成任务的移除是按 `turn_id` 持久化的，相关设置由 Tauri store 保存；若需要恢复已移除项目，可清理设置中的已移除任务 ID（实现位置见 `app.rs`）。

## 工作区清理建议

以下内容是构建或历史回归检查产生的体积大文件，不应提交到 Git：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `src-tauri/target-regression-*/`
- `src-tauri/gen/`
- `tsconfig.tsbuildinfo`
- `.worktrees/`

清理前请确认没有正在进行的构建或测试。源代码、素材、`package-lock.json`、`src-tauri/Cargo.lock`（如存在）、配置与测试应保留。

## 下一步建议

项目当前已可以交付和使用。若继续迭代，推荐顺序：

1. 仅在用户确认需要时微调布局或字体；
2. 给“已完成项目移除”增加可选的撤销入口；
3. 为 Codex 日志格式变动增加更多样本测试；
4. 整理 `.gitignore`，然后通过 GitHub Desktop 提交源代码与本文件，不提交构建缓存和安装包。
