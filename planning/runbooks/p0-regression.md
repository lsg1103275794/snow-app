# P0 运行手册：本地升级、记忆与授权回归

用途：在变更存储、MCP 配置、记忆或审批键之后，重跑 fork 的第一批合成回归。本手册是操作说明，不是发行放行或 PR 门禁已经生效的证明。当前证据见[本机验收记录](../verification/2026-11-21-p0-regression.md)；所有分类入口见[根路线图](../../ROADMAP.md)。

## 1. 环境与命令

从包含 `package.json` 的工作树根目录执行。推荐并已验证 Node 24 LTS（24.12.0）、npm 和 x64 MSVC Rust 工具链；Node 原生执行单测中导入的可擦除 TS 类型，无需自建转译器。Rust 读取 `native/Cargo.lock`；首次需要取得已锁依赖，**运行测试本身不需要在线服务、GitHub Token、模型凭据或 Electron**。

Windows 先打开 **Developer PowerShell for Visual Studio（x64）** 并切到工作树。若普通 PowerShell 链接时报缺 `msvcrt.lib`，而 VS Build Tools + Windows SDK 已安装，可用以下命令在**当前进程**载入官方环境，不改系统设置：

```powershell
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
Import-Module (Join-Path $vs 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64'
```

在工作树内：

```powershell
npm run check:p0
```

该入口先跑 Node 的客户端契约，再跑 `snow-storage-services` 的两个 Rust 集成测试目标；**任一步失败，退出码非零**。它不执行 `git`，不发布，不修改迁移版本，也不隐式安装 npm 包。拆开排错：

```powershell
node --test tests/p0/approval.test.mjs
cargo test --locked --manifest-path native/Cargo.toml -p snow-storage-services --test p0_upgrade
cargo test --locked --manifest-path native/Cargo.toml -p snow-storage-services --test p0_memory
```

需要项目级补充检查时执行 `npm ci --ignore-scripts --no-audit --no-fund`（**仅在新工作树**；会重建该工作树的 `node_modules`），然后 `npm run check:ts`、`npm run check:docs`。这两个现有命令独立于定向回归。`check:docs` 校验 `docs/` 下的双语用户手册，不用它代替 fork 的规划链接检查。

## 2. 场景、输入与判据

| 类别           | 实际输入                                                                                                                                                                                                                        | 如何判定回归                                                                                                                                                                                     |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 数据库启动     | 不存在的库；[v69 历史表切片](../../native/crates/snow-storage-services/tests/fixtures/p0-upgrade/v69.sql)；叠加[v70 反馈表与记录](../../native/crates/snow-storage-services/tests/fixtures/p0-upgrade/v70-feedback.sql)的旧库。 | 调用产品 `ensure_database` 到 v71，检查 WAL、外键、SQLite 完整性、反馈及其索引、MCP HTTP 旧行与旧项目 JSON 的 OAuth 缺省关闭、请求头与记忆来源锚点。再次启动不得丢数据或重置已显式选中的 OAuth。 |
| 升级前的名单   | v69 项目级旧免审名单和[旧子代理的工具列表](../../native/crates/snow-storage-services/tests/fixtures/p0-upgrade/v69-agent-tools.sql)。                                                                                           | 升级及重开后，生产读取服务只返回原有工具名，不补整组许可、不串到另一个项目。这里不发起任何工具调用。                                                                                             |
| 记忆与会话     | 两个合成项目、全局域、父/子会话、不同回复锚点；[用例](../../native/crates/snow-storage-services/tests/p0_memory.rs)。                                                                                                           | 同域同名合并保留 ID，跨域不合并；关键词只命中项目+全局；迁域保持 ID；快照首次写入胜出。选出的回滚目标和其他回复分开；会话删除传 `false` 保留、传 `true` 才连删。                                 |
| 客户端审批契约 | [Node 测试](../../tests/p0/approval.test.mjs)直接导入真实 `toolActionPolicy.ts`；桩仅提供两个工具的只读动作元数据。                                                                                                             | 两种工具的只读键、写键、跨动作隔离、未知/无效输入与重试/缓存按当前契约返回；不是对完整审批执行或后端拦截的断言。                                                                                 |

## 3. 数据与产物放在哪里

- 测试只创建随机、独占的系统临时目录和合成 SQLite 文件。共享夹具在测试进程内恢复标准 panic 输出，失败不写个人的 Snow panic 日志；释放 SQLite 连接后再清理本用例的目录。不要把任何个人 `.snowapp` 路径传给测试。
- `tests/p0/` 放 Node 契约测试；Rust 的代码与 SQL 夹具放在 [snow-storage-services/tests/](../../native/crates/snow-storage-services/tests)。不提交数据库文件、凭据、`native/target`、`node_modules` 或整份终端日志。
- `planning/runbooks/` 只放可复现步骤；`planning/verification/` 只放某次验证所用基线、命令、结果和未覆盖项。上游发布史不在这两类文档里重复维护。

## 4. 失败后怎么处理

1. **Cargo 离线提示找不到包**：先用已锁定的正常命令取得依赖，再按需加 `--offline`；这不是测试断言失败。不要为此删除锁文件或修改真实数据库。
2. **Windows 链接缺 CRT/SDK 库**：检查开发者 PowerShell 的 `LIB` 和 x64 工具链；不要复制系统库进仓库。空间有限可在此 PowerShell 设置 `$env:CARGO_PROFILE_TEST_DEBUG='0'` 和 `$env:CARGO_PROFILE_DEV_DEBUG='0'` 以减少本机调试产物。
3. **回归断言失败**：单跑失败目标，区分夹具漂移、上游有意改约和产品回归；保留旧断言及失败摘要供复核。只更新合成数据，不用生产库“让测试变绿”。
4. **验证范围**：本地绿灯不代表 Unix、桌面 UI/IPC、完整 MCP 审批或 GitHub 分支保护已验。远端签入检查、标签同步、自有发布和进入 B1 都是另行决策。
