use openidea_plugin::{
    Error,
    host::{self, Project},
};
use std::fmt::Write;

const LAYOUTS: &str = ".gradle/openidea-android-layouts";
const LAYOUT_INDEX: &str = ".gradle/openidea-android-layouts/history.tsv";
const COMMANDS: &str = ".gradle/openidea-android-adb-history";
const COMMAND_LIMIT: usize = 50;

pub struct Layout {
    pub id: u64,
    pub name: String,
}

impl Layout {
    pub fn filename(&self) -> String {
        format!("{}-{}.oialayout", self.name, self.id)
    }

    fn path(&self) -> String {
        format!("{LAYOUTS}/{}", self.filename())
    }
}

fn read(project: &Project, path: &str) -> Result<String, Error> {
    match project.read_file(path) {
        Err(Error::NotFound(_)) => Ok(String::new()),
        result => result,
    }
}

fn file_stem(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$'))
        .take(64)
        .collect();
    if name.is_empty() {
        "Layout".into()
    } else {
        name
    }
}

pub fn layouts(project: &Project) -> Result<Vec<Layout>, Error> {
    let source = read(project, LAYOUT_INDEX)?;
    let mut layouts = Vec::new();
    for (id, name) in source.lines().filter_map(|line| line.split_once('\t')) {
        let Ok(id) = id.parse() else { continue };
        if file_stem(name) != name {
            continue;
        }
        let record = Layout {
            id,
            name: name.into(),
        };
        if project.exists(&record.path())? {
            layouts.push(record);
        }
    }
    Ok(layouts)
}

fn save_layouts(project: &Project, layouts: &[Layout]) -> Result<(), Error> {
    let mut source = String::new();
    for Layout { id, name } in layouts {
        writeln!(source, "{id}\t{name}").expect("write to String");
    }
    project.write_file(LAYOUT_INDEX, &source)
}

pub fn save_layout(project: &Project, activity: &str, payload: &str) -> Result<Layout, Error> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut layouts = layouts(project)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "无法获取布局抓取时间")?
        .as_millis() as u64;
    let id = layouts
        .iter()
        .map(|record| record.id)
        .max()
        .map_or(now, |id| now.max(id.saturating_add(1)));
    let name = file_stem(activity);
    let record = Layout { id, name };
    let path = record.path();
    if project.exists(&path)? {
        return Err("同名布局文件已存在，请重新抓取".into());
    }
    layouts.insert(0, record);
    // A failed snapshot write leaves no visible row. The next index write drops it.
    save_layouts(project, &layouts)?;
    project.write_file(&path, payload)?;
    Ok(layouts.remove(0))
}

pub fn open_layout(record: &Layout) -> Result<(), Error> {
    host::open_editor(&record.path(), None)
}

pub fn layout_action(project: &Project, action: &str, id: &str) -> Result<(), Error> {
    let id = id.parse::<u64>().map_err(|_| "无效的布局记录")?;
    let mut records = layouts(project)?;
    let Some(position) = records.iter().position(|record| record.id == id) else {
        return if action == "delete-layout" {
            Ok(())
        } else {
            Err("布局记录已不存在".into())
        };
    };
    let record = &records[position];
    match action {
        "open-layout" => open_layout(record),
        "export-layout" => export_layout(project, record),
        "delete-layout" => {
            delete_layout(project, record)?;
            records.remove(position);
            save_layouts(project, &records)
        }
        _ => Err("无效的布局操作".into()),
    }
}

fn export_layout(project: &Project, record: &Layout) -> Result<(), Error> {
    let root = project.root()?;
    let Some(directory) = host::pick_directory("选择项目内的布局导出目录", Some(&root))?
    else {
        return Ok(());
    };
    let destination = std::path::Path::new(&directory)
        .join(record.filename())
        .to_string_lossy()
        .into_owned();
    if project.exists(&destination)? {
        return Err("导出目录已存在同名文件，请选择其他目录".into());
    }
    let payload = project.read_file(&record.path())?;
    project.write_file(&destination, &payload)?;
    host::notify(
        openidea_plugin::NotificationLevel::Info,
        "布局已导出",
        &destination,
    );
    Ok(())
}

fn delete_layout(project: &Project, record: &Layout) -> Result<(), Error> {
    // The SDK has no project file delete. Resolve the history directory physically
    // before unlinking one plugin-generated basename; refuse redirected directories.
    let script = r#"set -eu
cd -P -- "$1"
root=$(pwd -P)
cd -P -- "$2"
[ "$(pwd -P)" = "${root%/}/$2" ]
rm -f -- "$3"
"#;
    let args = [
        "-c".into(),
        script.into(),
        "openidea-layout-history".into(),
        project.root()?,
        LAYOUTS.into(),
        record.filename(),
    ];
    let result = project.run_process("sh", &args, None)?;
    if result.status == Some(0) {
        Ok(())
    } else {
        Err(format!("无法删除布局文件 {}", result.stderr.trim()).into())
    }
}

pub fn commands(project: &Project) -> Result<Vec<String>, Error> {
    let text = read(project, COMMANDS)?;
    Ok(text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

pub fn remember_command(project: &Project, command: &str) -> Result<(), Error> {
    let mut entries = commands(project)?;
    entries.retain(|entry| entry != command);
    entries.insert(0, command.into());
    entries.truncate(COMMAND_LIMIT);
    project.write_file(COMMANDS, &entries.join("\n"))
}

pub fn delete_command(project: &Project, command: &str) -> Result<(), Error> {
    let mut entries = commands(project)?;
    let count = entries.len();
    entries.retain(|entry| entry != command);
    if count != entries.len() {
        project.write_file(COMMANDS, &entries.join("\n"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn layout_history_reopens_snapshots_by_id_and_skips_missing_files() {
        let mock = MockHost::install().with_capabilities(&["project.read", "project.write"]);
        let project = mock.invocation().project().unwrap();
        mock.add_file(LAYOUT_INDEX, "123\tMissing\n124\t../outside\n");
        let first = save_layout(&project, "MainActivity", "first").unwrap();
        let second = save_layout(&project, "MainActivity", "second").unwrap();
        let records = layouts(&project).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, second.id);
        assert_eq!(records[1].id, first.id);
        assert_eq!(project.read_file(&records[1].path()).unwrap(), "first");
        assert_eq!(project.read_file(&records[0].path()).unwrap(), "second");
    }

    #[test]
    fn exports_a_snapshot_copy_without_overwriting_a_file() {
        let mock = MockHost::install().with_capabilities(&["project.read", "project.write"]);
        let project = mock.invocation().project().unwrap();
        let record = save_layout(&project, "MainActivity", "snapshot").unwrap();
        let destination = format!("/project/exports/{}", record.filename());
        mock.answer_next_prompt(Some(&["/project/exports"]));
        layout_action(&project, "export-layout", &record.id.to_string()).unwrap();
        assert_eq!(project.read_file(&destination).unwrap(), "snapshot");
        mock.answer_next_prompt(Some(&["/project/exports"]));
        assert!(layout_action(&project, "export-layout", &record.id.to_string()).is_err());
        assert_eq!(project.read_file(&record.path()).unwrap(), "snapshot");
    }

    #[test]
    fn adb_history_remembers_unique_commands_and_deletes_one_at_a_time() {
        let mock = MockHost::install().with_capabilities(&["project.read", "project.write"]);
        let project = mock.invocation().project().unwrap();
        for command in ["shell wm size", "shell getprop", "shell wm size"] {
            remember_command(&project, command).unwrap();
        }
        assert_eq!(
            commands(&project).unwrap(),
            ["shell wm size", "shell getprop"]
        );
        delete_command(&project, "shell wm size").unwrap();
        delete_command(&project, "shell wm size").unwrap();
        assert_eq!(commands(&project).unwrap(), ["shell getprop"]);
    }
}
