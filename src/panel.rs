use super::{Panel, Settings, gradle, history};
use openidea_plugin::Error;
use std::fmt::Write;

const TRASH: &str = r#"<svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true"><path d="M2.8 4.6h10.4M6 4.6V2.8h4v1.8M4.2 4.6l.5 8.3h6.6l.5-8.3M6.8 6.8v3.9m2.4-3.9v3.9" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const FILE: &str = r#"<svg class="file-icon" viewBox="0 0 16 18" width="16" height="18" fill="none" aria-hidden="true"><rect x="2.5" y="1.5" width="11" height="15" rx="2.5" stroke="currentColor"/><path d="M5.5 7h5m-5 3h5" stroke="currentColor" stroke-linecap="round"/></svg>"#;
const DOWNLOAD: &str = r#"<svg class="download-icon" viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true"><path d="M8 2.5v9m-3.3-3.3L8 11.5l3.3-3.3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

fn section_header(id: &str, title: &str, action: &str) -> String {
    format!(
        r#"<div class="section-header">
  <h2 id="{id}-title"><button type="button" class="section-toggle" aria-expanded="true" aria-controls="{id}-content">{title}</button></h2>
  {action}
</div>"#
    )
}

pub fn contents(
    panel: &Panel,
    saved: &Settings,
    modules: &gradle::Modules,
    layouts: Result<Vec<history::Layout>, Error>,
    commands: Result<Vec<String>, Error>,
) -> String {
    let mut device_options = option("", "选择设备", saved.device.is_empty());
    let mut online = false;
    for device in &panel.devices {
        if device.online() {
            let selected = device.serial == saved.device;
            online |= selected;
            device_options.push_str(&option(&device.serial, &device.label(), selected));
        } else if device.serial != saved.device {
            let state = if device.state == "unauthorized" {
                "未授权"
            } else {
                &device.state
            };
            write!(
                device_options,
                "<option disabled>{} · {}</option>",
                escape(&device.label()),
                escape(state)
            )
            .expect("write to String");
        }
    }
    if !saved.device.is_empty() && !online {
        device_options.push_str(&option(&saved.device, "所选设备不可用", true));
    }

    let module = saved.module(modules);
    let mut module_options = option("", "选择应用模块", module.is_empty());
    for path in modules.keys() {
        module_options.push_str(&option(path, path, path == module));
    }

    let variants = modules
        .get(module)
        .and_then(|variants| variants.as_deref())
        .unwrap_or_default();
    let selected_variant = saved.variant(module, Some(variants));
    let variant_options = if variants.is_empty() {
        option("", "暂无可选构建变体", true)
    } else {
        variants
            .iter()
            .map(|name| option(name, name, name == selected_variant))
            .collect()
    };
    let variant_disabled = if variants.is_empty() { " disabled" } else { "" };
    let device_disabled = if online { "" } else { " disabled" };
    let online_hidden = if online { "" } else { " hidden" };
    let settings_header = section_header(
        "settings",
        "设备配置",
        r#"<button type="button" class="primary" data-action="refresh-config">应用</button>"#,
    );
    let capture_button = format!(
        r#"<button type="button" class="primary" data-action="inspect-layout"{device_disabled}>抓取</button>"#
    );
    let emulator_header = section_header(
        "emulator",
        "模拟器管理",
        r#"<button type="button" class="primary" id="emulator-new" disabled>新建</button>"#,
    );
    let layout_header = section_header("layout", "布局检查", &capture_button);
    let adb_header = section_header("adb", "ADB 命令", "");

    let mut html = String::new();
    write!(
        html,
        r#"<section aria-labelledby="settings-title">
{settings_header}
<div id="settings-content" class="section-content">
<form id="android-form">
<label for="device">设备</label>
<div class="select-wrap device-select">
  <select id="device" aria-describedby="device-state">{device_options}</select>
  <span id="device-state"{online_hidden}>已连接</span>
</div>
<div class="module-variant-row">
  <div>
    <label for="module">应用模块</label>
    <div class="select-wrap"><select id="module">{module_options}</select></div>
  </div>
  <div>
    <label for="variant">构建变体</label>
    <div class="select-wrap"><select id="variant"{variant_disabled}>{variant_options}</select></div>
  </div>
</div>
</form>
</div></section>"#,
    )
    .expect("write to String");

    let status = panel.status.as_deref().or(panel.device_error.as_deref());
    write!(
        html,
        "<p id=\"panel-status\" role=\"status\"{}>{}</p>",
        if status.is_none() { " hidden" } else { "" },
        escape(status.unwrap_or_default())
    )
    .expect("write to String");

    write!(html, r#"<section aria-labelledby="emulator-title">
{emulator_header}
<div id="emulator-content" class="section-content">
<div id="emulator-list"></div>
<p id="emulator-status" role="status" hidden></p>
</div>
</section>
<section aria-labelledby="layout-title">
{layout_header}
<div id="layout-content" class="section-content">
<h3 id="layout-history-title">历史布局</h3>
<div id="layout-history" class="history-scroll" role="region" aria-labelledby="layout-history-title" tabindex="0">{}</div>
</div>
</section>
<section aria-labelledby="adb-title">
{adb_header}
<div id="adb-content" class="section-content">
<div id="adb-entry">
  <label class="command-input"><span aria-hidden="true">adb</span><input id="adb-command" aria-label="ADB 命令" placeholder="shell wm size" autocomplete="off" spellcheck="false" maxlength="4096"></label>
  <button type="button" class="primary" id="adb-submit" data-action="execute-adb"{device_disabled}>执行</button>
</div>
<h3 id="adb-history-title">历史命令</h3>
<div id="adb-history" class="history-scroll" role="region" aria-labelledby="adb-history-title" tabindex="0">{}</div>
</div>
</section>"#, layout_rows(layouts), command_rows(commands, device_disabled)).expect("write to String");
    html
}

fn layout_rows(records: Result<Vec<history::Layout>, Error>) -> String {
    let records = match records {
        Ok(records) if !records.is_empty() => records,
        Ok(_) => return "<p>暂无布局记录</p>".into(),
        Err(error) => return format!("<p>{}</p>", escape(&error.to_string())),
    };
    let mut html = String::from(r#"<ul class="history">"#);
    for record in records {
        let id = record.id;
        let name = escape(&record.name);
        let filename = escape(&record.filename());
        write!(html, r#"<li>
  {FILE}
  <div class="record-label"><span title="{filename}">{name}.oialayout</span><time data-time="{id}"></time></div>
  <div class="record-actions">
    <button type="button" data-action="open-layout" data-value="{id}">打开布局</button>
    <button type="button" data-action="export-layout" data-value="{id}">{DOWNLOAD}导出</button>
    <button type="button" class="icon-button delete" data-action="delete-layout" data-value="{id}" aria-label="删除 {name} 布局记录" title="删除布局记录">{TRASH}</button>
  </div>
</li>"#).expect("write to String");
    }
    html.push_str("</ul>");
    html
}

fn command_rows(records: Result<Vec<String>, Error>, device_disabled: &str) -> String {
    let records = match records {
        Ok(records) if !records.is_empty() => records,
        Ok(_) => return "<p>暂无命令记录</p>".into(),
        Err(error) => return format!("<p>{}</p>", escape(&error.to_string())),
    };
    let mut html = String::from(r#"<ul class="history">"#);
    for command in records {
        let command = escape(&command);
        write!(html, r#"<li>
  <code class="record-label" title="adb {command}">adb {command}</code>
  <div class="record-actions">
    <button type="button" class="run-command" data-action="run-command" data-value="{command}"{device_disabled}>▶ 执行</button>
    <button type="button" class="icon-button delete" data-action="delete-command" data-value="{command}" aria-label="删除 adb {command} 命令记录" title="删除命令记录">{TRASH}</button>
  </div>
</li>"#).expect("write to String");
    }
    html.push_str("</ul>");
    html
}

fn option(value: &str, label: &str, selected: bool) -> String {
    format!(
        "<option value=\"{}\"{}>{}</option>",
        escape(value),
        if selected { " selected" } else { "" },
        escape(label),
    )
}

pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
