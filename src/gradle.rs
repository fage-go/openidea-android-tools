use openidea_plugin::Error;
use openidea_plugin::host::{self, Project};
use std::collections::BTreeMap;

// A missing variant list means the cache was produced by an earlier version.
pub type Modules = BTreeMap<String, Option<Vec<String>>>;

const IDE_SETTINGS: &str = ".openidea/settings.toml";
const MODULE_CACHE: &str = ".gradle/openidea-android-tools.modules";
const VARIANT_CACHE: &str = ".gradle/openidea-android-tools.variants";
const DISCOVERY: &str = concat!(
    include_str!("gradle-env.sh"),
    "\n",
    include_str!("discover.sh")
);
pub const LAUNCH: &str = concat!(
    include_str!("gradle-env.sh"),
    "\n",
    include_str!("launch.sh")
);

pub fn is_project(project: &Project) -> bool {
    [
        "settings.gradle",
        "settings.gradle.kts",
        "build.gradle",
        "build.gradle.kts",
    ]
    .iter()
    .any(|path| project.read_file(path).is_ok())
}

pub fn jvm(project: &Project) -> Result<String, Error> {
    match project.read_file(IDE_SETTINGS) {
        Ok(source) => project_jvm(&source),
        Err(Error::NotFound(_)) => Ok(String::new()),
        Err(error) => Err(error),
    }
}

fn project_jvm(source: &str) -> Result<String, Error> {
    let mut gradle_section = false;
    for line in source.lines().map(str::trim) {
        if line.starts_with('[') {
            gradle_section =
                line.split('#').next().unwrap_or_default().trim() == "[languages.java.gradle]";
        } else if gradle_section
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "jvm"
        {
            return setting_path(value.trim())
                .ok_or_else(|| "无法读取项目设置中的 Gradle JVM 路径".into());
        }
    }
    Ok(String::new())
}

fn setting_path(value: &str) -> Option<String> {
    let mut chars = value.chars();
    let quote = chars.next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }
    let mut path = String::new();
    while let Some(c) = chars.next() {
        match c {
            c if c == quote => {
                let rest = chars.as_str().trim();
                return (rest.is_empty() || rest.starts_with('#')).then_some(path);
            }
            '\\' if quote == '"' => match chars.next()? {
                c @ ('\\' | '"') => path.push(c),
                _ => return None,
            },
            c if c.is_control() => return None,
            c => path.push(c),
        }
    }
    None
}

pub fn valid_variant(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub fn modules(project: &Project) -> Option<Modules> {
    project
        .read_file(VARIANT_CACHE)
        .or_else(|_| project.read_file(MODULE_CACHE))
        .ok()
        .map(|source| parse_modules(&source))
}

fn parse_modules(source: &str) -> Modules {
    source
        .lines()
        .filter_map(|line| {
            let (module, variants) = match line.split_once('\t') {
                Some((module, names)) => {
                    let mut variants: Vec<_> = names
                        .split('\t')
                        .filter(|name| valid_variant(name))
                        .map(str::to_owned)
                        .collect();
                    variants.sort();
                    variants.dedup();
                    (module, Some(variants))
                }
                None => (line, None),
            };
            (module.starts_with(':') && !module.contains(['\r', '\0']))
                .then(|| (module.to_owned(), variants))
        })
        .collect()
}

pub fn refresh(project: &Project) -> Result<Modules, Error> {
    let jvm = jvm(project)?;
    let _task = host::begin_task("识别 Android 应用模块和构建变体", true);
    let args = [
        "-c",
        DISCOVERY,
        "openidea-android-discover",
        &jvm,
        include_str!("application-modules.gradle"),
        VARIANT_CACHE,
    ];
    let output = project.run_process("sh", &args.map(str::to_owned), None)?;
    if output.status != Some(0) {
        let detail = if output.stderr.trim().is_empty() {
            &output.stdout
        } else {
            &output.stderr
        };
        let lines: Vec<_> = detail.lines().rev().take(18).collect();
        let message = lines.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(format!("Gradle 模块识别失败\n{message}").into());
    }
    modules(project).ok_or_else(|| "无法读取 Gradle 应用模块列表".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_variant_metadata_and_previous_module_cache() {
        let modules = parse_modules(":app\tpaidDebug\tfreeRelease\tpaidDebug\n:empty\t\n:legacy\n");
        assert_eq!(
            modules[":app"].as_deref().unwrap(),
            ["freeRelease", "paidDebug"]
        );
        assert_eq!(modules[":empty"], Some(Vec::new()));
        assert_eq!(modules[":legacy"], None);
    }

    #[test]
    fn reads_only_the_project_gradle_jvm() {
        let source = concat!(
            "[languages.java]\njvm = '/wrong'\n",
            "[languages.java.gradle] # current\njvm = \"/a JDK/#8\" # path\n",
        );
        assert_eq!(project_jvm(source).unwrap(), "/a JDK/#8");
        assert_eq!(project_jvm("[other]\njvm = '/wrong'\n").unwrap(), "");
        assert_eq!(
            project_jvm("[languages.java.gradle]\njvm = '~/.jdks/8'").unwrap(),
            "~/.jdks/8"
        );
        assert!(project_jvm("[languages.java.gradle]\njvm = 8").is_err());
    }
}
