use openidea_plugin::{
    Error,
    host::{self, Project},
};
use std::fmt::Write;

pub const DOWNLOAD: &str =
    "https://developer.android.com/tools/agents/android-cli/download?hl=zh-cn";
const DOWNLOAD_ROOT: &str = "https://dl.google.com/android/cli/latest";
const PLATFORMS: [(&str, &str); 5] = [
    ("darwin_arm64", "macOS · Apple 芯片"),
    ("darwin_x86_64", "macOS · Intel 芯片"),
    ("linux_x86_64", "Linux · x86-64"),
    ("linux_arm64", "Linux · ARM64"),
    ("windows_x86_64", "Windows · 命令提示符"),
];
const PROBE: &str = concat!(
    include_str!("android-cli.sh"),
    r#"
if android_cli_path >/dev/null; then
    printf 'ready\n'
else
    printf 'missing\n'
fi
system=$(uname -s 2>/dev/null || :)
arch=$(uname -m 2>/dev/null || :)
if [ "$system" = Darwin ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || :)" = 1 ]; then
    arch=arm64
elif [ "${OS:-}" = Windows_NT ]; then
    system=Windows
    arch=${PROCESSOR_ARCHITEW6432:-${PROCESSOR_ARCHITECTURE:-$arch}}
fi
printf '%s\n%s\n' "$system" "$arch"
"#
);

#[derive(Debug, PartialEq, Eq)]
pub struct Environment {
    pub ready: bool,
    platform: Option<&'static str>,
}

fn platform(system: &str, arch: &str) -> Option<&'static str> {
    match (system, arch) {
        ("Darwin", "arm64" | "aarch64") => Some("darwin_arm64"),
        ("Darwin", "x86_64") => Some("darwin_x86_64"),
        ("Linux", "x86_64") => Some("linux_x86_64"),
        ("Linux", "aarch64" | "arm64") => Some("linux_arm64"),
        ("Windows", "AMD64" | "x86_64") => Some("windows_x86_64"),
        _ => None,
    }
}

pub fn probe(project: Option<&Project>) -> Result<Environment, Error> {
    let args = [
        "-c".into(),
        PROBE.into(),
        "openidea-android-environment".into(),
    ];
    let output = match project {
        Some(project) => project.run_process("sh", &args, None),
        None => host::run_process("sh", &args, None),
    }?;
    if output.status != Some(0) {
        return Err(format!("检测 Android CLI 失败：{}", output.stderr.trim()).into());
    }
    let mut lines = output.stdout.lines();
    let ready = match lines.next() {
        Some("ready") => true,
        Some("missing") => false,
        _ => return Err("未取得 Android 环境检测结果".into()),
    };
    let platform = platform(
        lines.next().unwrap_or_default(),
        lines.next().unwrap_or_default(),
    );
    Ok(Environment { ready, platform })
}

pub fn install_command(platform: &str) -> Result<String, Error> {
    if !PLATFORMS.iter().any(|(id, _)| *id == platform) {
        return Err("请选择支持的操作系统".into());
    }
    if platform == "windows_x86_64" {
        Ok(format!(
            r#"curl.exe -fsSL {DOWNLOAD_ROOT}/{platform}/install.cmd -o "%TEMP%\i.cmd" && "%TEMP%\i.cmd""#
        ))
    } else {
        Ok(format!(
            "curl -fsSL {DOWNLOAD_ROOT}/{platform}/install.sh | bash"
        ))
    }
}

pub fn contents(environment: Result<Environment, Error>, configuration: bool) -> String {
    let (status, details, selected, ready) = match environment {
        Ok(env) if env.ready => ("Android 环境已就绪", String::new(), env.platform, true),
        Ok(env) => (
            "未检测到 Android 环境",
            "选择适合当前系统的安装方式，安装后回来重新检测。".into(),
            env.platform,
            false,
        ),
        Err(error) => (
            "无法检测 Android 环境",
            super::panel::escape(&error.to_string()),
            None,
            false,
        ),
    };
    let (dismiss, back, recheck) = if configuration {
        ("close-environment", "关闭", "configure-android-environment")
    } else {
        (
            "enter-android-tools",
            "稍后安装",
            "check-android-environment",
        )
    };
    let details = if details.is_empty() {
        details
    } else {
        format!(r#"<p class="environment-details">{details}</p>"#)
    };
    let ready_class = if ready { " ready" } else { "" };
    let mut options = String::from(r#"<option value="">选择操作系统</option>"#);
    for (id, name) in PLATFORMS {
        let command = super::panel::escape(&install_command(id).expect("known platform"));
        let state = if selected == Some(id) {
            " selected"
        } else {
            ""
        };
        write!(
            options,
            r#"<option value="{id}" data-command="{command}"{state}>{name}</option>"#
        )
        .expect("write to String");
    }
    format!(
        r#"
<header class="result-header">
  <h2 id="android-environment-title">配置Android环境</h2>
  <button type="button" class="icon-button close" data-environment-close aria-label="关闭" title="关闭">×</button>
</header>
<div class="environment-body">
<p class="environment-heading{ready_class}">{status}</p>
{details}
<div class="environment-install">
  <div class="environment-install-header">
    <h3>安装 Android CLI</h3>
    <button type="button" class="primary" data-action="open-cli-download">打开官方下载页</button>
  </div>
  <div class="environment-platform">
    <label for="android-install-platform">安装命令</label>
    <select id="android-install-platform">{options}</select>
    <button type="button" data-action="copy-install-command" disabled>复制命令</button>
  </div>
  <pre id="android-install-command" hidden></pre>
</div>
<div class="environment-actions">
  <button type="button" id="android-environment-dismiss" data-action="{dismiss}">{back}</button>
  <button type="button" data-action="{recheck}">重新检测</button>
</div>
<p id="android-environment-status" role="status" hidden></p>
</div>"#
    )
}
