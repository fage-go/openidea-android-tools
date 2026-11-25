# Android Tools for OpenIDEA

在 **Android Tools** 工具窗口配置设备、应用模块和构建变体，从 OpenIDEA 顶栏运行应用。

[已签名历史版本 v1.0.0](https://github.com/fage-go/openidea-android-tools/releases/tag/v1.0.0) · [所有正式版本](https://github.com/fage-go/openidea-android-tools/releases) · [社区上架进度](https://github.com/l0ng-ai/openidea-plugins/pull/350)

## 安装

1. 在官方 OpenIDEA 0.1.51 的 **Settings → Plugins** 中选择 **Install Plugin from Package File…**，打开 `fage.android-tools-1.1.0.oiplugin`。
2. 确认插件列出的项目读写、ADB、Shell 和剪贴板写入权限。若插件找不到 ADB，在插件设置中给它选择 Android SDK 下的 `platform-tools/adb`。
3. 右击主工具栏，打开 **Customize Main Toolbar…**。在右侧工具栏添加 **选择 Android 设备**，将它放在 **Build Project** 锤子前面。

## 使用

打开包含 Gradle Wrapper 和至少一个应用模块的 Android 工程。真机需开启并授权 USB 调试；也可使用 Android 模拟器。

- 点击 Android 图标时会刷新 ADB 列表。选择会同时用于 Android 工具窗口和顶栏运行配置。
- 打开 Android Tools 时，检测到 Android CLI 会直接进入工具。找不到 Android CLI 时会在工具页面上弹出配置，可打开官方下载页、复制对应系统的安装命令，或选择稍后安装。点击标题栏的 **设置 → 配置Android环境** 可打开面板内的配置弹窗，查看环境状态、重新检测或前往官方下载。
- 打开 Android 工具窗口或点击设备配置标题右侧的 **应用** 时，会更新设备、应用模块和构建变体。点击各区域标题可展开或收起。
- 构建变体下拉框显示当前应用模块在 Gradle 中启用的变体，并记住各模块的选择。首次选择模块时优先选择 `debug`，其次是带渠道的 Debug 变体。
- 配置会自动同步到顶栏运行下拉框。选择 `Android :app` 等配置运行时，会使用对应模块保存的构建变体。
- 模块识别和编译使用项目的 Gradle Wrapper 以及项目 `.openidea/settings.toml` 中配置的 Gradle JVM。项目未指定 JVM 时，使用当前 Gradle 环境。
- 点击顶栏 **运行** 会构建 APK、安装到选中的在线设备并启动应用。旧版 Gradle 使用变体对应的标准任务（如 `:app:assembleDebug`），并部署该次构建记录的单个 APK。运行需要本机 Android CLI 和 Android SDK，构建输出可在 Run 控制台查看。
- 在 **模拟器管理** 中可查看当前 SDK 的虚拟设备、启动或停止模拟器，并将已启动的模拟器设为当前部署设备。模拟器在独立窗口运行。
- 已停止的模拟器可通过「启动」旁的删除图标移除。在确认弹窗中可以勾选同时删除 System Image。共用镜像时，删除仅移除所选模拟器。
- 点击 **新建**，依次选择设备类型和硬件配置、Android API 与系统镜像，再确认名称和启动方向。可以选用已安装的镜像，或点击 **下载并创建** 安装兼容的镜像。创建需要 Android SDK 的 Emulator 和 Command-line Tools。下载使用 Android CLI。
- 选择设备并打开 Debug 版应用的目标页面，点击布局检查标题右侧的 **抓取**。快照会写入项目的 `.gradle/openidea-android-layouts` 并打开检查器。左侧布局树可横向滚动查看完整类名，右侧上方为截图，下方为可筛选、可折叠分组的 View 属性。历史记录可以重新打开、导出为项目内的 `.oialayout` 文件或删除。右击检查器标签，选择 **Open in New Window** 可以把预览移到独立窗口。布局检查使用 Android 6.0（API 23）及以上的 V2 协议。
- 布局树标题旁可以按完整类名筛选，并保留匹配项的父级路径。勾选 **精简** 显示短类名，节点 ID 以 `@` 开头。
- 布局树和右侧检查区等宽。点击截图会选中对应 View，滚动到树中的节点并更新属性。筛选隐藏节点时会临时显示该路径，编辑筛选词后恢复按类名筛选。
- 在 **ADB 命令** 输入框中输入 `shell wm size` 等子命令，点击 **执行** 或按 Enter。结果弹窗显示状态和可滚动的输出，右上角的复制图标会复制输出。项目保留最近 50 条不同的命令，每条可以在当前设备上重新执行或删除。

## 常见问题

| 现象 | 先检查什么 |
| --- | --- |
| 设备没有出现或显示未授权 | 跑 `adb devices -l`。真机解锁并确认 USB 调试授权，模拟器等到启动完成，再点 **应用**。 |
| 找不到 ADB | 确认 OpenIDEA 能找到 Android SDK 的 `platform-tools/adb`。把 `platform-tools` 加入 IDE 进程的 `PATH`，或确保 `ANDROID_HOME` 指向正确的 SDK。 |
| 找不到 `android` 命令 | 安装 Android CLI，确保 OpenIDEA 能在 `PATH` 找到 `android`，或放到 `~/.local/bin/android`。仅有 `adb` 还不能完成插件的运行流程。 |
| 没有应用模块、变体或运行配置 | 确认打开的是含 `gradlew` 和 Gradle 配置的项目根目录，检查项目 JDK，先保证 `./gradlew projects` 能运行，再回到工具窗口应用。 |
| 构建成功但装不上或打不开 | 查看 **Run** 控制台第一条报错，并确认所选设备仍在线。旧版 Gradle 路径需要当前变体能产出单个可部署的 APK。 |

如果仍有问题，请到 [GitHub Issues](https://github.com/fage-go/openidea-android-tools/issues) 附上 OpenIDEA、插件、Gradle/AGP 的版本，以及脱敏后的报错。

## 权限

- `project.read` 用于识别 Gradle 项目、读取 Gradle JVM 设置和已缓存的模块与变体。
- `project.write` 用于保存设备、模块、变体选择和历史，导出布局，并通知 OpenIDEA 刷新运行配置。
- `process:adb` 用于列出设备、执行侧边窗命令，并在布局检查时采集截图、查询前台应用和建立临时调试转发。
- `process:sh` 用于调用项目的 Gradle Wrapper、构建和运行 Android 应用，以及删除插件自身的历史布局文件。
- `clipboard` 在点击 ADB 结果弹窗的复制图标时写入系统剪贴板。
- 布局检查使用可选的 `ipc:$TMPDIR/openidea-android-layout-*` 权限连接 ADB 临时创建的本地 Socket。请在插件权限中授权该项。已有调试器连接到目标应用时，请先结束该调试会话。

只在可信项目中授予 Shell 等权限。插件会运行项目自己的 Gradle 脚本。

## 构建

使用 Rust stable、`wasm32-wasip2` 目标和 OpenIDEA `v0.1.51` 对应的 `openidea-plugin` CLI。

```sh
rustup target add wasm32-wasip2
cargo test --locked
openidea-plugin build
openidea-plugin pack --out dist/fage.android-tools-1.1.0.oiplugin
openidea-plugin check dist/fage.android-tools-1.1.0.oiplugin
```

[GitHub Actions](https://github.com/fage-go/openidea-android-tools/actions/workflows/build.yml) 在推送到 `main`、提交 PR 或手动触发时运行测试并构建插件。可以从工作流运行页的 **Artifacts** 下载未签名的构建产物。提交社区市场前，使用发布者 `fage` 的私钥为安装包签名。

## English quick start

Install the package in OpenIDEA from **Settings → Plugins → Install Plugin from Package File…**. Open a Gradle Android project and use **Android Tools** to select a target device, module and build variant. The panel also provides emulator management, ADB commands and layout snapshots for debuggable apps.

## 许可证

[Apache License 2.0](LICENSE)
