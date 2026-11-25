use super::{Panel, Settings, gradle};
use std::fmt::Write;

pub fn document(project: &str) -> String {
    format!(
        r#"<style>{}</style><main id="android-panel" data-project="{project}"><p role="status">正在刷新配置…</p></main><script>{}</script>"#,
        include_str!("panel.css"),
        include_str!("panel.js")
    )
}

pub fn contents(panel: &Panel, saved: &Settings, modules: &gradle::Modules) -> String {
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

    let mut html = String::from(r#"<form id="android-form">"#);
    write!(
        html,
        r#"<label for="device">设备</label>
<select id="device">{device_options}</select>
<div class="module-variant-row">
  <div>
    <label for="module">应用模块</label>
    <select id="module">{module_options}</select>
  </div>
  <div>
    <label for="variant">构建变体</label>
    <select id="variant"{variant_disabled}>{variant_options}</select>
  </div>
</div>
<button type="button" class="refresh-config" data-action="refresh-config">刷新配置</button>"#,
    )
    .expect("write to String");

    html.push_str("</form>");
    let status = panel.status.as_deref().or(panel.device_error.as_deref());
    write!(
        html,
        "<p role=\"status\"{}>{}</p>",
        if status.is_none() { " hidden" } else { "" },
        escape(status.unwrap_or_default())
    )
    .expect("write to String");
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

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
