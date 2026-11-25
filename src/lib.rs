use openidea_plugin::execution::{PluginRunConfiguration, RunConfigurations};
use openidea_plugin::host::{self, Project};
use openidea_plugin::views::{Body, View, ViewEvent, Viewport, Views};
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Plugin, register_plugin};
use std::collections::BTreeMap;

mod adb;
mod emulator;
mod gradle;
mod history;
mod layout;
mod panel;
mod setup;

const VIEW: &str = "android";
const SETTINGS: &str = ".gradle/openidea-android-tools.conf";
const RUN_INDEX: &str = ".openidea/index-android-tools.json";

fn view_request(payload: &str) -> Option<(&str, &str)> {
    let (id, text) = payload.split_once('\n')?;
    (!id.is_empty() && id.len() <= 60 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        .then_some((id, text))
}

fn configuration_id(module: &str) -> String {
    if module.len() > 28 {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in module.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        return format!("module-h{hash:016x}");
    }
    use std::fmt::Write;
    let mut id = String::from("module-");
    for byte in module.bytes() {
        write!(id, "{byte:02x}").expect("write to String");
    }
    id
}

#[derive(Default)]
struct AndroidTools {
    projects: BTreeMap<String, Panel>,
}

#[derive(Default)]
struct Panel {
    devices: Vec<Device>,
    device_error: Option<String>,
    emulators: emulator::Manager,
    status: Option<String>,
}

#[derive(PartialEq, Eq)]
struct Device {
    serial: String,
    state: String,
    name: String,
}

impl Device {
    fn online(&self) -> bool {
        self.state == "device"
    }

    fn label(&self) -> String {
        format!("{} ({})", self.name, self.serial)
    }
}

fn parse_devices(stdout: &str) -> Vec<Device> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            if matches!(serial, "List" | "*") {
                return None;
            }
            let state = match fields.next()? {
                "no" if fields.next() == Some("permissions") => "no permissions",
                state => state,
            };
            let name = fields
                .find_map(|field| field.strip_prefix("model:"))
                .unwrap_or(serial)
                .replace('_', " ");
            Some(Device {
                serial: serial.into(),
                state: state.into(),
                name,
            })
        })
        .collect()
}

fn devices(project: &Project) -> Result<Vec<Device>, Error> {
    let result = project.run_process("adb", &["devices".into(), "-l".into()], None)?;
    if result.status != Some(0) {
        return Err(format!("ADB 查询失败\n{}", result.stderr.trim()).into());
    }
    Ok(parse_devices(&result.stdout))
}

impl Panel {
    fn refresh_devices(&mut self, project: &Project) {
        match devices(project) {
            Ok(devices) => {
                self.devices = devices;
                self.device_error = None;
            }
            Err(error) => {
                self.devices.clear();
                self.device_error = Some(error.to_string());
            }
        }
    }
}

struct Settings {
    module: String,
    variant: String,
    device: String,
    variants: BTreeMap<String, String>,
}

impl Settings {
    fn read(project: &Project) -> Self {
        let source = project.read_file(SETTINGS).unwrap_or_default();
        let mut lines = source.lines();
        let module = lines.next().unwrap_or_default().to_owned();
        let variant = lines
            .next()
            .filter(|line| line.is_empty() || gradle::valid_variant(line))
            .unwrap_or("debug")
            .to_owned();
        let device = lines.next().unwrap_or_default().to_owned();
        let mut variants: BTreeMap<_, _> = lines
            .filter_map(|line| line.split_once('\t'))
            .filter(|(module, variant)| module.starts_with(':') && gradle::valid_variant(variant))
            .map(|(module, variant)| (module.to_owned(), variant.to_owned()))
            .collect();
        if !module.is_empty() && gradle::valid_variant(&variant) {
            variants
                .entry(module.clone())
                .or_insert_with(|| variant.clone());
        }
        Self {
            module,
            variant,
            device,
            variants,
        }
    }

    fn write(&self, project: &Project) -> Result<(), Error> {
        use std::fmt::Write;
        let mut text = format!("{}\n{}\n{}\n", self.module, self.variant, self.device);
        for (module, variant) in &self.variants {
            writeln!(text, "{module}\t{variant}").expect("write to String");
        }
        project.write_file(SETTINGS, &text)
    }

    fn module<'a>(&'a self, modules: &'a gradle::Modules) -> &'a str {
        if modules.contains_key(&self.module) {
            &self.module
        } else if modules.len() == 1 {
            modules.first_key_value().expect("one module").0
        } else {
            ""
        }
    }

    fn variant<'a>(&'a self, module: &str, variants: Option<&'a [String]>) -> &'a str {
        let saved = self.variants.get(module).map(String::as_str).or_else(|| {
            (self.module.is_empty() && gradle::valid_variant(&self.variant))
                .then_some(self.variant.as_str())
        });
        let Some(variants) = variants else {
            return saved.unwrap_or("debug");
        };
        if let Some(saved) = saved.filter(|saved| variants.iter().any(|variant| variant == saved)) {
            return saved;
        }
        variants
            .iter()
            .find(|variant| variant.as_str() == "debug")
            .or_else(|| variants.iter().find(|variant| variant.ends_with("Debug")))
            .or_else(|| variants.first())
            .map(String::as_str)
            .unwrap_or_default()
    }

    fn select(&mut self, module: String, variant: String) {
        if !module.is_empty() && !variant.is_empty() {
            self.variants.insert(module.clone(), variant.clone());
        }
        self.module = module;
        self.variant = variant;
    }
}

impl Plugin for AndroidTools {
    fn run_command(
        &mut self,
        command: &str,
        ctx: &Invocation,
        _: Option<&str>,
    ) -> Result<Option<String>, Error> {
        if command != "select-device" {
            return Ok(None);
        }
        let project = ctx.project().ok_or("请先打开 Gradle 项目")?;
        let root = project.root()?;
        let panel = self.projects.entry(root.clone()).or_default();
        panel.refresh_devices(&project);
        panel.publish(&root, &project);
        if let Some(error) = &panel.device_error {
            return Err(error.clone().into());
        }
        let settings = Settings::read(&project);
        let mut items: Vec<_> = panel
            .devices
            .iter()
            .filter(|device| device.online())
            .map(|device| {
                let item = PickItem::new(&device.serial, &device.label());
                if device.serial == settings.device {
                    item.description("当前设备")
                } else {
                    item
                }
            })
            .collect();
        if items.is_empty() {
            let message = if panel
                .devices
                .iter()
                .any(|device| device.state == "unauthorized")
            {
                "请在设备上允许 USB 调试"
            } else {
                "没有已连接的设备"
            };
            host::notify(NotificationLevel::Info, "Android 设备", message);
            return Ok(None);
        }
        items.sort_by(|a, b| a.label.cmp(&b.label));
        if let Some(serial) = host::pick("选择 Android 设备", &items)? {
            let mut settings = Settings::read(&project);
            settings.device = serial;
            panel.save(&project, &settings)?;
            panel.publish(&root, &project);
        }
        Ok(None)
    }
}

impl RunConfigurations for AndroidTools {
    fn configurations(&mut self, root: &str) -> Vec<PluginRunConfiguration> {
        let Some(project) = host::project_of(root) else {
            return Vec::new();
        };
        let Ok(jvm) = gradle::jvm(&project) else {
            return Vec::new();
        };
        let settings = Settings::read(&project);
        gradle::modules(&project)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(module, variants)| {
                let variant = settings.variant(&module, variants.as_deref());
                if variant.is_empty() {
                    return None;
                }
                Some(PluginRunConfiguration {
                    id: configuration_id(&module),
                    name: format!("Android {module}"),
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        gradle::LAUNCH.into(),
                        "openidea-android".into(),
                        jvm.clone(),
                        include_str!("legacy-apks.gradle").into(),
                        module,
                        variant.into(),
                    ],
                    ..Default::default()
                })
            })
            .collect()
    }
}

impl Views for AndroidTools {
    fn render(&mut self, view: &str, _: Option<Viewport>, _: &Invocation) -> Result<View, Error> {
        if view == layout::VIEW {
            return Ok(View::new(Body::Html(layout::document())));
        }
        Ok(View::message("打开 Android Gradle 项目"))
    }

    fn handle(&mut self, view: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        let ViewEvent::Message(message) = event else {
            return Ok(());
        };
        if view == layout::VIEW {
            if let Some(request) = message.strip_prefix("read-snapshot\n") {
                return layout::send_snapshot(ctx, request);
            }
            return Ok(());
        }
        if view != VIEW {
            return Ok(());
        }
        let project = ctx.project();
        let root = project.as_ref().map(Project::root).transpose()?;
        let scope = root
            .as_deref()
            .map(configuration_id)
            .unwrap_or_else(|| "no-project".into());
        if message == "open-cli-download" {
            let error = host::open_url(setup::DOWNLOAD)
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            return host::post_view_message(VIEW, &format!("{scope}\nsetup-link\n{error}"));
        }
        if let Some(platform) = message.strip_prefix("copy-install-command\n") {
            let error = setup::install_command(platform)
                .and_then(|command| host::copy_to_clipboard(&command))
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            return host::post_view_message(VIEW, &format!("{scope}\nsetup-copy\n{error}"));
        }
        if let Some((
            action @ ("open"
            | "configure-android-environment"
            | "check-android-environment"
            | "enter-android-tools"),
            request,
        )) = message.split_once('\n')
        {
            let Some((id, _)) = view_request(request) else {
                return Ok(());
            };
            let configuration = action == "configure-android-environment";
            let gradle_project = project.as_ref().filter(|p| gradle::is_project(p));
            if configuration || (action != "enter-android-tools" && gradle_project.is_some()) {
                let environment = setup::probe(project.as_ref());
                if configuration || !environment.as_ref().is_ok_and(|env| env.ready) {
                    let mut html = setup::contents(environment, configuration);
                    if !configuration
                        && let (Some(project), Some(root)) = (gradle_project, root.as_ref())
                    {
                        let contents = self
                            .projects
                            .entry(root.clone())
                            .or_default()
                            .contents(project);
                        html.push_str("<template data-tools-preview>");
                        html.push_str(&contents);
                        html.push_str("</template>");
                    }
                    return host::post_view_message(VIEW, &format!("{scope}\nsetup\n{id}\n{html}"));
                }
            }
            let (Some(project), Some(root)) = (gradle_project, root.as_ref()) else {
                return host::post_view_message(
                    VIEW,
                    &format!("{scope}\nunsupported-project\n{id}\n"),
                );
            };
            let panel = self.projects.entry(root.clone()).or_default();
            panel.refresh_devices(project);
            if let Err(error) = panel.refresh_modules(project) {
                panel.status = Some(error.to_string());
            }
            return panel.post_contents(root, project, &format!("tools-ready\n{id}"));
        }
        let project = project.ok_or("请打开 Android Gradle 项目")?;
        let root = root.ok_or("请打开 Android Gradle 项目")?;
        if let Some(request) = message.strip_prefix("emulator\n") {
            let Some((id, command)) = view_request(request) else {
                return Ok(());
            };
            let panel = self.projects.entry(root.clone()).or_default();
            let payload = emulator::handle(panel, &project, &root, command)
                .unwrap_or_else(emulator::error_json);
            return host::post_view_message(VIEW, &format!("{scope}\nemulator\n{id}\n{payload}"));
        }
        if let Some(request) = message.strip_prefix("copy-adb-result\n") {
            let Some((id, output)) = view_request(request) else {
                return Ok(());
            };
            let error = adb::copy_result(output)
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            return host::post_view_message(
                VIEW,
                &format!("{}\nadb-copy\n{id}\n{error}", configuration_id(&root)),
            );
        }
        let panel = self.projects.entry(root.clone()).or_default();
        let result = match message.as_str() {
            "refresh-config" => {
                panel.refresh_devices(&project);
                panel.refresh_modules(&project)
            }
            _ => match message.split_once('\n') {
                Some(("inspect-layout", request)) => {
                    let Some((id, _)) = view_request(request) else {
                        return Ok(());
                    };
                    return panel.capture_layout(&root, &project, id);
                }
                Some((action @ ("open-layout" | "export-layout" | "delete-layout"), id)) => {
                    panel.status = None;
                    history::layout_action(&project, action, id)
                }
                Some(("delete-command", command)) => {
                    panel.status = None;
                    history::delete_command(&project, command)
                }
                Some(("run-adb", request)) => {
                    let Some((id, command)) = view_request(request) else {
                        return Ok(());
                    };
                    let serial = Settings::read(&project).device;
                    let result = adb::execute(&project, &serial, command);
                    let payload = adb::message(result);
                    let header = format!("{}\nadb\n{id}\n", configuration_id(&root));
                    if let Err(error) = host::post_view_message(VIEW, &format!("{header}{payload}"))
                    {
                        let failure = adb::message(Err(error));
                        host::post_view_message(VIEW, &format!("{header}{failure}"))?;
                    }
                    panel.status = None;
                    Ok(())
                }
                Some((field, value)) => panel.change(&project, field, value),
                None => return Ok(()),
            },
        };
        if let Err(error) = result {
            panel.status = Some(error.to_string());
        }
        panel.publish(&root, &project);
        Ok(())
    }
}

impl Panel {
    fn capture_layout(&mut self, root: &str, project: &Project, id: &str) -> Result<(), Error> {
        self.status = None;
        let record =
            match layout::capture(project, &Settings::read(project).device).and_then(|snapshot| {
                let mut payload = String::new();
                snapshot.write_json(&mut payload);
                history::save_layout(project, &snapshot.activity, &payload)
            }) {
                Ok(record) => Some(record),
                Err(error) => {
                    self.status = Some(error.to_string());
                    None
                }
            };
        // Release the panel's pending capture before opening an editor can move focus.
        let posted = self.post_contents(root, project, &format!("capture\n{id}"));
        if let Some(record) = record
            && let Err(error) = history::open_layout(&record)
        {
            self.status = Some(error.to_string());
            self.publish(root, project);
        }
        posted
    }

    fn publish(&self, root: &str, project: &Project) {
        let _ = self.post_contents(root, project, "panel");
    }

    fn contents(&self, project: &Project) -> String {
        let settings = Settings::read(project);
        let modules = gradle::modules(project).unwrap_or_default();
        let layouts = history::layouts(project);
        let commands = history::commands(project);
        panel::contents(self, &settings, &modules, layouts, commands)
    }

    fn post_contents(&self, root: &str, project: &Project, update: &str) -> Result<(), Error> {
        let html = self.contents(project);
        host::post_view_message(
            VIEW,
            &format!("{}\n{update}\n{html}", configuration_id(root)),
        )
    }

    fn save(&self, project: &Project, settings: &Settings) -> Result<(), Error> {
        settings.write(project)?;
        if let Some(modules) = gradle::modules(project) {
            update_run_index(project, &modules, settings.module(&modules))?;
        }
        Ok(())
    }

    fn change(&mut self, project: &Project, field: &str, value: &str) -> Result<(), Error> {
        if value.contains(['\r', '\n', '\0']) {
            return Err("无效的 Android 配置".into());
        }
        let mut settings = Settings::read(project);
        let modules = gradle::modules(project).unwrap_or_default();
        match field {
            "device"
                if value.is_empty()
                    || self
                        .devices
                        .iter()
                        .any(|device| device.online() && device.serial == value) =>
            {
                settings.device = value.into();
            }
            "device" => return Err("所选设备不可用".into()),
            "module" => {
                let variants = modules.get(value).ok_or("请选择 Gradle 识别的应用模块")?;
                let variant = settings.variant(value, variants.as_deref()).to_owned();
                settings.select(value.into(), variant);
            }
            "variant" => {
                let module = settings.module(&modules);
                let variants = modules.get(module).and_then(|variants| variants.as_deref());
                if !variants.is_some_and(|variants| variants.iter().any(|variant| variant == value))
                {
                    return Err("请选择当前应用模块支持的构建变体".into());
                }
                settings.select(module.into(), value.into());
            }
            _ => return Ok(()),
        }
        self.save(project, &settings)?;
        self.status = None;
        Ok(())
    }

    fn refresh_modules(&mut self, project: &Project) -> Result<(), Error> {
        let modules = gradle::refresh(project)?;
        let mut settings = Settings::read(project);
        let module = settings.module(&modules);
        let variant = if module.is_empty() {
            ""
        } else {
            settings.variant(module, modules[module].as_deref())
        };
        if module != settings.module || variant != settings.variant {
            settings.select(module.into(), variant.into());
            settings.write(project)?;
        }
        update_run_index(project, &modules, &settings.module)?;
        self.status = modules
            .is_empty()
            .then(|| "Gradle 未发现 Android 应用模块".into());
        Ok(())
    }
}

fn update_run_index(
    project: &Project,
    modules: &gradle::Modules,
    selected: &str,
) -> Result<(), Error> {
    let ids: Vec<_> = modules
        .keys()
        .map(|module| format!("\"{}\"", configuration_id(module)))
        .collect();
    let selected = if selected.is_empty() {
        String::new()
    } else {
        configuration_id(selected)
    };
    // The 0.1.51 run widget re-queries configuration providers on watched index-*.json changes.
    project.write_file(
        RUN_INDEX,
        &format!(
            "{{\"selected\":\"{selected}\",\"configurations\":[{}]}}\n",
            ids.join(",")
        ),
    )
}

register_plugin!(AndroidTools, views, run_configurations);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_configuration_is_available_when_android_cli_is_ready_or_missing() {
        use openidea_plugin::{ProcessOutput, mock::MockHost};
        use std::{cell::Cell, rc::Rc};

        let mock = MockHost::install().with_capabilities(&[
            "project.read",
            "project.write",
            "process:sh",
            "process:adb",
            "clipboard",
        ]);
        let installed = Rc::new(Cell::new(true));
        let host_installed = Rc::clone(&installed);
        mock.on_process("sh", move |args, _| {
            if args
                .last()
                .is_some_and(|name| name == "openidea-android-environment")
            {
                Ok(ProcessOutput {
                    status: Some(0),
                    stdout: if host_installed.get() {
                        "ready\nDarwin\narm64\n"
                    } else {
                        "missing\nDarwin\narm64\n"
                    }
                    .into(),
                    stderr: String::new(),
                })
            } else {
                Err("Gradle unavailable in fixture".into())
            }
        });
        mock.on_process("adb", |_, _| {
            Ok(ProcessOutput {
                status: Some(0),
                stdout: "List of devices attached\n".into(),
                stderr: String::new(),
            })
        });
        let ctx = mock.invocation();
        let send = |plugin: &mut AndroidTools, message: &str| {
            plugin
                .handle(VIEW, ViewEvent::Message(message.into()), &ctx)
                .unwrap();
        };
        let last = || mock.view_messages().pop().unwrap().1;
        let mut plugin = AndroidTools::default();
        send(&mut plugin, "open\nunsupported\n");
        assert!(last().contains("\nunsupported-project\nunsupported\n"));
        mock.set_project_root(None);
        send(&mut plugin, "configure-android-environment\nstandalone\n");
        assert!(last().starts_with("no-project\nsetup\nstandalone\n"));
        send(&mut plugin, "enter-android-tools\nstandalone-back\n");
        assert!(last().starts_with("no-project\nunsupported-project\nstandalone-back\n"));
        mock.set_project_root(Some("/project"));
        mock.add_file("settings.gradle", "");
        send(&mut plugin, "open\nfirst\n");
        assert!(last().contains("\ntools-ready\nfirst\n"));
        send(
            &mut plugin,
            "configure-android-environment\nconfiguration\n",
        );
        let page = last();
        assert!(page.contains("\nsetup\nconfiguration\n"));
        assert!(page.contains("配置Android环境"));
        assert!(page.contains("Android 环境已就绪"));
        assert!(page.contains("install.sh | bash\" selected>macOS · Apple 芯片"));
        assert!(page.contains("data-action=\"close-environment\">关闭"));
        assert!(!page.contains("<template data-tools-preview>"));
        assert!(page.contains("data-action=\"configure-android-environment\">重新检测"));
        send(&mut plugin, "enter-android-tools\nback\n");
        assert!(last().contains("\ntools-ready\nback\n"));
        installed.set(false);
        send(
            &mut plugin,
            "configure-android-environment\nconfiguration-missing\n",
        );
        assert!(last().contains("\nsetup\nconfiguration-missing\n"));
        assert!(last().contains("未检测到 Android 环境"));
        send(&mut plugin, "open\nmissing\n");
        let first_missing = last();
        assert!(first_missing.contains("\nsetup\nmissing\n"));
        assert!(first_missing.contains("未检测到 Android 环境"));
        assert!(first_missing.contains("install.sh | bash\" selected>macOS · Apple 芯片"));
        let (_, tools) = first_missing
            .split_once("<template data-tools-preview>")
            .unwrap();
        assert!(tools.contains("设备配置"));
        assert!(tools.contains("模拟器管理"));
        assert!(tools.contains("布局检查"));
        assert!(tools.contains("ADB 命令"));
        send(&mut plugin, "open-cli-download");
        assert_eq!(mock.opened_urls(), [setup::DOWNLOAD]);
        send(&mut plugin, "copy-install-command\nwindows_x86_64");
        assert_eq!(
            mock.clipboard().unwrap(),
            setup::install_command("windows_x86_64").unwrap()
        );
        send(&mut plugin, "enter-android-tools\nskip\n");
        assert!(last().contains("\ntools-ready\nskip\n"));
        installed.set(true);
        send(&mut plugin, "check-android-environment\nrecheck\n");
        assert!(last().contains("\ntools-ready\nrecheck\n"));
    }

    #[test]
    fn launch_scripts_fit_openidea_run_configuration_argument_limits() {
        assert!(gradle::LAUNCH.len() <= 4096);
        assert!(include_str!("legacy-apks.gradle").len() <= 4096);
    }

    #[test]
    fn run_ids_are_distinct_for_modules_with_similar_paths() {
        assert_ne!(configuration_id(":demo:app"), configuration_id(":demo-app"));
        assert_eq!(configuration_id(":app"), "module-3a617070");
        assert!(configuration_id(":features:very-long-application-module-name").len() <= 64);
    }

    #[test]
    fn variant_choices_stay_with_their_application_module() {
        let mut settings = Settings {
            module: ":one".into(),
            variant: "release".into(),
            device: String::new(),
            variants: BTreeMap::from([(":one".into(), "release".into())]),
        };
        let one = ["debug".into(), "release".into()];
        let two = ["demoRelease".into(), "paidDebug".into()];
        assert_eq!(settings.variant(":one", Some(&one)), "release");
        assert_eq!(settings.variant(":two", Some(&two)), "paidDebug");
        settings.select(":two".into(), "demoRelease".into());
        assert_eq!(settings.variant(":two", Some(&two)), "demoRelease");
        assert_eq!(settings.variant(":one", Some(&one)), "release");
        assert_eq!(settings.variant(":two", Some(&[])), "");
    }

    #[test]
    fn parses_adb_status_and_model() {
        let devices = parse_devices(
            "List of devices attached\nemulator-5554     device product:sdk model:Pixel_8\nUSB123\tunauthorized usb:1\n",
        );
        assert_eq!(devices.len(), 2);
        assert!(devices[0].online());
        assert_eq!(devices[0].label(), "Pixel 8 (emulator-5554)");
        assert!(!devices[1].online());
    }
}
