# Codex Pet Island

一个运行在 Windows 桌面的 Codex 状态桌宠。它会从本机 Codex 记录中读取工作状态、项目进度和本周用量，并以透明置顶的“灵动岛”形式显示。

> 本工具只读取本机 Codex 数据，不连接或读取 ChatGPT 对话内容。

## 下载与安装

请前往本项目的 [Releases](../../releases) 页面，下载最新的：

```text
Codex Pet Island_<版本>_x64-setup.exe
```

安装步骤：

1. 如果电脑上已运行旧版，请在系统托盘中右键桌宠图标并选择 **Quit**；
2. 双击下载的 `x64-setup.exe`；
3. 完成安装后，桌宠会出现在桌面右下角；
4. 如未显示，请在 Windows 开始菜单搜索并打开 **Codex Pet Island**。

## 功能

- **搬砖中**：存在正在执行的 Codex 任务；
- **任务完成**：任务完成后显示短暂提示；
- **休息中**：当前没有执行中的 Codex 任务；
- 显示项目数量和各项目的进行中、已完成、待处理状态；
- 点击已完成项目所在行，可从桌宠列表中移除该项目；
- 显示最近一次写入本机的本周使用比例、最近同步时间和下次重置时间；
- 无边框、透明、置顶，可通过顶部 `CODEX` 区域拖动位置；
- 可隐藏到托盘，并从托盘恢复；
- 支持开机启动、动态效果开关和单实例运行；
- 鼠标移入时背景变深，移开时更透明。

## 使用提示

- 点击右上角的星形按钮可打开或关闭动态效果；
- 点击右上角的横线按钮可隐藏到系统托盘；
- 在托盘图标上双击，或从右键菜单选择 **Restore**，可恢复桌宠；
- 已完成项目的 `×` 圆标背景是透明的，会随整个灵动岛背景的深浅变化；
- 用量数据取决于 Codex 是否已把新的用量记录写入本机，因此可能不会与界面即时变化完全同步。

## 从源码运行

### 环境

- Windows 10/11（64 位）
- Node.js
- Rust（MSVC 工具链）
- Tauri 2 所需的 WebView2 运行时（通常 Windows 已内置）

### 安装依赖并启动开发版

```powershell
npm install
npm run tauri dev
```

### 构建安装程序

```powershell
npm run build
npm run tauri build -- --bundles nsis
```

构建完成后，安装包通常位于：

```text
src-tauri\target\release\bundle\nsis\
```

本项目当前开发环境如无法识别 `npm` 命令，请参考 [HANDOFF.md](HANDOFF.md) 中使用项目内 Node 运行时的构建说明。

## 项目结构

```text
src/                 React 界面
src-tauri/src/       Tauri / Rust 后端与 Codex 日志适配
src-tauri/icons/     应用图标
HANDOFF.md           后续维护与打包交接说明
```

## 隐私

桌宠仅读取本机 Codex 的会话日志和本地用量记录，用于生成状态与项目概览；不会上传这些内容。

## 许可证

请根据自己的发布计划补充许可证文件（例如 MIT License）。
