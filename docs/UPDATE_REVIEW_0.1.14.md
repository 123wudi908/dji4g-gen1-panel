# 0.1.14 自动更新提交前审查报告

> 最终提交说明：本文件保留对应开发阶段的审查记录，阶段性的测试统计、Git 状态、待授权说明和人工验收结论不代表最终提交状态。最终设置保存修复、补充 Release GUI 验收与仍未验证的边界以 RELEASE_NOTES_0.1.14.md 和本次 PR 正文为准。2026-10-08 已获得用户明确 Commit、Push 至个人 Fork 和创建 PR 的授权。

日期：2026-10-08。范围：`feature/update-check` 当前全部未提交改动。本报告的最终审查、修复和验证由当前主智能体完成；收到禁止子智能体指示后未再调用子智能体。附件为需求依据，其中提交与发布步骤须等待后续明确授权。

**结论：暂不建议提交。** 实现及自动化门禁完成，但用户要求的真实 GitHub 下载、窗口响应和设备人工验收尚未完成。当前没有已知未修复的代码审查阻断问题。交付后停止，等待人工确认，不提交或发布。

## 修改的文件及目的

| 已修改文件（12 个） | 目的 |
| --- | --- |
| `.github/workflows/ci.yml` | 增加 portable updater 打包回归；发布条件及发布命令未改动、未执行。 |
| `Cargo.toml` | workspace 改为 0.1.14，注册独立 updater。 |
| `Cargo.lock` | 同步成员版本与所需依赖，保留既有外部依赖版本。 |
| `apps/panel/Cargo.toml` | 更新服务与 ZIP/SHA 测试依赖。 |
| `apps/panel/src/app.rs` | 顶部入口、进度、非阻塞轮询、空闲门槛、既有安全退出和首帧确认。 |
| `apps/panel/src/lib.rs` | 导出 update 模块。 |
| `apps/panel/src/localization.rs` | 简体、繁体、英文更新文案及完整性计数。 |
| `apps/panel/src/main.rs` | 后台检查、updater 专用启动参数、失败恢复提示。 |
| `apps/panel/src/ui/mod.rs` | 注册更新 UI。 |
| `docs/THIRD-PARTY-NOTICES.txt` | 更新直接依赖来源与许可证说明。 |
| `packaging/msix/Package.appxmanifest` | 同步 0.1.14.0，满足既有版本门禁。 |
| `packaging/scripts/build-portable.ps1` | 加入 updater，保持实际附件名与 SHA sidecar。 |

| 新增文件（20 个） | 用途 |
| --- | --- |
| `apps/panel/src/update.rs` | 版本选择、附件来源、摘要解析、流边界和启动确认。 |
| `apps/panel/src/update/service.rs` | UI 状态机、有限队列、后台调度和 updater 存活监测。 |
| `apps/panel/src/update/worker.rs` | 下载、落盘校验、复制已安装 updater、独立进程交接。 |
| `apps/panel/src/ui/update.rs` | 顶部下载箭头、提示、百分比/大小、失败及安装入口。 |
| `apps/panel/tests/update_release.rs` | 版本、附件、来源、摘要、下载中断和启动参数回归。 |
| `apps/updater/Cargo.toml` | 独立 updater 最小依赖。 |
| `apps/updater/src/lib.rs` | 固定发布文件清单、错误类型、只读 ZIP 校验入口。 |
| `apps/updater/src/main.rs` | 命令行入口、错误 marker、退出状态。 |
| `apps/updater/src/archive.rs` | 整包 SHA、ZIP 安全、manifest 哈希及受限解压。 |
| `apps/updater/src/install.rs` | 路径预检、独占锁、同卷备份/替换与回滚。 |
| `apps/updater/src/process.rs` | Windows 进程身份、等待、重启、首帧确认。 |
| `apps/updater/src/session.rs` | 参数/会话验证与完整更新编排。 |
| `apps/updater/tests/safety.rs` | 18 项 ZIP、路径、摘要、锁、占用、回滚和数据保护回归。 |
| `apps/updater/tests/process_session.rs` | 进程/参数/启动确认，及由测试调用的子进程夹具。 |
| `apps/updater/tests/cli_handoff.rs` | 真实 updater 子进程成功交接、启动失败回滚、helper 超时恢复。 |
| `packaging/tests/portable-update.ps1` | 实际打包脚本的 12 项内容、清单、SHA 和缺失 updater 回归。 |
| `docs/RELEASE_NOTES_0.1.14.md` | 新功能、失败不影响原有业务、适用范围和恢复边界。 |
| `docs/superpowers/specs/2026-10-08-in-app-update-design.md` | 设计边界、信任来源及架构。 |
| `docs/superpowers/plans/2026-10-08-in-app-update.md` | 开发验证步骤。 |
| `docs/UPDATE_REVIEW_0.1.14.md` | 本报告。 |

## 完整更新流程

1. 正常启动时独立后台线程请求固定仓库 `zhu-hailin/dji4g-gen1-panel` 的 latest Release API。失败仅日志记录，回到无更新状态，不发设备或 helper 命令。
2. semver 解析 `0.1.14`/`v0.1.14`，忽略 draft、API prerelease 和语义预发布版本。按版本优先级比较，构建元数据不构成更新。
3. 只接受已核对 CI/packaging 的 `dji4g-panel-windows-x64-portable.zip` 与 `.zip.sha256`，要求附件唯一、大小合理、URL 精确匹配仓库/tag/文件名。
4. 新版本在顶部 GitHub 左侧显示现有圆形样式下载箭头，悬停“发现新版本 v…”。相同/旧版本隐藏；点击软件内下载，不打开浏览器。
5. 后台读取 sidecar，流式下载到本方临时会话；UI 有限非阻塞轮询显示百分比及已下载/总 MiB。只有 SHA、ZIP、manifest 全部通过才可安装。
6. 点击“安装并重启”要求串口任务、确认窗口、业务操作、驱动安装、支持报告均空闲。仅正式 portable 布局支持原地安装，MSIX/开发目录拒绝。
7. 将当前安装目录中可信 updater 复制到临时会话，以分离 argv 传 Panel PID、ZIP、SHA、安装目录和会话目录。Panel 后台等待准备。
8. updater 持有安装独占锁，验证精确 Panel/helper 映像，重新验包，安装目录内同卷暂存，全部准备后写 ready。
9. Panel 保留 Child 并持续检查存活。退出/过期撤销交接；成功才走既有 explicit exit，完成短信归档本地 I/O 后关闭。
10. updater 等待精确 Panel 句柄退出，再枚举并等待同目录 helper，统一有界等待，不杀旧进程。helper 超时不触碰发布文件，若 Panel 已退出则重启旧版。
11. 只替换固定 8 个发布文件，保留同卷旧备份。启动新 Panel，等待首次 egui update 的 startup-ok。成功清理本方安装事务并退出。
12. 启动失败先停止且确认停止本次新子进程，再恢复旧文件、重启旧版并提示。无法确认停止或回滚被阻止则保留备份、返回 RECOVERY_REQUIRED，不继续制造混合版本。

## 版本修改

workspace/Cargo.lock 全部 8 个成员均为 **0.1.14**：application、at-protocol、domain、helper、ipc、panel、updater、windows-platform，均使用 `version.workspace = true`。MSIX 为 **0.1.14.0**。

对照 HEAD，Cargo.lock 新增 38 个外部包版本，未删除或替换任何既有外部依赖版本。所有门禁使用 `--locked`。原有设备/AT/PnP/驱动业务 crate 源码未改动。

## 自动化测试与构建

| 最终检查 | 实际结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过，退出 0。 |
| `cargo test --workspace --all-targets --locked` | 通过，退出 0；1,146 项通过、3 项标记忽略，76 个测试目标结果。 |
| `cargo build --workspace --release --locked --target x86_64-pc-windows-msvc` | 通过，退出 0；独立 CARGO_TARGET_DIR 避免覆盖正在运行的旧 Panel。 |
| `cargo deny check advisories licenses bans sources` | 通过，四类均 ok；官方 cargo-deny 0.20.2，已有 advisory 例外/重复依赖策略未放宽。 |
| `packaging/tests/build-msix-dry-run.ps1` | 通过。 |
| `packaging/tests/portable-update.ps1` | 通过，12 项。 |
| `packaging/scripts/test-driver-completion.ps1` | 通过，6 场景，无硬件修改。 |
| `packaging/scripts/test-driver-install-command.ps1` | 通过，12 场景，模拟命令退出码，无驱动安装。 |
| `packaging/scripts/test-driver-plan.ps1` | 通过，6 场景，无硬件修改。 |
| 实际 portable ZIP | 最终 release 二进制打包通过，8 文件内容/清单/SHA 一致。 |
| 实际 ZIP 与 updater 兼容 | 临时只读验证程序调用最终 updater 库，整包与 7 个 manifest 摘要通过；未运行安装。 |
| 实际未签名 MSIX + `verify-release.ps1` | 通过：身份、整包、Panel/helper 摘要及多余二进制限制。 |
| `git diff --check` | 通过，无空白错误。 |
| 可选 `test-local-driver-plan.ps1` | 未完成：缺少真实且签名符合要求的本地驱动包目录输入，没有伪造包或放宽校验。 |

3 项标记忽略中，2 项是子进程夹具入口，上层测试实际用 `--ignored --exact` 调用；另 1 项是原有需要人工 Windows 网络环境的 WinRT 清单测试，未运行。不是“所有测试都已执行”的声明。

更新专用证据：版本/来源 9 项、UI 2 项、下载存储 2 项、过期/失败交接撤销 2 项、归档退出 1 项、伪造会话不删文件 1 项、updater 安全 18 项、进程/参数 8 项、真实 CLI 3 项。CLI 使用运行时编译的合成 Panel/helper，实际测试 Windows 等待及文件替换，没有改写用户安装或连接设备。

本机 MSVC 环境问题用测试进程加载编译工具环境解决，未改全局配置；默认 release 二进制被现有 Panel 占用，改用隔离目录，未关闭用户进程。实际打包产物在 Git 忽略的 `target/update-review-0.1.14-portable` 与 `target/update-review-0.1.14-msix`。MSIX 使用既有脚本的临时副本，仅调整仓库解析和输出目录；内容/签名逻辑未改，正式脚本未改。MSIX 未签名、未安装、未发布。

## 人工测试结果

**真实 GUI/GitHub/设备人工验收未完成。** 当前原生窗口自动化不可用，GitHub API 请求遇到 403 限流。没有把合成夹具称作人工测试，也未操作当前生产安装或系统网络来制造验收结果。

| 要求场景 | 已有自动化证据 | 人工状态 |
| --- | --- | --- |
| 相同版本无图标 | semver/egui 回归通过 | 待验收 |
| 较高版本有图标 | 解析/可点击入口通过 | 待验收 |
| 点击内部下载且 UI 不冻结 | 无浏览器输出；网络/落盘/等待均在线程内，UI 有限轮询 | 待真实下载和窗口操作 |
| 下载后 SHA 正确 | sidecar/整包/manifest/实际 ZIP 通过 | 待真实 GitHub 下载 |
| Panel 退出后才替换 | 实际 updater CLI 等待合成 Panel 通过 | 待真实 Panel |
| helper 不导致文件占用 | 实际同目录 helper 等待/超时保护通过 | 待真实 helper/UAC/设备 |
| 完成后自动重启 | 合成新版启动/确认通过 | 待 GUI 与设备恢复 |
| 无网络原程序正常 | 检查错误静默 Idle，服务不调用业务 | 待断网验收 |
| 下载中断旧版可运行 | 短流/错误拒绝、清理、不覆盖通过 | 待网络中断 |
| SHA 错误拒绝安装 | 错误摘要拒绝并删本方无效会话 | 待 GUI 提示 |
| 更新失败保护旧版 | 占用回滚、失败启动旧版重启、helper 超时不覆盖通过 | 待真实安装目录 |

## updater 安全和 SHA 流程

- 来源固定，不接受其他仓库/tag/近似文件名、重复附件、HTTP、用户口令 URL、非 443 端口。最多 5 次重定向，只接受 github.com 和两个 GitHub release CDN 主机。
- 连接 10 秒、API 20 秒、sidecar 30 秒、ZIP 下载 15 分钟；API 2 MiB、sidecar 4 KiB、ZIP 512 MiB、展开 1 GiB、manifest 64 KiB。实际下载长度必须与发布记录相等。
- sidecar 必须唯一指定准确 ZIP 名和 64 位十六进制摘要。Panel 落盘 sync 后计算整包 SHA 并验证 ZIP/manifest；updater 持不可写共享句柄再计算 SHA、核对每文件摘要，然后才解压/安装。
- 固定 8 个平面文件名；拒绝目录、穿越、绝对路径、symlink、加密、重复中央目录、额外文件、展开超限。仅 create_new 写入本方空目录，防 Zip Slip。
- 只接受普通绝对目录，检查祖先和文件的 symlink/reparse；分离 argv 不拼 shell，不请求提权。
- 进程句柄与完整映像路径抵抗 PID 重用。helper 按安装路径识别，只等待旧进程；只可能终止本次刚启动但失败的新 Panel。
- share_mode(0) 安装锁贯穿事务；既有锁文件不截断、不删除，退出后可复用。
- 仅遍历固定 PAYLOAD，配置、日志、归档与其他运行数据不在替换列表。归档完成才退出。CLI 启动确认仅写 marker，绝不据此递归删目录。

## 失败如何保护旧版

检查/下载/校验/updater 启动或准备失败：Panel 保持运行，旧文件未动；无效下载由本方 TempDir 清理。旧 Panel 退出后 helper 超时：不杀 helper、不替换文件，重启旧版。

文件替换或新启动失败：确认新子进程停止后回滚，仅移除本次已安装文件，恢复本次备份，再重启旧 Panel。备份留在同卷事务目录。无法确认新进程停止时保留完整新内容和旧备份，不执行部分回滚；回滚被占用时不覆盖被占目标、保留备份及错误。此异常下不能承诺自动恢复成功。

## 关键 diff 审查

| 发现 | 最终修正及证据 |
| --- | --- |
| Installing 遮挡归档安全退出 | explicit_exit 继续既有退出路径，异步归档后 Close 回归通过。 |
| 启动参数诱发删除非本方目录 | 启动确认端不清理目录，伪造会话保留用户文件通过。 |
| ready 后 updater 已死仍关闭 Panel | 保留 Child，轮询和消费前检查；真实死子进程/失败事件撤销回归通过。 |
| 未确认新 Panel 停止便回滚 | 所有无法确认停止归为 PROCESS_UNSTOPPED，保留完整新文件/旧备份；回归通过。 |
| updater 并发替换 | 独占锁贯穿事务，第二实例拒绝通过。 |
| helper 等待失败后旧 Panel 未重启 | 安装前失败路径重启未动的旧版，真实 60 秒 CLI 回归通过。 |
| 本地化键清单计数遗漏 | ALL 含新键，同步断言至 1283；各语言/占位符回归通过。 |

原有设备识别、PnP、AT、helper 权限、驱动安装、网络诊断源码没有重构。updater 没有这些业务 crate 依赖，不发设备/AT/驱动命令。Panel 只复用既有空闲与安全退出设施。业务回归通过，真实硬件仍待人工验收。

## 依赖用途与许可证

| 直接接入依赖 | 用途 | 许可证 |
| --- | --- | --- |
| reqwest 0.12.28 | HTTPS 请求/超时/受控重定向，blocking API 运行在线程内 | MIT OR Apache-2.0 |
| semver 1.0.28（复用已锁定包） | 语义版本比较 | MIT OR Apache-2.0 |
| tempfile（复用已锁定包） | 下载会话、同卷事务、测试目录 | MIT OR Apache-2.0 |
| sha2 0.10.9 | 整包与文件 SHA-256 | MIT OR Apache-2.0 |
| zip 6.0.0 | ZIP 校验/解压、测试构造 | MIT |
| serde / serde_json（复用） | Release 与 manifest 解析 | MIT OR Apache-2.0 |
| windows-sys 0.61.2（复用） | 进程句柄/等待/Toolhelp | MIT OR Apache-2.0 |
| 本仓库 dji4g-updater 0.1.14 | 独立进程和受限校验库 | MIT OR Apache-2.0 |

ZIP 仅启用所需 deflate；reqwest 关闭默认特性，使用 native-tls 系统信任。新增 Windows 包的已知 rust-version 不超过 workspace 1.85；本次实际验证为 stable 1.99，不宣称已验证 1.85。

全部 38 项新增外部锁文件包许可证如下；Windows 图涉及 35 项，另外 arbitrary、derive_arbitrary、wasi 属于其他目标/可选解析。依据 Cargo metadata 和 cargo-deny；三个非 Windows 包另外核对 [arbitrary](https://crates.io/api/v1/crates/arbitrary/1.5.0)、[derive_arbitrary](https://crates.io/api/v1/crates/derive_arbitrary/1.5.0)、[wasi](https://crates.io/api/v1/crates/wasi/0.11.1+wasi-snapshot-preview1) 官方元数据。

| 包 | 版本 | SPDX 许可证 |
| --- | --- | --- |
| arbitrary | 1.5.0 | MIT OR Apache-2.0 |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| derive_arbitrary | 1.5.0 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| http | 1.5.0 | MIT OR Apache-2.0 |
| http-body | 1.1.0 | MIT |
| http-body-util | 0.1.5 | MIT |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| hyper | 1.12.0 | MIT |
| hyper-tls | 0.6.0 | MIT/Apache-2.0 |
| hyper-util | 0.1.21 | MIT |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |
| mio | 1.2.4 | MIT |
| reqwest | 0.12.28 | MIT OR Apache-2.0 |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| sync_wrapper | 1.0.2 | Apache-2.0 |
| tokio-native-tls | 0.3.1 | MIT |
| tower | 0.5.3 | MIT |
| tower-http | 0.6.11 | MIT |
| tower-layer | 0.3.3 | MIT |
| tower-service | 0.3.3 | MIT |
| try-lock | 0.2.5 | MIT |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| want | 0.3.2 | MIT |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |
| zip | 6.0.0 | MIT |

## 安全风险及尚未解决的问题

1. 用户要求的 11 类真实人工验收未完成，尤其真实 GitHub、UI 响应、helper/UAC、重启后设备与 AT 恢复；应在测试 portable 目录补齐。
2. 同源 SHA sidecar 只保证完整性，不提供独立发布者签名；信任 GitHub HTTPS 和上游发布权限。
3. 逐文件替换与进程内回滚不是断电原子事务。磁盘损坏、杀毒隔离、强制终止或恢复文件占用可能需要保留备份的人工恢复。
4. 成功交接后的系统临时下载会话保留用于诊断，避免从 CLI 推导删除权限，可能占用一个下载包大小；没有跨会话自动清理。本方成功安装事务会删除，失败事务保留。
5. 同用户权限恶意进程可以修改安装目录或伪造 marker；路径预检不是同权限隔离沙箱。
6. startup-ok 只证明首次 egui update，不证明设备初始化、后续图形呈现或长期健康。
7. 0.1.13 没有 updater，需先手动安装 0.1.14 portable 才能用于后续版本升级。MSIX、完整离线驱动包、standalone 不支持本版原地更新。
8. 可选本地驱动包回归、原有人工 WinRT 清单测试缺少输入/环境，明确未完成，没有放宽它们的校验。

## 提交内容检查

对所有修改和新增文件扫描，未发现实际 GitHub token/PAT/私钥、本机用户或工作区路径硬编码、exe/zip/msix/dll/pdb 等生成文件。测试里的虚拟 Unicode/空格路径只作参数夹具。测试专用代码在 cfg(test) 或 tests/，没有新增生产调试入口。下载、合成测试 EXE/ZIP、编译产物、临时验证脚本和日志均在 Git 忽略的 target/ 或 .superpowers/。CI 原有 GH_TOKEN 是 secret 表达式占位，没有添加凭证值。扫描不能替代人工秘密审查，关键新增源码与 diff 已阅读。

## Git 状态与 diff

当前分支 `feature/update-check`。HEAD 与 main 均为 `1dc3de9f530c89f4c8c13c27dfb0bf0025a88068`。索引为空。未 commit/push/PR/tag/Release，remote 未修改，未向原作者仓库写入。工作区 12 个修改文件和 20 个新增文件。
```text
 M .github/workflows/ci.yml
 M Cargo.lock
 M Cargo.toml
 M apps/panel/Cargo.toml
 M apps/panel/src/app.rs
 M apps/panel/src/lib.rs
 M apps/panel/src/localization.rs
 M apps/panel/src/main.rs
 M apps/panel/src/ui/mod.rs
 M docs/THIRD-PARTY-NOTICES.txt
 M packaging/msix/Package.appxmanifest
 M packaging/scripts/build-portable.ps1
?? apps/panel/src/ui/update.rs
?? apps/panel/src/update.rs
?? apps/panel/src/update/
?? apps/panel/tests/update_release.rs
?? apps/updater/
?? docs/RELEASE_NOTES_0.1.14.md
?? docs/UPDATE_REVIEW_0.1.14.md
?? docs/superpowers/plans/2026-10-08-in-app-update.md
?? docs/superpowers/specs/2026-10-08-in-app-update-design.md
?? packaging/tests/portable-update.ps1
```

`git diff --stat` 仅统计已跟踪文件，新增文件另见上述清单：
```text
 .github/workflows/ci.yml             |   4 +
 Cargo.lock                           | 443 ++++++++++++++++++++++++++++++++++-
 Cargo.toml                           |   4 +-
 apps/panel/Cargo.toml                |   8 +
 apps/panel/src/app.rs                | 144 +++++++++++-
 apps/panel/src/lib.rs                |   2 +
 apps/panel/src/localization.rs       |  99 +++++++-
 apps/panel/src/main.rs               |  14 ++
 apps/panel/src/ui/mod.rs             |   2 +
 docs/THIRD-PARTY-NOTICES.txt         |  10 +
 packaging/msix/Package.appxmanifest  |   2 +-
 packaging/scripts/build-portable.ps1 |   2 +-
 12 files changed, 720 insertions(+), 14 deletions(-)
```

**暂不建议提交。** 完成自动化与审查后在此停止；补齐人工验收并由用户明确确认后再考虑提交。
