# Android Tools for OpenIDEA

在 OpenIDEA 里构建、安装和运行 Android 应用，也可以管理模拟器、检查 Debug 应用布局、执行 ADB 命令。

[下载 v1.1.0 签名包](https://github.com/fage-go/openidea-android-tools/releases/download/v1.1.0/fage.android-tools-1.1.0.oiplugin) · [版本记录](https://github.com/fage-go/openidea-android-tools/releases) · [市场登记进度](https://github.com/l0ng-ai/openidea-plugins/pull/350)

## 安装与首次运行

1. 建议使用 [OpenIDEA 0.1.51](https://github.com/l0ng-ai/openidea/releases/tag/v0.1.51)。打开 **Settings → Plugins → Install Plugin from Package File…**，并选择下载的 `.oiplugin`。安装或升级时，按提示确认权限。
2. 打开包含 Gradle Wrapper 和应用模块的 Android 工程。准备好项目所需的 JDK、Android SDK、Platform Tools（`adb`）及 Android CLI（`android`）。可从右侧工具窗口进入 **设置 → 配置Android环境** 查看检测结果。
3. 在右侧 **Android Tools → 设备配置** 选择设备、应用模块和构建变体，点击 **应用**。真机需要开启并授权 USB 调试；也可以选择已启动的模拟器。
4. 在顶栏选择 **Android :app**（或对应模块的运行配置），点击 **运行**。插件会构建、安装并启动应用。日志在 **Run** 控制台中。

想从顶栏快速切换设备，可右击工具栏打开 **Customize Main Toolbar…**，添加 **选择 Android 设备**。变体按模块保存；构建沿用项目的 Gradle Wrapper 和已配置的 Gradle JVM。

## 1.1.0 功能

### 模拟器管理

查看、启动、停止或新建 Android 虚拟设备。新建时可选设备类型、Android API、系统镜像与方向，也可以下载兼容镜像后创建。启动后可直接设为部署目标。删除已停止的模拟器时，可以同时删除未被其他模拟器使用的镜像。

### 布局检查

选择在线设备，在 Android 6.0（API 23）或以上打开可调试应用的目标页面，然后点击 **布局检查 → 抓取**。在同一视图查看截图、布局树和 View 属性。点击截图可定位节点；布局树可按类名筛选，也能切换精简显示。

快照保存在项目的 `.gradle/openidea-android-layouts`。可从历史重新打开、删除或导出为 `.oialayout`。快照包含设备截图及 View 数据，分享前请检查是否有敏感信息。若抓取失败，先检查插件的可选本地 Socket 权限，并断开已占用目标应用的调试器。

### ADB 命令

在当前设备上运行命令。在 **ADB 命令** 输入 `shell wm size` 等子命令，按 Enter 或点 **执行**。结果可以复制。每个项目保留最近 50 条不同的命令，可重跑或删除。

## 权限

| 权限 | 用途 |
| --- | --- |
| `project.read` | 识别 Gradle 工程，读取 JVM 配置、缓存和已保存的布局快照。 |
| `project.write` | 保存设备、模块、变体和历史，写入或导出快照，并刷新 IDE 运行配置。 |
| `process:adb` | 列出设备、运行用户输入的 ADB 命令；布局检查时获取截图并建立临时本地调试转发。 |
| `process:sh` | 执行项目的 Gradle Wrapper 和本机 Android 工具，用于构建、运行、管理模拟器及清理插件历史文件。 |
| `clipboard` | 用户点击复制时写入 ADB 执行结果或 Android CLI 安装命令。 |
| `ipc:$TMPDIR/openidea-android-layout-*`（可选） | 仅布局检查需要，用于连接 ADB 临时创建的本地 Socket。 |

插件会执行项目自己的 Gradle 脚本；请只打开并运行可信项目。

## 排查问题

- **找不到设备或设备未授权**：运行 `adb devices -l`，在真机确认 USB 调试授权，或确认模拟器启动完成，再点 **应用**。
- **找不到 ADB 或 Android CLI**：检查 IDE 进程的 `PATH` 和 Android SDK 路径，并在 **配置Android环境** 重新检测。
- **没找到模块或变体**：确认打开的是含 `gradlew` 的项目根目录，检查 JDK，并先运行 `./gradlew projects`。
- **构建成功却没装上**：先查 **Run** 控制台和设备连接。旧版 Gradle 路径要求本次构建产出一个可部署的 APK。

其他问题请到 [GitHub Issues](https://github.com/fage-go/openidea-android-tools/issues) 反馈，附上插件、IDE 和 Gradle/AGP 版本，以及脱敏后的错误日志。

## 从源码构建

使用 Rust stable、`wasm32-wasip2` 和 [OpenIDEA 插件 CLI](https://github.com/l0ng-ai/openidea/releases/tag/v0.1.51)。

```sh
rustup target add wasm32-wasip2
cargo test --locked
openidea-plugin build
openidea-plugin pack --out dist/fage.android-tools-1.1.0.oiplugin
openidea-plugin check dist/fage.android-tools-1.1.0.oiplugin
```

[GitHub Actions](https://github.com/fage-go/openidea-android-tools/actions/workflows/build.yml) 的构建产物未签名；安装请优先使用 GitHub Releases 的签名包。

## English quick start

Install the signed package in **Settings → Plugins → Install Plugin from Package File…**. Open a Gradle Android project, then use **Android Tools** to select a device, app module, and variant. Select the matching **Android :module** configuration in the toolbar and click **Run**. The tool window also supports emulator management, ADB commands, and layout snapshots for debuggable apps on Android 6.0+.

## License

[Apache License 2.0](LICENSE)
