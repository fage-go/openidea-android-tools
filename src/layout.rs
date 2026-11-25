use openidea_plugin::{
    Error, Invocation, ProcessOutput,
    host::{self, Project},
    text::escape_json,
};
use std::fmt::Write;

mod jdwp;
mod v2;

pub const VIEW: &str = "layout-inspector";

pub struct Snapshot {
    pub activity: String,
    window: String,
    screenshot: String,
    root: v2::Node,
}

pub fn document() -> String {
    format!(
        r#"<style>{}</style>
<main id="android-layout">
  <p id="layout-status" role="status">正在打开布局快照…</p>
  <div id="layout-columns" hidden>
    <section class="layout-tree-column" aria-labelledby="layout-tree-title">
      <div class="layout-tree-header">
        <h2 id="layout-tree-title">布局树</h2>
        <div class="layout-tree-controls">
          <input id="layout-tree-filter" type="search" placeholder="筛选类名" aria-label="筛选完整类名" autocomplete="off" spellcheck="false">
          <label><input id="layout-tree-compact" type="checkbox"><span class="compact-check" aria-hidden="true"></span>精简</label>
        </div>
      </div>
      <div id="layout-tree" tabindex="0" role="region" aria-labelledby="layout-tree-title">
        <ul id="layout-tree-rows"></ul>
        <p id="layout-tree-empty" role="status" hidden>没有匹配的类</p>
      </div>
    </section>
    <div class="layout-detail-column">
      <section class="layout-image-pane" aria-labelledby="layout-window">
        <h2 id="layout-window"></h2>
        <div id="layout-viewport">
          <div id="layout-screen"><img alt="设备截图"><span id="layout-highlight" hidden></span></div>
        </div>
      </section>
      <section class="layout-property-pane" aria-labelledby="layout-properties-title">
        <div class="layout-property-header">
          <h2 id="layout-properties-title">View 属性</h2>
          <input id="layout-property-filter" type="search" placeholder="筛选属性" aria-label="筛选属性" autocomplete="off" spellcheck="false">
        </div>
        <div id="layout-properties"></div>
      </section>
    </div>
  </div>
</main><script>{}</script>"#,
        include_str!("layout/view.css"),
        include_str!("layout/view.js"),
    )
}

pub fn send_snapshot(ctx: &Invocation, request: &str) -> Result<(), Error> {
    let Some((id, _)) = crate::view_request(request) else {
        return Ok(());
    };
    let prefix = format!("layout\n{id}\n");
    let read = || {
        let project = ctx.project().ok_or("请先打开布局文件所在的项目")?;
        let editor = ctx.editor().ok_or("请选择布局文件")?;
        project.read_file(&editor.path())
    };
    let payload = match read() {
        Ok(payload) => payload,
        Err(error) => return host::post_view_message(VIEW, &format!("{prefix}error\n{error}")),
    };

    // A snapshot can be bigger than the IDE's 4 MiB HTML body limit.
    let mut rest = payload.as_str();
    while !rest.is_empty() {
        let mut end = rest.len().min(1 << 20);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (chunk, remaining) = rest.split_at(end);
        host::post_view_message(VIEW, &format!("{prefix}chunk\n{chunk}"))?;
        rest = remaining;
    }
    host::post_view_message(VIEW, &format!("{prefix}done\n"))
}

pub fn capture(project: &Project, serial: &str) -> Result<Snapshot, Error> {
    if serial.is_empty() {
        return Err("请先选择在线 Android 设备".into());
    }
    let api = adb(
        project,
        serial,
        &["shell", "getprop", "ro.build.version.sdk"],
    )?;
    if api.trim().parse::<u32>().unwrap_or_default() < 23 {
        return Err("V2 布局检查要求 Android 6.0（API 23）或以上".into());
    }
    let activity = foreground(project, serial)?;
    let pid = activity_pid(project, serial, &activity)?;
    let (_, class) = activity.split_once('/').ok_or("无法识别应用窗口")?;
    let (window, data) = {
        let forward = Forward::open(project, serial, pid)?;
        let connection = host::ipc_connect(&forward.ipc_name).map_err(|error| {
            format!("无法打开布局检查连接，请检查插件的本地 Socket 权限：{error}")
        })?;
        jdwp::capture(&connection, class)?
    };
    let root = v2::decode(&data)?;
    let png = adb(project, serial, &["exec-out", "screencap -p | base64"])?;
    let screenshot: String = png.split_ascii_whitespace().collect();
    if !screenshot.starts_with("iVBORw0KGgo")
        || !screenshot
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    {
        return Err("设备未返回有效的 PNG 截图".into());
    }
    if foreground(project, serial)? != activity {
        return Err("采集期间前台应用发生变化，请重新检查布局".into());
    }
    Ok(Snapshot {
        activity: class.rsplit('.').next().unwrap_or(class).to_owned(),
        window,
        screenshot,
        root,
    })
}

fn foreground(project: &Project, serial: &str) -> Result<String, Error> {
    let activities = adb(
        project,
        serial,
        &["shell", "dumpsys", "activity", "activities"],
    )?;
    foreground_component(&activities)
        .map(str::to_owned)
        .ok_or_else(|| "未找到设备的前台应用".into())
}

fn foreground_component(activities: &str) -> Option<&str> {
    ["topResumedActivity", "mResumedActivity", "ResumedActivity"]
        .into_iter()
        .find_map(|key| {
            activities.lines().find_map(|line| {
                let tail = line.trim_start().strip_prefix(key)?;
                let tail = tail.trim_start().strip_prefix([':', '='])?;
                tail.split_whitespace().find_map(|word| {
                    let component = word.trim_end_matches('}');
                    let (package, class) = component.split_once('/')?;
                    (!package.is_empty()
                        && !class.is_empty()
                        && package
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_')))
                    .then_some(component)
                })
            })
        })
}

fn activity_pid(project: &Project, serial: &str, component: &str) -> Result<u32, Error> {
    let top = adb(project, serial, &["shell", "dumpsys", "activity", "top"])?;
    if let Some(pid) = top.lines().rev().find_map(|line| {
        let mut fields = line
            .trim_start()
            .strip_prefix("ACTIVITY ")?
            .split_whitespace();
        if fields.next()? != component {
            return None;
        }
        fields.find_map(|field| {
            field
                .strip_prefix("pid=")?
                .parse::<u32>()
                .ok()
                .filter(|&pid| pid != 0)
        })
    }) {
        return Ok(pid);
    }
    let (package, _) = component.split_once('/').ok_or("无法识别前台应用包名")?;
    let pids = adb(project, serial, &["shell", "pidof", package])?;
    pids.trim()
        .parse::<u32>()
        .ok()
        .filter(|&pid| pid != 0)
        .ok_or_else(|| "无法确定前台 Activity 的进程，请打开目标应用后重试".into())
}

fn adb(project: &Project, serial: &str, args: &[&str]) -> Result<String, Error> {
    let args: Vec<_> = ["-s", serial]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    checked(project.run_process("adb", &args, None)?)
}

fn shell(project: &Project, script: &str, args: &[&str]) -> Result<String, Error> {
    let args: Vec<_> = ["-c", script, "openidea-android-layout"]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    checked(project.run_process("sh", &args, None)?)
}

fn checked(output: ProcessOutput) -> Result<String, Error> {
    if output.status == Some(0) {
        return Ok(output.stdout);
    }
    let detail = output
        .stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("无进程输出");
    Err(format!("布局检查命令失败：{detail}").into())
}

struct Forward<'a> {
    project: &'a Project,
    serial: &'a str,
    path: String,
    ipc_name: String,
    endpoint: String,
    bound: bool,
}

fn socket_paths(mktemp_output: &str) -> Result<(String, String), Error> {
    let (directory, name) = mktemp_output
        .trim_end()
        .rsplit_once('/')
        .filter(|(directory, name)| {
            directory.starts_with('/') && name.starts_with("openidea-android-layout-")
        })
        .ok_or("无法确定布局检查的临时 Socket 路径")?;
    // Keep the authorization request relative to the same host prefix as the manifest.
    let path = format!("{}/{name}", directory.trim_end_matches('/'));
    Ok((path, format!("$TMPDIR/{name}")))
}

impl<'a> Forward<'a> {
    fn open(project: &'a Project, serial: &'a str, pid: u32) -> Result<Self, Error> {
        let path = shell(
            project,
            r#"mktemp "${TMPDIR:?需要本机临时目录}/openidea-android-layout-XXXXXXXXXX""#,
            &[],
        )?;
        let (path, ipc_name) = socket_paths(&path)?;
        let endpoint = format!("localfilesystem:{path}");
        let mut forward = Self {
            project,
            serial,
            path,
            ipc_name,
            endpoint,
            bound: false,
        };
        adb(
            project,
            serial,
            &[
                "forward",
                "--no-rebind",
                &forward.endpoint,
                &format!("jdwp:{pid}"),
            ],
        )
        .map_err(|error| format!("无法连接前台应用的调试进程，请运行 Debug 版应用：{error}"))?;
        forward.bound = true;
        Ok(forward)
    }
}

impl Drop for Forward<'_> {
    fn drop(&mut self) {
        if self.bound {
            let _ = adb(
                self.project,
                self.serial,
                &["forward", "--remove", &self.endpoint],
            );
        }
        let _ = shell(
            self.project,
            r#"if [ -e "$1" ] || [ -S "$1" ]; then unlink "$1"; fi"#,
            &[&self.path],
        );
    }
}

impl Snapshot {
    pub fn write_json(&self, out: &mut String) {
        out.push_str("{\"window\":");
        string(out, &self.window);
        out.push_str(",\"png\":");
        string(out, &self.screenshot);
        out.push_str(",\"tree\":");
        write_node(out, &self.root);
        out.push('}');
    }
}

fn string(out: &mut String, value: &str) {
    out.push('"');
    out.push_str(&escape_json(value));
    out.push('"');
}

fn write_node(out: &mut String, node: &v2::Node) {
    out.push_str("{\"class\":");
    string(out, &node.class);
    out.push_str(",\"bounds\":");
    if let Some([left, top, right, bottom]) = node.bounds {
        write!(out, "[{left:.2},{top:.2},{right:.2},{bottom:.2}]").expect("write to String");
    } else {
        out.push_str("null");
    }
    out.push_str(",\"properties\":[");
    for (index, (name, value)) in node.properties.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push('[');
        string(out, name);
        out.push(',');
        string(out, value);
        out.push(']');
    }
    out.push_str("],\"children\":[");
    for (index, child) in node.children.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        write_node(out, child);
    }
    out.push_str("]}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_snapshot_chunks_without_splitting_utf8_or_injecting_html() {
        let mock = openidea_plugin::mock::MockHost::install().with_capabilities(&["project.read"]);
        let payload = "a".repeat((1 << 20) - 1) + "界</script>";
        mock.open_editor("/project/example.oialayout", "text", &payload, &[]);
        send_snapshot(&mock.invocation(), "editor-request\n").unwrap();
        let messages = mock.view_messages();
        let chunks: String = messages
            .iter()
            .filter_map(|(_, message)| message.strip_prefix("layout\neditor-request\nchunk\n"))
            .collect();
        assert_eq!(chunks, payload);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2].1, "layout\neditor-request\ndone\n");
        assert!(!document().contains(&payload));
    }

    #[test]
    fn uses_the_manifest_prefix_for_sockets_created_under_a_slash_terminated_tmpdir() {
        let socket = "openidea-android-layout-3H6SesGQ2q";
        let (path, ipc_name) =
            socket_paths(&format!("/var/folders/example/T//{socket}\n")).unwrap();
        assert_eq!(path, format!("/var/folders/example/T/{socket}"));
        assert_eq!(ipc_name, format!("$TMPDIR/{socket}"));
    }

    #[test]
    fn finds_the_resumed_component_on_legacy_and_modern_android() {
        assert_eq!(
            foreground_component(
                "    mResumedActivity: ActivityRecord{b1 u0 com.example/.MainActivity t24}"
            ),
            Some("com.example/.MainActivity")
        );
        assert_eq!(
            foreground_component(
                "topResumedActivity=ActivityRecord{b2 u0 com.example/com.example.Main t2}"
            ),
            Some("com.example/com.example.Main")
        );
    }
}
