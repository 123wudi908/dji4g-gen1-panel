# 界面与维修功能检查（2026-09-30）

本轮保留 Rust/egui 和现有 Material 3 方向，缩小字体、控件与留白，完善已有交互，不增加装饰或新的维修流程。

## 界面与交互

- 常规字号收紧为 14，加载圈为 16；速率数字保留层级，未获取数据用较小的中性色文本。
- 按钮、导航、卡片及图表减少占用，统一悬停、键盘焦点、忙碌和禁用反馈。
- 短信编辑、发送确认、归档筛选和异步错误处理更加稳定；编辑中暂停会打断输入的刷新。
- 设置指令统一经过应用命令入口，派发失败可见；确认取消、过期重试和晚到结果不污染新的设备上下文。

## 维修功能修复

- 确认框直接使用完整动作参数：区分自动/静态 DNS、NDIS/ECM，列出实际 DNS 地址和 CID/APN。摘要缺少参数时使用中性名称。
- 自动 IPv4 DNS 显式设置修改 Flags，并用空字符串清除静态覆盖；回读识别空配置，冲突配置保持保守拒绝。
- DHCP 新鲜证据明确为关闭时禁用续租并给出原因，原生执行前仍重新核验，不把静态 IP 自动改成 DHCP。
- USB 配置要求新鲜 AT 证明，AT/PnP 操作可在没有网卡时准备。保存配置与实际模式生效分别解释，需手动重启后复检，不自动重启。
- 热点在执行前重新绑定确认的精确模块身份；修复探测监督器提前退出。WinRT 静态接口改为当前 apartment 内的新 factory，修复本机第二次枚举的原生崩溃。
- APN 空、非法或超长输入说明禁用原因。平台回读必须匹配 CID、APN 和 IP 类型，活动或不完整上下文不能写入。
- DHCP、网卡/模块重启、重新枚举的模糊超时不会仅因设备仍存在而升级为成功。网卡操作前后校验同一接口，恢复结果完整传到 UI。

## 验证

- `cargo test --workspace --all-targets --offline --locked`：1043 项通过，69 个测试程序；两个显式 fixture/主机回归默认跳过。
- 显式 Windows 只读回归：同线程连续八次初始化、真实枚举、释放对象并反初始化，通过。
- `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`、`cargo fmt --all --check`、`git diff --check` 通过。
- Release 单文件包独立两次校验，各通过 17 个内置资源。
- 界面使用真实 egui 渲染和模拟数据检查；输入与取消回归使用模拟端口，不产生设备写操作。

连接模块的只读检查确认：当前 NDIS 通路可访问网关和公网，静态 DNS 预设的两个服务器可解析查询；热点能力为 Enabled，当前状态为 Off。Debug 与 Release 探测都成功读取精确模块网卡状态。

未执行真实 USB 模式切换、DNS 改写、DHCP 续租、热点开关、APN 修改、网卡重启或设备重新枚举。代码、模拟端口和只读检查通过，不能据此认定所有真机写操作均通过。

只读 WinRT 回归需在 Windows 主机显式运行：

```powershell
cargo test -p dji4g-windows-platform --lib --locked hotspot::native::tests::profile_enumeration_survives_repeated_apartment_teardown -- --exact --ignored
```

本机设备 GUID、串口、日志、源码备份、构建缓存和临时产物不随本记录提交。
