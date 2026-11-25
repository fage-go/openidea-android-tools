# Android Tools for OpenIDEA

在 **Android** 工具窗口配置设备、应用模块和构建变体，从 OpenIDEA 顶栏运行应用。

## 安装

1. 在官方 OpenIDEA 0.1.51 的 **Settings → Plugins** 中选择 **Install Plugin from Package File…**，打开 `fage.android-tools-1.0.0.oiplugin`。
2. 确认插件列出的项目读写、ADB 和 Shell 权限。若插件找不到 ADB，在插件设置中给它选择 Android SDK 下的 `platform-tools/adb`。
3. 右击主工具栏，打开 **Customize Main Toolbar…**。在右侧工具栏添加 **选择 Android 设备**，将它放在 **Build Project** 锤子前面。

## 使用

- 点击 Android 图标时会刷新 ADB 列表。选择会同时用于 Android 工具窗口和顶栏运行配置。
- 打开 Android 工具窗口或点击底部 **刷新配置** 时，会更新设备、应用模块和构建变体。
- 构建变体下拉框显示当前应用模块在 Gradle 中启用的变体，并记住各模块的选择。首次选择模块时优先选择 `debug`，其次是带渠道的 Debug 变体。
- 配置会自动同步到顶栏运行下拉框。选择 `Android :app` 等配置运行时，会使用对应模块保存的构建变体。
- 模块识别和编译使用项目的 Gradle Wrapper 以及项目 `.openidea/settings.toml` 中配置的 Gradle JVM。项目未指定 JVM 时，使用当前 Gradle 环境。
- 点击顶栏 **运行** 会构建 APK、安装到选中的在线设备并启动应用。旧版 Gradle 使用变体对应的标准任务（如 `:app:assembleDebug`），并部署该次构建记录的单个 APK。运行需要本机 Android CLI 和 Android SDK，构建输出可在 Run 控制台查看。

## 权限

- `project.read` 用于识别 Gradle 项目、读取 Gradle JVM 设置和已缓存的模块与变体。
- `project.write` 用于保存设备、模块和变体选择，并通知 OpenIDEA 刷新运行配置。
- `process:adb` 用于列出已连接的 Android 设备及其授权状态。
- `process:sh` 用于使用项目的 Gradle Wrapper 识别模块与变体，并调用 Gradle、ADB 和 Android CLI 完成构建、安装和启动。

## 构建

使用 Rust stable、`wasm32-wasip2` 目标和 OpenIDEA `v0.1.51` 对应的 `openidea-plugin` CLI。

```sh
rustup target add wasm32-wasip2
cargo test --locked
openidea-plugin build
openidea-plugin pack --out dist/fage.android-tools-1.0.0.oiplugin
openidea-plugin check dist/fage.android-tools-1.0.0.oiplugin
```

GitHub Actions 在推送到 `main`、提交 PR 或手动触发时运行测试并构建插件。可以从工作流运行页的 **Artifacts** 下载安装包。提交社区市场前，使用发布者 `fage` 的私钥为安装包签名。
