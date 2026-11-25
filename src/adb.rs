use crate::history;
use openidea_plugin::{
    Error, ProcessOutput,
    host::{self, Project},
    text::escape_json,
};
use std::fmt::Write;

fn arguments(line: &str) -> Result<Vec<String>, Error> {
    if line.len() > 4096 || line.contains(['\r', '\n', '\0']) {
        return Err("请输入一行有效的 ADB 命令".into());
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut in_argument = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if quote != Some('\'') => {
                current.push(chars.next().ok_or("ADB 命令末尾有未完成的转义")?);
                in_argument = true;
            }
            '\'' | '"' if quote.is_none() => {
                quote = Some(c);
                in_argument = true;
            }
            c if quote == Some(c) => quote = None,
            c if quote.is_none() && c.is_whitespace() => {
                if in_argument {
                    args.push(std::mem::take(&mut current));
                    in_argument = false;
                }
            }
            _ => {
                current.push(c);
                in_argument = true;
            }
        }
    }
    if quote.is_some() {
        return Err("ADB 命令有未闭合的引号".into());
    }
    if in_argument {
        args.push(current);
    }
    match args.first() {
        None => Err("请输入 ADB 命令".into()),
        Some(command) if command == "adb" => Err("请输入 ADB 子命令".into()),
        Some(command) if command.is_empty() || command.starts_with('-') => {
            Err("请从 ADB 子命令开始输入，设备由上方选择器指定".into())
        }
        _ => Ok(args),
    }
}

pub fn execute(project: &Project, serial: &str, line: &str) -> Result<ProcessOutput, Error> {
    let line = line.trim();
    let line = line
        .strip_prefix("adb")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map(str::trim_start)
        .unwrap_or(line);
    let args = arguments(line)?;
    if serial.is_empty() {
        return Err("请先选择在线 Android 设备".into());
    }
    let args = ["-s".into(), serial.into()]
        .into_iter()
        .chain(args)
        .collect::<Vec<_>>();
    history::remember_command(project, line)?;
    project.run_process("adb", &args, None)
}

pub fn message(result: Result<ProcessOutput, Error>) -> String {
    let mut json = String::new();
    match result {
        Ok(output) => {
            let status = output.status.map_or("null".into(), |code| code.to_string());
            write!(
                json,
                r#"{{"status":{status},"stdout":"{}","stderr":"{}"}}"#,
                escape_json(&output.stdout),
                escape_json(&output.stderr)
            )
            .expect("write to String");
        }
        Err(error) => {
            write!(json, r#"{{"error":"{}"}}"#, escape_json(&error.to_string()))
                .expect("write to String");
        }
    }
    json
}

pub fn copy_result(output: &str) -> Result<(), Error> {
    // OpenIDEA 0.1.51 clips the host clipboard payload at 1 MiB.
    if output.len() > 1 << 20 {
        return Err("输出超过剪贴板接口的 1 MiB 限制，可选中文本分段复制。".into());
    }
    host::copy_to_clipboard(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_arguments_without_host_shell_expansion() {
        assert_eq!(
            arguments(r#"shell "echo 'a b'" '$HOME' "" ;"#).unwrap(),
            ["shell", "echo 'a b'", "$HOME", "", ";"]
        );
        assert_eq!(
            arguments(r"push file\ name /data/local/tmp/").unwrap(),
            ["push", "file name", "/data/local/tmp/"]
        );
    }

    #[test]
    fn rejects_a_device_override_or_malformed_input() {
        for input in [
            "",
            "adb",
            "-s other shell",
            "-H server devices",
            "shell 'unterminated",
            "shell\ngetprop",
        ] {
            assert!(arguments(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn uses_the_selected_device_and_saves_the_command_without_the_adb_prefix() {
        let mock = openidea_plugin::mock::MockHost::install().with_capabilities(&[
            "project.read",
            "project.write",
            "process:adb",
        ]);
        let project = mock.invocation().project().unwrap();
        mock.on_process("adb", |args, stdin| {
            assert_eq!(args, ["-s", "device-1", "shell", "wm", "size"]);
            assert_eq!(stdin, None);
            Ok(ProcessOutput {
                status: Some(0),
                stdout: "Physical size: 1920x1080".into(),
                stderr: String::new(),
            })
        });
        execute(&project, "device-1", "adb  shell wm size").unwrap();
        assert!(execute(&project, "device-1", "adb -s another shell").is_err());
        assert_eq!(history::commands(&project).unwrap(), ["shell wm size"]);
        assert_eq!(mock.process_runs().len(), 1);
    }

    #[test]
    fn copies_exact_output_and_refuses_to_clip_large_results() {
        let mock = openidea_plugin::mock::MockHost::install().with_capabilities(&["clipboard"]);
        let output = "第一行\nSecond line\n";
        copy_result(output).unwrap();
        assert_eq!(mock.clipboard().as_deref(), Some(output));
        assert!(copy_result(&"x".repeat((1 << 20) + 1)).is_err());
        assert_eq!(mock.clipboard().as_deref(), Some(output));
    }
}
