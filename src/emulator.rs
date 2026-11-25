use crate::{Panel, Settings, devices};
use openidea_plugin::{
    Error, ProcessOutput,
    host::{self, Child, Chunk, Command, Project},
    text::escape_json,
};
use std::collections::{BTreeMap, BTreeSet};

const SDK_ENV: &str = include_str!("emulator-env.sh");
const CREATE: &str = concat!(
    include_str!("emulator-avdmanager.sh"),
    include_str!("android-cli.sh"),
    include_str!("emulator-create.sh")
);
const LIST: &str = r#"
require_emulator
names=$("$emulator" -list-avds)
printf '%s\n' "$names" | while IFS= read -r name; do
    [ -n "$name" ] || continue
    config=$(avd_config "$name") || config=
    image=
    if [ -n "$config" ]; then
        image=$(sed -n 's/^image\.sysdir\.1[[:space:]]*=[[:space:]]*//p' "$config" | tr -d '\r')
    fi
    printf '%s\t%s\n' "$name" "$image"
done
"#;
const PROFILES: &str = concat!(
    include_str!("emulator-avdmanager.sh"),
    r#"
require_avdmanager
printf 'architecture: %s\n' "$(uname -m)"
"$avdmanager" list device
"#
);
const IMAGE_USERS: &str = include_str!("emulator-image-users.sh");
const DELETE_AVD: &str = concat!(
    include_str!("emulator-avdmanager.sh"),
    r#"
require_avdmanager
config=$(avd_config "$1") || config=
if [ -n "$config" ] && [ -e "${config%/*}/hardware-qemu.ini.lock" ]; then
    fail '请先停止模拟器。'
fi
"$avdmanager" delete avd -n "$1"
"#
);
const DELETE_IMAGE: &str = concat!(
    include_str!("android-cli.sh"),
    r#"
cli=$(android_cli_path) || fail '找不到 Android CLI。'
"$cli" --sdk="$sdk" sdk remove --ignore-outdated-xmls "$1" </dev/null
"#
);
const INSTALLED_IMAGES: &str = r#"
printf 'Installed packages:\n'
for folder in "$sdk"/system-images/*/*/*; do
    if [ -f "$folder/source.properties" ] || [ -f "$folder/package.xml" ]; then
        printf '%s\n' "${folder#"$sdk"/}"
    fi
done
"#;
const REMOTE_IMAGES: &str = concat!(
    include_str!("android-cli.sh"),
    r#"
cli=$(android_cli_path) || fail '找不到 Android CLI。'
"$cli" --sdk="$sdk" sdk list --all 'system-images*'
"#
);
const START: &str = r#"
require_emulator
exec "$emulator" -avd "$1"
"#;

#[derive(Default)]
pub struct Manager {
    launches: BTreeMap<String, Launch>,
    stopping: BTreeSet<String>,
    catalog: Option<Catalog>,
    creation: Option<Creation>,
    notice: Option<String>,
}

struct Launch {
    child: Child,
    output: String,
}

struct Creation {
    task: Launch,
    name: String,
    start: bool,
    progress: CreationProgress,
}

struct CreationProgress {
    phase: &'static str,
    percent: Option<u8>,
    line: String,
}

impl CreationProgress {
    fn new(installed: bool) -> Self {
        Self {
            phase: if installed { "create" } else { "download" },
            percent: None,
            line: String::new(),
        }
    }

    fn record(&mut self, output: &str) {
        for part in output.split_inclusive(['\r', '\n']) {
            self.line.push_str(part);
            if !part.ends_with(['\r', '\n']) {
                if self.line.len() > 4096 {
                    self.line.clear();
                }
                continue;
            }
            let line = std::mem::take(&mut self.line);
            let line = line.trim();
            if line == "@create" {
                self.phase = "create";
                self.percent = None;
            } else if line == "@download" {
                self.phase = "download";
                self.percent = None;
            } else if self.phase != "create" {
                if line.starts_with("Unzipping ") && self.phase != "unpack" {
                    self.phase = "unpack";
                    self.percent = None;
                }
                if let Some(percent) = line.split_whitespace().find_map(|part| {
                    part.trim_matches(['[', ']', '(', ')'])
                        .strip_suffix('%')?
                        .parse::<f32>()
                        .ok()
                        .filter(|number| (0.0..=100.0).contains(number))
                        .map(|number| number.floor() as u8)
                }) {
                    self.percent = Some(
                        self.percent
                            .map_or(percent, |previous| previous.max(percent)),
                    );
                }
            }
        }
    }

    fn json(&self) -> String {
        let percent = self
            .percent
            .map_or_else(|| "null".into(), |value| value.to_string());
        format!(
            r#"{{"working":true,"phase":"{}","percent":{percent}}}"#,
            self.phase
        )
    }
}

struct Avd {
    name: String,
    api: String,
    serial: Option<String>,
    ready: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Phone,
    Tablet,
    Tv,
    Wear,
    Automotive,
    Desktop,
}

impl Form {
    fn id(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Tablet => "tablet",
            Self::Tv => "tv",
            Self::Wear => "wear",
            Self::Automotive => "automotive",
            Self::Desktop => "desktop",
        }
    }
}

struct Profile {
    id: String,
    name: String,
    form: Form,
}

struct Image {
    path: String,
    api: String,
    tag: String,
    abi: String,
    installed: bool,
    form: Form,
}

struct Catalog {
    profiles: Vec<Profile>,
    images: Vec<Image>,
    warning: Option<String>,
}

fn string(value: &str) -> String {
    format!("\"{}\"", escape_json(value))
}

fn field(value: &str) -> bool {
    !value.is_empty() && value.len() <= 160 && !value.chars().any(char::is_control)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn deletable_name(name: &str) -> bool {
    field(name)
        && !name.starts_with('-')
        && !name.contains(['/', '\\'])
        && !matches!(name, "." | "..")
}

fn property_path(project: &Project) -> String {
    let source = project.read_file("local.properties").unwrap_or_default();
    let Some(value) = source.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "sdk.dir").then_some(value.trim())
    }) else {
        return String::new();
    };
    // Java properties escape common Windows path characters and spaces.
    let mut path = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' && matches!(chars.as_str().chars().next(), Some('\\' | ':' | ' ' | '=')) {
            path.extend(chars.next());
        } else {
            path.push(ch);
        }
    }
    path
}

fn command(project: &Project, script: &str, args: &[&str]) -> Command {
    let script = format!("{SDK_ENV}\n{script}");
    debug_assert!(script.len() <= 4096);
    Command::new("sh")
        .args([
            "-c",
            &script,
            "openidea-android-emulator",
            &property_path(project),
        ])
        .args(args)
}

fn run(project: &Project, script: &str, args: &[&str]) -> Result<ProcessOutput, Error> {
    host::run_command(&command(project, script, args), Some(project), None)
}

fn checked(output: ProcessOutput, context: &str) -> Result<String, Error> {
    if output.status == Some(0) {
        return Ok(output.stdout);
    }
    let detail = if output.stderr.trim().is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    Err(message(context, detail).into())
}

fn message(context: &str, detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        return context.into();
    }
    let start = detail
        .char_indices()
        .rev()
        .nth(1499)
        .map_or(0, |(index, _)| index);
    format!("{context}\n{}", &detail[start..])
}

fn profile_form(id: &str, name: &str, tag: &str) -> Option<Form> {
    let label = format!("{id} {name}").to_ascii_lowercase();
    let tag = tag.to_ascii_lowercase();
    if label.contains("glasses") || tag.contains("xr") {
        return None;
    }
    Some(match tag.as_str() {
        tag if tag.contains("automotive") => Form::Automotive,
        tag if tag.contains("wear") => Form::Wear,
        tag if tag.contains("desktop") => Form::Desktop,
        tag if tag.contains("tv") => Form::Tv,
        _ if id.starts_with("tv_") || label.contains("television") => Form::Tv,
        _ if label.contains("tablet")
            || matches!(id, "Nexus 7" | "Nexus 7 2012" | "Nexus 9" | "Nexus 10") =>
        {
            Form::Tablet
        }
        _ => Form::Phone,
    })
}

fn parse_profiles(output: &str) -> Vec<Profile> {
    output
        .split("---------")
        .filter_map(|record| {
            let id = record.lines().find_map(|line| {
                let (_, id) = line.trim().strip_prefix("id:")?.split_once(" or \"")?;
                id.strip_suffix('"')
            })?;
            let name = record
                .lines()
                .find_map(|line| line.trim().strip_prefix("Name:").map(str::trim))?;
            let tag = record
                .lines()
                .find_map(|line| line.trim().strip_prefix("Tag :").map(str::trim))
                .unwrap_or("");
            let form = profile_form(id, name, tag)?;
            field(id).then(|| Profile {
                id: id.into(),
                name: name.into(),
                form,
            })
        })
        .collect()
}

fn image(path: &str, installed: bool) -> Option<Image> {
    let fields: Vec<_> = path.split('/').collect();
    let ["system-images", platform, tag, abi] = fields.as_slice() else {
        return None;
    };
    let api = platform.strip_prefix("android-")?;
    if [api, tag, abi].iter().any(|value| {
        value.is_empty()
            || !value.as_bytes()[0].is_ascii_alphanumeric()
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    }) {
        return None;
    }
    let form = match *tag {
        tag if tag.contains("automotive") => Form::Automotive,
        tag if tag.contains("wear") => Form::Wear,
        tag if tag.contains("desktop") => Form::Desktop,
        tag if tag.contains("tv") => Form::Tv,
        tag if tag.contains("xr") || tag.contains("glasses") => return None,
        _ => Form::Phone,
    };
    Some(Image {
        path: path.into(),
        api: api.into(),
        tag: tag.to_string(),
        abi: abi.to_string(),
        installed,
        form,
    })
}

fn compatible_arch(arch: &str, abi: &str) -> bool {
    match arch {
        "arm64" | "aarch64" => abi == "arm64-v8a",
        "x86_64" | "amd64" | "AMD64" => matches!(abi, "x86_64" | "x86"),
        _ => true,
    }
}

fn parse_images(arch: &str, output: &str) -> Vec<Image> {
    let mut installed = false;
    let mut images = BTreeMap::new();
    for line in output.lines().map(str::trim) {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("installed packages:") {
            installed = true;
        } else if lower.starts_with("available packages:") {
            installed = false;
        }
        let Some(path) = line.split_whitespace().next() else {
            continue;
        };
        if let Some(entry) = image(path, installed).filter(|entry| {
            compatible_arch(arch, &entry.abi)
                && (installed
                    || entry
                        .api
                        .split('.')
                        .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit())))
        }) {
            images
                .entry(path)
                .and_modify(|old: &mut Image| old.installed |= entry.installed)
                .or_insert(entry);
        }
    }
    images.into_values().collect()
}

fn compatible_form(profile: &Profile, image: &Image) -> bool {
    (profile.form == image.form || (profile.form == Form::Tablet && image.form == Form::Phone))
        && (!image.tag.contains("distantdisplay") || profile.id.contains("distant_display"))
}

impl Catalog {
    fn load(project: &Project) -> Result<Self, Error> {
        let _task = host::begin_task("读取模拟器硬件与系统镜像", true);
        let output = checked(run(project, PROFILES, &[])?, "读取模拟器硬件配置失败")?;
        let arch = output
            .lines()
            .find_map(|line| line.strip_prefix("architecture: "))
            .unwrap_or("");
        let profiles = parse_profiles(&output);
        if profiles.is_empty() {
            return Err("未找到 Android SDK 的硬件配置".into());
        }
        let mut output = checked(run(project, INSTALLED_IMAGES, &[])?, "读取已安装镜像失败")?;
        let remote = run(project, REMOTE_IMAGES, &[])
            .and_then(|result| checked(result, "读取远程镜像列表失败"));
        let warning = match remote {
            Ok(available) => {
                output.push('\n');
                output.push_str(&available);
                None
            }
            Err(error) => Some(message("仍可使用已安装的镜像。", &error.to_string())),
        };
        let images = parse_images(arch, &output);
        Ok(Self {
            profiles,
            images,
            warning,
        })
    }

    fn json(&self) -> String {
        let profiles = self
            .profiles
            .iter()
            .map(|p| {
                format!(
                    r#"{{"id":{},"name":{},"form":{}}}"#,
                    string(&p.id),
                    string(&p.name),
                    string(p.form.id())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let images = self
            .images
            .iter()
            .map(|i| {
                format!(
                    r#"{{"path":{},"api":{},"tag":{},"abi":{},"form":{},"installed":{}}}"#,
                    string(&i.path),
                    string(&i.api),
                    string(&i.tag),
                    string(&i.abi),
                    string(i.form.id()),
                    i.installed
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"profiles":[{profiles}],"images":[{images}],"warning":{}}}"#,
            self.warning
                .as_deref()
                .map(string)
                .unwrap_or_else(|| "null".into())
        )
    }
}

impl Launch {
    fn start(project: &Project, script: &str, args: &[&str]) -> Result<Self, Error> {
        let child = host::start(&command(project, script, args), Some(project))?;
        child.close_stdin()?;
        Ok(Self {
            child,
            output: String::new(),
        })
    }

    fn poll(&mut self) -> Result<Chunk, Error> {
        let chunk = self.child.poll(16 * 1024)?;
        self.output.push_str(&chunk.stdout);
        self.output.push_str(&chunk.stderr);
        if let Some((index, _)) = self.output.char_indices().rev().nth(2999) {
            self.output.drain(..index);
        }
        Ok(chunk)
    }
}

fn adb(project: &Project, serial: &str, args: &[&str]) -> Result<String, Error> {
    let args = ["-s", serial]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    checked(
        project.run_process("adb", &args, None)?,
        "模拟器 ADB 命令失败",
    )
}

fn avds(project: &Project, online: &[crate::Device]) -> Result<Vec<Avd>, Error> {
    let output = checked(run(project, LIST, &[])?, "读取模拟器列表失败")?;
    let mut records: BTreeMap<_, _> = output
        .lines()
        .filter_map(|line| {
            let (name, path) = line.split_once('\t')?;
            if !field(name) {
                return None;
            }
            let path = path.trim().trim_end_matches(['/', '\\']).replace('\\', "/");
            let api = image(&path, true).map(|im| im.api).unwrap_or_default();
            Some((
                name.to_owned(),
                Avd {
                    name: name.to_owned(),
                    api,
                    serial: None,
                    ready: false,
                },
            ))
        })
        .collect();

    for device in online
        .iter()
        .filter(|device| device.online() && device.serial.starts_with("emulator-"))
    {
        let Ok(reply) = adb(project, &device.serial, &["emu", "avd", "name"]) else {
            continue;
        };
        let Some(name) = reply
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && *line != "OK")
        else {
            continue;
        };
        if let Some(record) = records.get_mut(name) {
            record.serial = Some(device.serial.clone());
            record.ready = true;
        }
    }
    Ok(records.into_values().collect())
}

struct ImageUsage {
    package: Option<String>,
    shared: bool,
}

fn image_usage(project: &Project, name: &str, package: &str) -> Result<ImageUsage, Error> {
    let output = checked(
        run(project, IMAGE_USERS, &[name, package])?,
        "检查系统镜像失败",
    )?;
    let mut lines = output.lines();
    let package = lines
        .next()
        .filter(|path| image(path, true).is_some())
        .map(str::to_owned);
    Ok(ImageUsage {
        package,
        shared: lines.any(|line| line == "shared"),
    })
}

impl Manager {
    fn check_deletable(&self, project: &Project, name: &str) -> Result<(), Error> {
        let online = devices(project)?;
        let avd = avds(project, &online)?
            .into_iter()
            .find(|avd| avd.name == name)
            .ok_or("找不到所选模拟器")?;
        if avd.serial.is_some() || self.launches.contains_key(name) || self.stopping.contains(name)
        {
            return Err("请先停止模拟器".into());
        }
        Ok(())
    }

    fn delete_preview(&self, project: &Project, name: &str) -> Result<String, Error> {
        self.check_deletable(project, name)?;
        let usage = image_usage(project, name, "")?;
        Ok(format!(
            r#"{{"image":{},"shared":{}}}"#,
            usage
                .package
                .as_deref()
                .map(string)
                .unwrap_or_else(|| "null".into()),
            usage.shared
        ))
    }

    fn delete(&self, project: &Project, name: &str, package: &str) -> Result<String, Error> {
        self.check_deletable(project, name)?;
        if !package.is_empty() {
            let usage = image_usage(project, name, "")?;
            if usage.package.as_deref() != Some(package) {
                return Err("System Image 已变化，请重新确认。".into());
            }
            if usage.shared {
                return Err("System Image 正被其他模拟器使用，请取消勾选后重试。".into());
            }
        }

        let _task = host::begin_task("删除 Android 模拟器", true);
        checked(run(project, DELETE_AVD, &[name])?, "删除模拟器失败")?;
        if avds(project, &[])?.iter().any(|avd| avd.name == name) {
            return Err("模拟器仍在列表中，删除未完成。".into());
        }
        let notice = if package.is_empty() {
            None
        } else {
            self.delete_image(project, package)
                .err()
                .map(|error| format!("模拟器已删除。{error}"))
        };
        Ok(format!(
            r#"{{"notice":{}}}"#,
            notice
                .as_deref()
                .map(string)
                .unwrap_or_else(|| "null".into())
        ))
    }

    fn delete_image(&self, project: &Project, package: &str) -> Result<(), Error> {
        let usage = image_usage(project, "", package)?;
        if usage.shared {
            return Err("System Image 正被其他模拟器使用，已保留镜像。".into());
        }
        if usage.package.as_deref() != Some(package) {
            return Err("System Image 已变化，请刷新后确认。".into());
        }
        checked(
            run(project, DELETE_IMAGE, &[package])?,
            "删除 System Image 失败",
        )?;
        Ok(())
    }

    fn rows(&mut self, project: &Project, devices: &[crate::Device]) -> Result<String, Error> {
        let mut records = avds(project, devices)?;
        self.launches.retain(|name, launch| {
            let outcome = launch.poll().map(|chunk| chunk.status);
            if let Ok(None) = outcome {
                if let Some(avd) = records
                    .iter_mut()
                    .find(|avd| &avd.name == name && avd.serial.is_some())
                {
                    avd.ready = avd
                        .serial
                        .as_deref()
                        .and_then(|serial| {
                            adb(project, serial, &["shell", "getprop", "sys.boot_completed"]).ok()
                        })
                        .is_some_and(|value| value.trim() == "1");
                }
                return true;
            }
            if !self.stopping.contains(name) && !matches!(outcome, Ok(Some(0))) {
                let detail = match outcome {
                    Err(error) => error.to_string(),
                    _ => launch.output.clone(),
                };
                self.notice = Some(message(&format!("{name} 启动失败"), &detail));
            }
            false
        });
        self.stopping.retain(|name| {
            self.launches.contains_key(name)
                || records
                    .iter()
                    .any(|row| &row.name == name && row.serial.is_some())
        });
        let mut rows = Vec::new();
        for avd in records {
            let status = if self.stopping.contains(&avd.name) {
                "stopping"
            } else if avd.ready {
                "running"
            } else if self.launches.contains_key(&avd.name) {
                "starting"
            } else {
                "stopped"
            };
            rows.push(format!(
                r#"{{"name":{},"api":{},"serial":{},"state":{}}}"#,
                string(&avd.name),
                string(&avd.api),
                avd.serial
                    .as_deref()
                    .map(string)
                    .unwrap_or_else(|| "null".into()),
                string(status)
            ));
        }
        let notice = self
            .notice
            .take()
            .as_deref()
            .map(string)
            .unwrap_or_else(|| "null".into());
        Ok(format!(
            r#"{{"rows":[{}],"notice":{notice}}}"#,
            rows.join(",")
        ))
    }

    fn launch(&mut self, project: &Project, name: &str) -> Result<(), Error> {
        if self.launches.contains_key(name) {
            return Ok(());
        }
        let task = Launch::start(project, START, &[name])?;
        self.launches.insert(name.into(), task);
        Ok(())
    }

    fn create(&mut self, project: &Project, input: &[&str]) -> Result<String, Error> {
        let &[name, profile, image, orientation, start] = input else {
            return Err("模拟器配置不完整".into());
        };
        if self.creation.is_some() {
            // A wizard can be closed by the IDE before it collects the result.
            let _ = self.creation_status(project);
            if self.creation.is_some() {
                return Err("已有模拟器正在创建".into());
            }
        }
        if !valid_name(name) {
            return Err("名称须以字母或数字开头，可使用字母、数字、点、下划线和连字符。".into());
        }
        if !matches!(orientation, "default" | "portrait" | "landscape")
            || !matches!(start, "0" | "1")
        {
            return Err("无效的模拟器选项".into());
        }
        let Some(catalog) = &self.catalog else {
            return Err("请重新读取模拟器配置".into());
        };
        let profile = catalog
            .profiles
            .iter()
            .find(|p| p.id == profile)
            .ok_or("请选择有效的硬件配置")?;
        let image = catalog
            .images
            .iter()
            .find(|i| i.path == image)
            .ok_or("请选择有效的系统镜像")?;
        if !compatible_form(profile, image)
            || (profile.form == Form::Tv && orientation == "portrait")
        {
            return Err("所选镜像或启动方向与设备类型不兼容".into());
        }
        if avds(project, &[])?
            .iter()
            .any(|avd| avd.name.eq_ignore_ascii_case(name))
        {
            return Err("已存在同名模拟器".into());
        }
        let task = Launch::start(
            project,
            CREATE,
            &[name, &profile.id, &image.path, orientation],
        )?;
        let progress = CreationProgress::new(image.installed);
        let status = progress.json();
        self.creation = Some(Creation {
            task,
            name: name.into(),
            start: start == "1",
            progress,
        });
        Ok(status)
    }

    fn creation_status(&mut self, project: &Project) -> Result<String, Error> {
        let Some(mut job) = self.creation.take() else {
            return Err("创建任务已结束，请刷新模拟器列表。".into());
        };
        let chunk = job.task.poll()?;
        match chunk.status {
            None => {
                job.progress.record(&chunk.stdout);
                let status = job.progress.json();
                self.creation = Some(job);
                Ok(status)
            }
            Some(0) => {
                let warning = if job.start {
                    self.launch(project, &job.name)
                        .err()
                        .map(|error| error.to_string())
                } else {
                    None
                };
                Ok(format!(
                    r#"{{"created":{},"notice":{}}}"#,
                    string(&job.name),
                    warning
                        .as_deref()
                        .map(string)
                        .unwrap_or_else(|| "null".into())
                ))
            }
            Some(_) => Err(message("创建模拟器失败", &job.task.output).into()),
        }
    }
}

pub fn handle(
    panel: &mut Panel,
    project: &Project,
    root: &str,
    request: &str,
) -> Result<String, Error> {
    let parts: Vec<_> = request.split('\n').collect();
    match parts.as_slice() {
        ["catalog"] => {
            let catalog = Catalog::load(project)?;
            let response = catalog.json();
            panel.emulators.catalog = Some(catalog);
            Ok(response)
        }
        ["create", options @ ..] => panel.emulators.create(project, options),
        ["creation-status"] => panel.emulators.creation_status(project),
        ["delete-preview", name] if deletable_name(name) => {
            panel.emulators.delete_preview(project, name)
        }
        ["delete", name, package] if deletable_name(name) => {
            panel.emulators.delete(project, name, package)
        }
        ["list"] => {
            if let Ok(online) = devices(project)
                && (online != panel.devices || panel.device_error.is_some())
            {
                panel.devices = online;
                panel.device_error = None;
                panel.publish(root, project);
            }
            panel.emulators.rows(project, &panel.devices)
        }
        [action @ ("start" | "stop" | "select"), name] if field(name) => {
            let online = devices(project)?;
            let avd = avds(project, &online)?
                .into_iter()
                .find(|avd| avd.name == *name)
                .ok_or("找不到所选模拟器")?;
            let current = panel.emulators.launches.get(&avd.name);
            match *action {
                "start" if avd.serial.is_none() && current.is_none() => {
                    panel.emulators.launch(project, name)?
                }
                "stop" => {
                    if let Some(serial) = &avd.serial {
                        adb(project, serial, &["emu", "kill"])?;
                        panel.emulators.stopping.insert(avd.name);
                    } else if let Some(launch) = current {
                        launch.child.kill()?;
                        panel.emulators.stopping.insert(avd.name);
                    }
                }
                "select" => {
                    let serial = avd.serial.ok_or("模拟器尚未连接")?;
                    if current.is_some()
                        && adb(
                            project,
                            &serial,
                            &["shell", "getprop", "sys.boot_completed"],
                        )?
                        .trim()
                            != "1"
                    {
                        return Err("模拟器尚未启动完成".into());
                    }
                    if panel.emulators.stopping.contains(*name) {
                        return Err("模拟器正在停止".into());
                    }
                    let mut settings = Settings::read(project);
                    settings.device = serial;
                    panel.save(project, &settings)?;
                    panel.devices = online;
                    panel.publish(root, project);
                }
                _ => {}
            }
            Ok(r#"{}"#.into())
        }
        _ => Err("无法识别模拟器操作".into()),
    }
}

pub fn error_json(error: Error) -> String {
    format!(r#"{{"error":{}}}"#, string(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_catalog_keeps_installed_images_and_filters_by_device_and_architecture() {
        let profiles = parse_profiles(
            r#"Available devices definitions:
id: 15 or "medium_phone"
    Name: Medium Phone
---------
id: 16 or "medium_tablet"
    Name: Medium Tablet
---------
id: 66 or "tv_1080p"
    Name: Television (1080p)
    Tag : android-tv
"#,
        );
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[1].form.id(), "tablet");
        assert_eq!(profiles[2].form.id(), "tv");

        let images = parse_images(
            "arm64",
            "Installed packages:\n  system-images/android-35/default/arm64-v8a\n\
             Available packages:\n  system-images/android-35/default/arm64-v8a  2.0\n\
             system-images/android-35/default/x86_64  2.0\n\
             system-images/android-36/google-tv/arm64-v8a  4.0\n\
             system-images/android-37.2-beta2/google_apis/arm64-v8a  3.0",
        );
        assert_eq!(images.len(), 2);
        assert!(images[0].installed);
        assert!(!images[1].installed);
        assert!(compatible_form(&profiles[1], &images[0]));
        assert!(!compatible_form(&profiles[1], &images[1]));
        assert!(compatible_form(&profiles[2], &images[1]));
    }

    #[test]
    fn creation_reports_download_unpack_and_create_from_streamed_output() {
        let mut progress = CreationProgress::new(false);
        assert_eq!(
            progress.json(),
            r#"{"working":true,"phase":"download","percent":null}"#
        );
        progress.record("@down");
        progress.record("load\n\rhttps://dl.google.com/android/repository/image.zip...\n");
        assert_eq!(progress.percent, None);
        progress.record("Downloading [ 4");
        progress.record("2%]\rDownloading [ 37%]\r");
        assert_eq!(progress.percent, Some(42));
        progress.record("Unzip");
        progress.record("ping system-images......\n");
        assert_eq!(
            progress.json(),
            r#"{"working":true,"phase":"unpack","percent":null}"#
        );
        progress.record("Unzipping [20%]\r");
        assert_eq!(progress.percent, Some(20));
        progress.record("\n@create\nActual creation output 99%\n");
        assert_eq!(
            progress.json(),
            r#"{"working":true,"phase":"create","percent":null}"#
        );
    }

    #[test]
    fn emulator_shell_arguments_fit_the_host_limit() {
        for script in [
            CREATE,
            LIST,
            PROFILES,
            INSTALLED_IMAGES,
            REMOTE_IMAGES,
            START,
            IMAGE_USERS,
            DELETE_AVD,
            DELETE_IMAGE,
        ] {
            assert!(SDK_ENV.len() + 1 + script.len() <= 4096);
        }
        assert!(valid_name("Android_TV.35-test"));
        assert!(!valid_name("--unsafe"));
        assert!(!valid_name("../another"));
        assert!(image("system-images/android-35/default/../arm64-v8a", true).is_none());
    }
}
