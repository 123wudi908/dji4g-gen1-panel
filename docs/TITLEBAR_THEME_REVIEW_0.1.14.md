# v0.1.14 Windows 原生标题栏主题修复审查

> 最终提交说明：本文件保留对应开发阶段的审查记录，阶段性的测试统计、Git 状态、待授权说明和人工验收结论不代表最终提交状态。最终设置保存修复、补充 Release GUI 验收与仍未验证的边界以 RELEASE_NOTES_0.1.14.md 和本次 PR 正文为准。2026-10-08 已获得用户明确 Commit、Push 至个人 Fork 和创建 PR 的授权。

日期：2026-10-08。分支：`feature/update-check`。本次开发、测试和最终审查均由主智能体独立完成，没有使用子智能体。

结论：本次修复未发现代码审查阻断问题，建议用户确认后将本修复纳入 v0.1.14 的 PR。Windows 11 的原生窗口验收已完成；Windows 10 尚未实机测试，建议在合并前补验。已停止于待确认阶段，不 commit、push、创建 PR 或 Release。

## 问题及根因

修复前仍然存在问题。原生窗口回归在深色配置启动时失败：`startup before first frame: native dark=false, expected Dark`。截图确认界面内部已经深色，而原生标题栏仍为浅色。

`PanelApp::new` 和 `render_ui` 原先仅调用 `apply_settings_theme` 更新 egui 的颜色、字体和主题偏好，没有更新 Windows HWND 的非客户区主题。窗口框架因此继续使用原生窗口初始化时的系统主题。当前依赖 winit 的原生主题接口使用未公开的 `SetWindowCompositionAttribute`，本次选择直接通过平台层使用官方 DWM API。

## 本次增量文件

任务开始前已有未提交的自动更新改动，以下只列本次新增或增量修改。

| 文件 | 修改目的 |
| --- | --- |
| `apps/panel/src/app.rs` | 复用原有 HWND；构造阶段立即同步，界面应用主题后按句柄及实际深浅色变化同步。记录已尝试状态，避免每帧重复调用。 |
| `crates/windows-platform/src/window_theme.rs`（新增） | `DwmSetWindowAttribute`，四字节 BOOL，`DWMWA_USE_IMMERSIVE_DARK_MODE`，成功后刷新原生边框；失败返回 false，非 Windows 为无操作。附空/无效句柄降级回归。 |
| `crates/windows-platform/src/lib.rs` | 导出窗口主题平台模块，unsafe 仍留在平台层。 |
| `crates/windows-platform/Cargo.toml` | 为已有 windows-sys 开启 `Win32_Graphics_Dwm`。 |
| `apps/panel/tests/native_titlebar.rs`（新增） | 交互桌面专用真实 eframe 窗口验收，覆盖启动、实际设置控件、配置持久化和原生窗口操作。默认 ignored，须单独显式运行。 |
| `apps/panel/Cargo.toml` | Windows 测试依赖 winit，用于在 Rust 测试线程建立真实事件循环。 |
| `Cargo.lock` | 仅增加 panel 对已有 winit 包的依赖引用；与任务开始时相比，没有新增包或升级依赖版本。 |
| `docs/TITLEBAR_THEME_REVIEW_0.1.14.md`（新增） | 本报告。 |

没有改动版本、颜色 token、主题选择逻辑、原生装饰、图标或更新流程。仍使用 Windows 原生标题栏、原生最小化/最大化/关闭按钮。

## DWM 与降级边界

以现有 egui 已解析的 `visuals.dark_mode` 为准，包括“跟随系统”，不建立另一套主题系统。启动时不依赖首帧执行，因此沿用已有隐藏窗口启动架构。

优先请求当前 SDK 的属性 20；失败时尝试早期 Windows 10 曾使用的属性 19。属性 19 仅作为尽力兼容路径，不声称受到当前 SDK 文档保证。两次失败均被安全忽略，不向启动链返回错误，不改变窗口样式或系统主题。

[Microsoft 的 DWM 枚举文档](https://learn.microsoft.com/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)说明当前属性使用 BOOL，并将官方支持起点列为 Windows 11 build 22000。Windows 10 的属性接受情况和实际视觉效果必须实机验证；本机没有 Windows 10，未把兼容代码或无效句柄测试等同于 Windows 10 实测。

## 实际验收

环境：Windows 11 build 26100，系统应用为浅色主题，Windows MSVC x64。常规 shell/Node 沙箱助手初始化失败，经自动审核批准的本机命令通道完成编译和测试。Computer Use 的 Node 内核无法启动；采用仓库内测试驱动真实窗口，没有使用 headless 状态代替原生验收。

原生测试复用 `PanelApp::new/update`，通过实际绘制的设置下拉框文字定位，向原生 eframe 输入循环注入点击，再走真实 `UiCommand::SetTheme`、Controller 和 ConfigStore。设备执行器为测试环境，不连接设备、不驱动更新下载；所有 config 存放在 `target/titlebar-review` 的隔离目录，未改写用户配置。

| 场景 | 实际结果及证据 |
| --- | --- |
| 深色启动 | 在 CreationContext 中、首帧之前读回 DWM 深色；截图标题栏深色。 |
| 浅色启动 | 首帧之前读回 DWM 浅色；截图标题栏浅色。 |
| 深色 → 浅色 | 点击实际设置选项，UI 应用主题的同一帧 DWM 已同步；配置保存为 Light。 |
| 浅色 → 深色 | 点击实际设置选项，同一帧 DWM 已同步；配置保存为 Dark。 |
| 重新启动后恢复 | 深色、浅色各连续运行两个独立测试进程，读取各自已保存的同一配置文件，启动断言与实际标题栏均正确。 |
| 最大化、还原 | 向本测试 HWND 发送原生系统命令，`IsZoomed` 确认状态；主题保持一致。 |
| 最小化、恢复 | 原生系统命令最小化；测试线程用 `IsIconic` 确认真实状态，再恢复窗口；线程结果被主测试收取，恢复后主题正确。 |
| 文字、图标、按钮 | 原生 caption/system/min/max 样式与 WM_GETICON 读回正常；已查看深浅色启动、最大化、恢复截图，标题文字及三个原生按钮清晰可见。关闭按钮的显示已验收；窗口退出沿用已有行为，相关 tray/close 回归通过。 |
| 空/无效句柄 | 平台单元测试通过，失败返回 false，不 panic。 |

最终原生验收共四个独立进程全部通过，每个进程覆盖双向切换及原生窗口操作。截图采集加入前台 HWND 校验，拒绝被其他窗口遮挡的图像；早期一张遮挡截图已被有效重跑结果替换，不用作验收证据。

本地证据目录：`target/titlebar-review/`。日志：`native-dark-final-1.log`、`native-dark-final-2.log`、`native-light-final-1.log`、`native-light-final-2.log`。截图：`failed-before-fix.png`、`深色-startup.png`、`浅色-startup.png`、`深色-to-浅色.png`、`浅色-to-深色.png`、深浅色各自的 `maximized/restored` 图。

## 自动化门禁及增量审查

| 检查 | 实际结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过，无警告。 |
| `cargo test --workspace --locked` | 1153 passed、0 failed、6 ignored（含文档测试）。 |
| `cargo test --workspace --all-targets --locked` | 1148 passed、0 failed、5 ignored（含 examples，命令不运行文档测试）。 |
| 四次显式 native_titlebar 验收 | 每次 1 passed、0 failed；不计入以上默认 ignored 的统计。 |
| `packaging/tests/portable-update.ps1` | 12 项通过。 |
| `cargo build --workspace --release --locked` | 通过，未创建或发布 Release。 |
| `git diff --check` | 通过。 |

自动更新的版本选择、附件/摘要验证、实际 updater 子进程交接、启动失败回滚、helper 等待和 portable 包清单回归均通过。

任务开始时保存了原有未提交文件哈希和 diff。逐文件复核后，原有更新文件中仅 panel app/Cargo.toml/Cargo.lock 包含上述标题栏/测试接入增量；其余自动更新代码、CI、打包脚本、发布说明和既有审查报告字节一致。单独对照了 `app.rs` 的本次增量，并读取了两个新增 Rust 文件；没有修改更新首帧确认、后台轮询、归档安全退出或启动参数。

版本仍为 v0.1.14。待用户确认后再决定提交/PR；本报告不授权执行这些操作。

## 复跑方式

在可交互且未锁定的 Windows 桌面，分别执行以下命令，每条命令建立独立原生事件循环：

```powershell
cargo test -p dji4g-panel --test native_titlebar --locked native_dark_startup_and_theme_switches -- --ignored --exact --nocapture
cargo test -p dji4g-panel --test native_titlebar --locked native_light_startup_and_theme_switches -- --ignored --exact --nocapture
```

可将 `DJI4G_TITLEBAR_CONFIG` 设为专用测试目录；同一场景连续运行两次即覆盖新进程读取保存配置。可选的 `DJI4G_TITLEBAR_CAPTURE` 指向接收 HWND 与标签的截图脚本，本次本地脚本和全部图像保存在证据目录；不设置它也会执行 DWM、设置、持久化和窗口状态断言。
