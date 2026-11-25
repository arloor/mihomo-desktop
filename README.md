# Mihomo Desktop (GPUI)

基于 **Rust + GPUI (`gpui-kit`)** 构建的 Windows 桌面代理管理客户端，面向以 **Windows Service** 方式运行的 `mihomo` 核心提供原生 GPU 加速界面。

## 核心功能

1. **Windows Service 生命周期管理 (`src/system/service.rs`)**
   - 直接调用 Windows Service Control Manager (SCM) API 查询服务状态、PID、启动类型与二进制路径。
   - 支持一键 **启动 / 停止 / 重启** `mihomo` Windows 服务；优先直接调用 SCM API 免 UAC 执行，仅在权限不足时按需通过单次 UAC 提权执行。
   - 通过内置 `--service` 宿主运行 `mihomo` 服务，并可从 SCM 启动命令行自动同步核心路径、工作目录和生效配置文件路径。
2. **系统代理与开机自启 (`src/system/proxy.rs`, `src/system/autostart.rs`)**
   - 通过 Windows 注册表（`Internet Settings`）及 WinInet `InternetSetOptionW` 实时开关并刷新系统代理，支持自定义绕过列表（`ProxyOverride`）。
   - 通过 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 管理桌面应用开机自启。
3. **URL 配置导入与多配置管理 (`src/config/mod.rs`)**
   - 支持从远程订阅 URL 导入配置（自动解析 `Subscription-Userinfo` 流量配额与到期时间），支持一键更新、本地 YAML 导入及同步当前服务配置。
4. **TUN 模式与默认 TUN / DNS 自动补全**
   - 一键开启/关闭 TUN 模式，自动合并配置并调用 `PUT /configs?force=true` 热重载。
   - 当用户配置文件未提供 `dns` 或 `tun` 字段时，自动注入内置默认 `dns`（FakeIP + 国内外分流）与 `tun`（`Mihomo` / `system` / `mtu: 1492`）配置；支持预览合并后的运行时 YAML，并可选开启「强制覆盖原配置 TUN/DNS」。
5. **代理切换、外部资源、连接管理与核心管理**
   - **代理切换**：展示所有策略组与节点卡片，支持节点切换、单节点/全组延迟测速、延迟/名称排序与搜索、`URLTest`/`Fallback` 取消固定。
   - **外部资源**：查看与刷新 `proxy-providers`（含健康检查）、`rule-providers` 及搜索全部分流规则（`/rules`）。
   - **连接管理**：实时查看活跃连接、上下行流量、进程与规则链路，支持搜索、排序、暂停刷新、断开单条或全部连接。
   - **核心管理 (`src/system/core_manager.rs`)**：从 GitHub (`MetaCubeX/mihomo`) 拉取 Release 列表（支持走本地代理及筛选预发布版本），按架构下载解压 Windows 构建包，一键切换版本（自动停止服务 → 备份替换 `mihomo.exe` → 重启服务）。
6. **Mihomo 全量核心管理接口与实时日志 (`src/mihomo/mod.rs`, `src/runtime.rs`)**
   - WebSocket 实时流：`/traffic`（60 秒速率趋势图）、`/memory`、`/logs`（支持级别切换、过滤、暂停与清空）。
   - 管理接口控制台：`PUT /configs?force=true`、`POST /configs/geo`、`POST /cache/fakeip/flush`、`POST /cache/dns/flush`、`POST /restart`、`POST /upgrade`、`POST /upgrade/ui`、`PATCH /configs`（`allow-lan` / `ipv6` / `tcp-concurrent` / `unified-delay` / `sniffing` / `find-process-mode` / `log-level`）以及交互式 `GET /dns/query` 查询工具。

## 构建、运行与打包安装程序

```powershell
# 开发模式运行
cargo run

# 运行单元测试
cargo test

# 构建发布版并打包安装程序（自动复用本机 mihomo.exe 或通过 -CorePath 指定；可选 -GitHubMirror 加速 GEO 下载）
.\scripts\build-installer.ps1
```

### 安装程序与内置 Windows Service 说明

- `mihomo-desktop.exe` 本身内置原生 Windows Service 托管入口（无需第三方 WinSW）：
  - `--service --service-name mihomo --core <mihomo.exe> --work-dir <dir> --config <yaml>`：以 `LocalSystem` 权限看护 `mihomo.exe`（Windows Job Object 防孤儿进程 + 崩溃 5 秒自动重启 + 日志 10MB 滚动）。
  - `--install-service ...`：创建目录、部署核心、生成最小初始 `clash.yaml`（如已存在则保留）、注册并启动服务，同时配置服务 DACL 与 `ProgramData\Mihomo` 目录写权限，使普通权限桌面端日常启停服务、切换配置与开关 TUN **免弹 UAC**。
  - `--uninstall-service --service-name mihomo`：停止并卸载服务（保留用户配置）。
- `.\scripts\build-installer.ps1` 会在 `dist\stage\` 生成带 `Install.cmd` / `Uninstall.cmd` 的安装包；若本机已安装 Inno Setup 6（`winget install JRSoftware.InnoSetup`），还会自动编译单文件安装包 `dist\MihomoDesktop-Setup-0.1.0-x64.exe`。

## 环境要求

- Windows 10 / 11 (x64)
- Rust 工具链（支持 Edition 2024，即 Rust 1.85+）
- `mihomo` 核心可执行文件（可在应用内「核心管理」页面直接从 [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo) 下载）

## License

本项目采用 [MIT License](LICENSE) 开源。本项目仅作为 [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo) 的桌面管理客户端，不包含也不提供任何代理节点或订阅服务。
