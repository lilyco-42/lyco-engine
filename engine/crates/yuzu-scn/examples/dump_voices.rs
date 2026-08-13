//! 打印剧本每句台词的语音引用(voice refs)。用法: cargo run -p yuzu-scn --example dump_voices -- <file.scn> [角色名关键字]
use yuzu_scn::{parse, Content, Line, Scn};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut json = false;
    let mut file = None;
    let mut kw = String::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--kw" => { i += 1; kw = args.get(i).cloned().unwrap_or_default(); }
            a if a.starts_with('-') => { eprintln!("unknown flag {a}"); std::process::exit(1); }
            a => {
                if file.is_none() { file = Some(a.to_string()); } else { kw = a.to_string(); }
            }
        }
        i += 1;
    }
    let file = file.expect("usage: dump_voices <file.scn> [character-keyword] [--json] [--kw X]");
    let data = std::fs::read(&file).expect("read scn");
    let scn: Scn = parse(&data).map_err(|e| e).expect("parse scn");

    let mut voice_prefixes: std::collections::BTreeMap<String, usize> = Default::default();
    for s in &scn.scenes {
        for line in &s.lines {
            if let Line::Text(idx) = line {
                if let Some(t) = s.texts.get(*idx) {
                    let who = t.character.as_deref().unwrap_or("?");
                    let hit = kw.is_empty() || who.contains(&kw);
                    if !hit {
                        continue;
                    }
                    let text = t
                        .dialogues
                        .iter()
                        .map(|d| match &d.content {
                            Content::Plain(x) => x.clone(),
                            Content::Lang(lm) => lm.first(&["cn"]).unwrap_or_default(),
                        })
                        .collect::<Vec<_>>()
                        .join(" / ");
                    let vrefs: Vec<String> = t
                        .voices
                        .iter()
                        .filter_map(|v| v.voice.clone())
                        .collect();
                    if vrefs.is_empty() {
                        continue;
                    }
                    for v in &vrefs {
                        let prefix = v.split('_').next().unwrap_or(v).to_string();
                        *voice_prefixes.entry(prefix).or_default() += 1;
                    }
                    if json {
                        println!("{}", serde_json::json!({"who": who, "voices": vrefs, "text": text}));
                    } else {
                        println!("{} | {:?} | {}", who, vrefs, text);
                    }
                }
            }
        }
    }
    if !json {
        println!("\n=== voice 前缀统计 ===");
        for (k, n) in &voice_prefixes {
            println!("{k}: {n}");
        }
    }
}
