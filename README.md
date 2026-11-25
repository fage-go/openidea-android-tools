# Android Tools for OpenIDEA

在 OpenIDEA 里选择 Android 设备、应用模块和构建变体，然后从顶栏构建、安装并启动 App。不用为每个模块手动新建一份运行配置。

[下载签名安装包 v1.0.0](https://github.com/fage-go/openidea-android-tools/releases/download/v1.0.0/fage.android-tools-1.0.0.oiplugin) · [所有版本](https://github.com/fage-go/openidea-android-tools/releases) · [社区上架进度](https://github.com/l0ng-ai/openidea-plugins/pull/350)

目前可从 GitHub 下载后手动安装。OpenIDEA 社区市场的首次上架申请已提交，市场能否搜索安装请以上架进度为准。

## 安装前

- **OpenIDEA**：建议先用 [0.1.51](https://github.com/l0ng-ai/openidea/releases/tag/v0.1.51)；插件声明兼容的插件 API 为 `^1.13`。IDE 版本和插件 API 版本不是同一个编号。
- **Android 项目**：使用 Gradle Wrapper（`gradlew`），并有至少一个应用模块，例如 `:app`。
- **本机工具**：安装项目所需的 JDK、Android SDK 和 Platform Tools。安装或运行 App 还需要 Android CLI（命令 `android`）。
- **部署目标**：启动模拟器，或连接已开启 USB 调试且已授权的真机。用 `adb devices -l` 确认目标状态是 `device`。

## 安装和首次运行

1. 下载 `fage.android-tools-1.0.0.oiplugin`。在 OpenIDEA 打开 **Settings → Plugins → Install Plugin from Package File…**，选择安装包，核对权限后安装。
2. 打开 Android Gradle 项目，在右侧打开 **Android** 工具窗口，点 **刷新配置**，依次选择 **设备 → 应用模块 → 构建变体**，例如 `Pixel 模拟器 → :app → debug`。
3. 在顶栏的运行配置下拉框选择 **Android :app** 并点击 **运行**。构建、安装和启动的输出可以在 **Run** 控制台查看。

想在顶栏快速切换设备？右击工具栏，打开 **Customize Main Toolbar…**，添加 **选择 Android 设备**。不添加这个按钮，也可以在右侧工具窗口选设备。

## 它怎么处理构建

- **变体按模块保存**：下拉框显示当前模块可用的 Gradle 变体。第一次选择模块时优先用 `debug`，其次是带渠道的 Debug 变体，例如 `demoDebug`。配置会同步到顶栏。
- **沿用项目的 JVM 设置**：模块识别和 Gradle 命令使用项目的 Wrapper。若在 `.openidea/settings.toml` 配了 Gradle JVM，就优先使用；没配置则用当前环境。
- **兼容旧版 Gradle 的部署路径**：插件检测到旧版 Gradle 时，先调用变体对应的任务（如 `:app:assembleDebug`），再部署本次构建记录的单个 APK，不从输出目录随便选一个包。
- **配置保存在项目本地**：设备、模块和变体选择写到 `.gradle/openidea-android-tools.conf`；换设备或换电脑后重新选一次即可。

## 常见问题

| 现象 | 先检查什么 |
| --- | --- |
| 设备没有出现或显示未授权 | 跑 `adb devices -l`。真机解锁并确认 USB 调试授权，模拟器等到启动完成，再点 **刷新配置**。 |
| 找不到 ADB | 确认 OpenIDEA 能找到 Android SDK 的 `platform-tools/adb`。把 `platform-tools` 加入 IDE 进程的 `PATH`，或确保 `ANDROID_HOME` 指向正确的 SDK。 |
| 找不到 `android` 命令 | 安装 Android CLI，确保 OpenIDEA 能在 `PATH` 找到 `android`，或放到 `~/.local/bin/android`。仅有 `adb` 还不能完成插件的运行流程。 |
| 没有应用模块、变体或运行配置 | 确认打开的是含 `gradlew` 和 Gradle 配置的项目根目录，检查项目 JDK，先保证 `./gradlew projects` 能运行，再回到工具窗口刷新。 |
| 构建成功但装不上或打不开 | 查看 **Run** 控制台第一条报错，并确认所选设备仍在线。旧版 Gradle 路径需要当前变体能产出单个可部署的 APK。 |

如果仍有问题，请到 [GitHub Issues](https://github.com/fage-go/openidea-android-tools/issues) 附上 OpenIDEA、插件、Gradle/AGP 的版本，以及脱敏后的报错。

## 插件权限

| 权限 | 用途 |
| --- | --- |
| `project.read` | 识别 Gradle 项目，读取项目的 Gradle JVM 设置和模块、变体缓存。 |
| `project.write` | 保存项目本地的设备、模块、变体选择，写入供运行配置刷新使用的项目文件。 |
| `process:adb` | 执行 `adb devices -l`，列出 Android 设备及授权状态。 |
| `process:sh` | 调用项目的 Gradle Wrapper、ADB 和 Android CLI，完成模块识别与构建、安装、启动。 |

`process:sh` 会执行项目自己的 Gradle 脚本。请只在可信项目中授予权限。

## 从源码构建

使用 Rust stable、`wasm32-wasip2` 和 [OpenIDEA v0.1.51 的插件 CLI](https://github.com/l0ng-ai/openidea/releases/tag/v0.1.51)：

```sh
rustup target add wasm32-wasip2
cargo test --locked
openidea-plugin build
openidea-plugin pack --out dist/fage.android-tools-1.0.0.oiplugin
openidea-plugin check dist/fage.android-tools-1.0.0.oiplugin
```

[GitHub Actions](https://github.com/fage-go/openidea-android-tools/actions/workflows/build.yml) 在推送到 `main`、提交 PR 或手动触发时构建并校验安装包。**Actions 产物未签名**，日常安装优先用 GitHub Releases 的签名包。

## English quick start

Android Tools adds an Android tool window and per-module run configurations to OpenIDEA. [Download the signed package](https://github.com/fage-go/openidea-android-tools/releases/tag/v1.0.0), install it from **Settings → Plugins → Install Plugin from Package File…**, then open a Gradle Android project. In the **Android** tool window, choose a device, application module and build variant. Select **Android :app** (or your module) in the main toolbar and click **Run**. Deployment needs the Android SDK, ADB and the Android CLI. [Track the community listing here](https://github.com/l0ng-ai/openidea-plugins/pull/350).

## 许可证

[Apache License 2.0](LICENSE)
