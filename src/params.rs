//! 参数收集与 `{{模板}}` 表达式解析。
//!
//! 内置变量 `name` = 项目名。`{{key|filter1|filter2}}` 先取 key 的值，再从左到右应用过滤器。
//! 支持过滤器：path / rust / parent / pascal / camel / snake / kebab / upper / lower / space。

use crate::manifest::TemplateManifest;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};

/// 内置变量名：项目名
pub const NAME: &str = "name";

/// 收集参数值。优先级：`--param` 覆盖 > 交互提示（TTY 且未 `--yes`）> 默认值。
pub fn collect(
    manifest: &TemplateManifest,
    name: &str,
    overrides: &[(String, String)],
    yes: bool,
) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    values.insert(NAME.to_string(), name.to_string());

    let interactive = !yes && io::stdin().is_terminal();

    for p in &manifest.params {
        let default = resolve(&p.default, &values);
        let mut value = match overrides.iter().find(|(k, _)| k == &p.key) {
            Some((_, v)) => v.clone(),
            None if interactive => prompt(&p.label, &default, &p.help)?,
            None => default,
        };
        // 覆盖值 / 交互值本身也可能是模板（可引用 name 与更早定义的参数）
        value = resolve(&value, &values);
        if value.is_empty() && p.required {
            bail!("参数 {} 不能为空", p.key);
        }
        values.insert(p.key.clone(), value);
    }
    Ok(values)
}

/// 把输入中的 `{{key|filter}}` 全部解析；未知 key 保留字面量。
pub fn resolve(input: &str, values: &HashMap<String, String>) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' && chars.get(i + 1) == Some(&'{') {
            let mut j = i + 2;
            while j + 1 < chars.len() && !(chars[j] == '}' && chars[j + 1] == '}') {
                j += 1;
            }
            if j + 1 < chars.len() {
                let expr: String = chars[i + 2..j].iter().collect();
                let parts: Vec<&str> = expr.split('|').map(str::trim).collect();
                if let Some(val) = values.get(parts[0]) {
                    let mut v = val.clone();
                    for f in &parts[1..] {
                        v = apply_filter(&v, f);
                    }
                    out.push_str(&v);
                    i = j + 2;
                    continue;
                }
                // 未知 key → 保留字面量 {{...}}
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 应用单个过滤器。
pub fn apply_filter(v: &str, f: &str) -> String {
    match f {
        "path" => v.replace('.', "/"),
        "rust" => v.replace('.', "_"),
        "parent" => match v.rfind('.') {
            Some(i) => v[..i].to_string(),
            None => v.to_string(),
        },
        "pascal" => to_pascal(v),
        "camel" => to_camel(v),
        "snake" => to_snake(v),
        "kebab" => to_kebab(v),
        "upper" => v.to_uppercase(),
        "lower" => v.to_lowercase(),
        "space" => v.replace(['_', '-'], " "),
        _ => v.to_string(),
    }
}

/// 按分隔符与驼峰边界切词。
fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.push(c);
            prev_lower = c.is_lowercase();
        } else {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            prev_lower = false;
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn to_pascal(s: &str) -> String {
    split_words(s)
        .iter()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn to_camel(s: &str) -> String {
    let p = to_pascal(s);
    let mut chars = p.chars();
    match chars.next() {
        Some(f) => f.to_lowercase().collect::<String>() + chars.as_str(),
        None => p,
    }
}

fn to_snake(s: &str) -> String {
    split_words(s)
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

fn to_kebab(s: &str) -> String {
    split_words(s)
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// 简单交互提示：输出 `label [default]: `，空输入取默认值。
fn prompt(label: &str, default: &str, help: &str) -> io::Result<String> {
    if !help.is_empty() {
        eprintln!("  · {help}");
    }
    let msg = if default.is_empty() {
        format!("{label}: ")
    } else {
        format!("{label} [{default}]: ")
    };
    print!("{msg}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let v = line.trim().to_string();
    Ok(if v.is_empty() { default.to_string() } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("name".into(), "auto-sprint".into());
        m.insert("package".into(), "com.example.myapp".into());
        m
    }

    #[test]
    fn resolve_simple() {
        assert_eq!(resolve("{{name}}", &vals()), "auto-sprint");
    }

    #[test]
    fn resolve_unknown_key_kept_literal() {
        assert_eq!(resolve("{{nope}}", &vals()), "{{nope}}");
    }

    #[test]
    fn filters_on_package() {
        let v = vals();
        assert_eq!(resolve("{{package|path}}", &v), "com/example/myapp");
        assert_eq!(resolve("{{package|rust}}", &v), "com_example_myapp");
        assert_eq!(resolve("{{package|parent}}", &v), "com.example");
    }

    #[test]
    fn filters_on_name() {
        let v = vals();
        assert_eq!(resolve("{{name|pascal}}", &v), "AutoSprint");
        assert_eq!(resolve("{{name|snake}}", &v), "auto_sprint");
        assert_eq!(resolve("{{name|kebab}}", &v), "auto-sprint");
        assert_eq!(resolve("{{name|space}}", &v), "auto sprint");
    }

    #[test]
    fn pascal_preserves_existing_case() {
        assert_eq!(apply_filter("AutoSprint", "pascal"), "AutoSprint");
        assert_eq!(apply_filter("my_app", "pascal"), "MyApp");
    }

    #[test]
    fn collect_uses_defaults_non_interactive() {
        let m = TemplateManifest {
            id: "t".into(),
            params: vec![crate::manifest::Param {
                key: "package".into(),
                label: "包名".into(),
                default: "com.{{name|snake}}".into(),
                help: String::new(),
                required: false,
            }],
            ..Default::default()
        };
        let values = collect(&m, "hello-world", &[], true).unwrap();
        assert_eq!(values.get("package").unwrap(), "com.hello_world");
    }

    #[test]
    fn override_beats_default() {
        let m = TemplateManifest {
            id: "t".into(),
            params: vec![crate::manifest::Param {
                key: "package".into(),
                label: "包名".into(),
                default: "com.example".into(),
                help: String::new(),
                required: false,
            }],
            ..Default::default()
        };
        let values = collect(&m, "x", &[("package".into(), "com.override".into())], true).unwrap();
        assert_eq!(values.get("package").unwrap(), "com.override");
    }
}
