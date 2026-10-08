# DJI 4G Panel v0.1.14 定时更新检测审查

> 最终提交说明：本文件保留对应开发阶段的审查记录，阶段性的测试统计、Git 状态、待授权说明和人工验收结论不代表最终提交状态。最终设置保存修复、补充 Release GUI 验收与仍未验证的边界以 RELEASE_NOTES_0.1.14.md 和本次 PR 正文为准。2026-10-08 已获得用户明确 Commit、Push 至个人 Fork 和创建 PR 的授权。

日期：2026-10-08。分支：`feature/update-check`。本次按开发、测试、主智能体审查顺序完成；未使用子智能体，未 commit、push、创建 PR、tag 或 Release。

## 本次修改文件

任务开始时工作区已存在更新功能与标题栏改动。本报告列出相对于任务开始快照的增量，未将已有改动计为本次实现。

| 文件 | 本次修改 |
| --- | --- |
| `apps/panel/src/update/service.rs` | `Instant` 固定 6 小时调度、在途检查互斥、保留已验证候选、失败节流、检查结果失效处理；下载前取消重查。 |
| `apps/panel/src/update/worker.rs` | 仍使用一次性后台 worker；检查请求改用现有 reqwest 异步接口与 Tokio，以 oneshot 取消等待。下载、校验、安装函数未改。 |
| `apps/panel/src/update/periodic_tests.rs` | 新增 13 项调度、状态、提示及生命周期回归测试。 |
| `apps/panel/src/app.rs` | 现有 `poll_updates` 调用服务 `tick`；显式退出期间仅消费事件，不开启新检查。 |
| `apps/panel/Cargo.toml` | 复用既有 Tokio 1.53.1，启用 `sync`；供通用更新模块使用，移至普通 dependencies。没有新增包或修改锁文件。 |
| `docs/UPDATE_PERIODIC_REVIEW_0.1.14.md` | 本报告。 |

workspace 版本仍为 `0.1.14`，MSIX 仍为 `0.1.14.0`；`Cargo.lock` 与任务开始快照 SHA-256 相同。GitHub 固定仓库、latest API、正式版本筛选、附件/URL/重定向/SHA/ZIP/manifest 安全规则均保持原值。设备检测、AT、驱动和 helper 业务源码无本次改动。

## 调度与界面行为

- 正常启动调用既有 `start`，立即后台检查一次；重复配置不会再触发启动检查。演示/未配置服务不会因 UI 轮询自动开启网络功能。
- UI 更新循环只非阻塞消费最多 8 个事件，并比较 `Instant`；距上次检查尝试开始满 21,600 秒才可发起下一次。记录请求或线程启动失败的尝试，因此失败不逐帧重试。
- 检查在途或旧任务尚未完成事件清理时不会重复启动；Downloading、Verifying、Ready、Installing 状态暂停新检查。忙碌解除后若已到期可检查，长时间暂停后不补发多轮请求。
- 定时发现更高的正式 Release，仍走既有 Available 状态与 GitHub 左侧下载图标。候选存在时重查不会短暂隐藏图标。用户不点击，仅保留提示。
- 失败保留候选和 Available 提示；成功返回无更新则清除候选并进入 Idle。自动检测不会调用下载或安装入口。
- 用户点击时沿用现有下载路径，先取消正在重查的请求；已经排队的旧检查结果不能覆盖下载状态或候选。安装仍要求用户点击既有“安装并重启”。
- 服务释放时 CheckTask Drop 发送取消信号，丢弃待完成请求 Future；无常驻调度线程，UI 不 join 网络线程。原生 DNS 的阻塞调用不能强制中断，使用 Tokio shutdown_background 避免等待它；取消后不会继续 Release 请求或重试，进程退出时操作系统回收剩余线程。

检查继续保留连接 10 秒、请求总计 20 秒、API 响应 2 MiB、HTTPS only、不跟随 API 重定向、既有 User-Agent 与 API 版本头。

## 测试与门禁

| 验证 | 实际结果 |
| --- | --- |
| 修复前回归 | “失败丢失提示”和“无更新保留旧候选”两项断言失败；取消结果未清理便开启下一轮的竞态断言也先失败后修复。 |
| 更新定向测试 | 20 项通过、0 失败，包含新增 13 项；时间参数直接推进至 21,599 / 21,600 / 43,200 秒，没有实际等待 6 小时。 |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 最终通过，退出 0。 |
| `cargo test --workspace --all-targets --locked` | 通过，退出 0；1,161 项通过、0 失败、5 项标记忽略，共 77 个测试目标结果。 |
| `cargo build --workspace --release --locked --target x86_64-pc-windows-msvc` | 通过，退出 0。复用隔离 target 目录，未覆盖正在运行的软件。 |
| `cargo deny check advisories licenses bans sources` | 四类均 ok，退出 0；保留既有策略警告，没有放宽规则。 |
| `packaging/tests/portable-update.ps1` | 通过，12 项。 |
| `packaging/tests/build-msix-dry-run.ps1` | 通过。 |
| 三组 `test-driver-completion / test-driver-install-command / test-driver-plan.ps1` | 均通过，分别 6 / 12 / 6 个模拟场景。未实际安装驱动。 |
| `git diff --check` | 通过。 |

全量测试包括原有版本/来源安全 9 项、下载存储/校验回归、updater 安全 18 项、进程/参数 8 项、真实 updater CLI 成功交接/启动失败回滚/helper 超时恢复 3 项。CLI 使用合成进程和临时安装目录，不操作用户生产安装或设备。

5 项标记忽略：2 项子进程夹具入口由上层回归实际调用；2 项原有标题栏原生 GUI 测试与 1 项 WinRT 网络清单人工测试未在本次运行。未进行真实 GitHub 断网/限流或 6 小时 GUI 驻留验收；调度、错误处理与提示行为由可控时间/事件/egui 回归验证。

测试日志、修改前快照与本次完整增量 diff 位于 Git 忽略的 `target/periodic-update-review/`。本机受限执行环境初始化失败，已使用普通执行环境运行获准的项目命令；MSVC 环境只在测试进程加载，未修改全局配置。

## 审查结论

已检查工作区已跟踪 diff 和新增更新/UI/updater 源码，并逐行复核本次增量、状态迁移、取消竞态与测试断言。发现并修复：

1. 检查失败清空已有新版本提示。
2. 成功确认无更新时没有同步清除旧候选。
3. 取消检查的已排队结果可能被误当成下一轮结果；现在必须在 poll 中回收旧任务并清理事件后才能开启下一轮。

本次增量没有未解决的阻塞问题。版本、来源与完整性校验保持原有约束；既有更新与业务回归通过。人工网络/原生 GUI 长时间运行验收不在已完成测试声明内。

HEAD 仍为 `1dc3de9f530c89f4c8c13c27dfb0bf0025a88068`，索引没有暂存改动。完成审查后停止，等待用户后续明确指令。
