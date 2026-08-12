//! # yuzu-engine — VN 剧本运行时(剧本机)
//!
//! 将 `yuzu-scn` 解析出的 SCN 剧本编译为引擎可执行的**步骤流**:
//! 台词 / 资源命令 / 事件 / 场景跳转。渲染器(合成器 / WASM / web)
//! 按步消费并驱动对话、换背景、换立绘、语音、剧情跳转。
//!
//! 纯 Rust、无 IO,可编译为 wasm32 供浏览器调用。

use yuzu_scn::{Content, Line, Scene, Scn};

/// 引擎可消费的原子步骤。
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// 台词: (角色名, 展示文本, 语音文件引用)
    Dialogue {
        character: String,
        text: String,
        voice: Option<String>,
    },
    /// 资源命令(如 "bg:stage" / "chara:天.stand" / "voice:xxx")
    Command(String),
    /// 引擎事件(kind + 原始字段)
    Event { kind: String, fields: Vec<String> },
    /// 场景标签(锚点)
    Label(String),
    /// 场景跳转(nexts)
    Jump { storage: String, target: String },
    /// 剧本结束
    End,
}

impl Step {
    /// 是否是"需要玩家点击推进"的步骤(台词;引擎据此停等)。
    pub fn is_waiting(&self) -> bool {
        matches!(self, Step::Dialogue { .. })
    }
}

/// 剧本机:编译 + 运行。
#[derive(Debug, Clone, Default)]
pub struct ScriptMachine {
    scenes: Vec<CompiledScene>,
}

#[derive(Debug, Clone)]
struct CompiledScene {
    label: String,
    steps: Vec<Step>,
}

impl ScriptMachine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 SCN 编译全部场景(标签 -> 步骤流)。
    pub fn compile(&mut self, scn: &Scn) {
        self.scenes = scn
            .scenes
            .iter()
            .map(|s| CompiledScene {
                label: s.label.clone(),
                steps: compile_scene(s),
            })
            .collect();
    }

    pub fn scene_labels(&self) -> Vec<&str> {
        self.scenes.iter().map(|s| s.label.as_str()).collect()
    }

    /// 在指定场景从第 `start_at` 步开始一次运行。
    pub fn run(&self, scene_label: &str, start_at: usize) -> Run {
        let steps = self
            .scenes
            .iter()
            .find(|s| s.label == scene_label)
            .map(|s| s.steps.clone())
            .unwrap_or_default();
        Run {
            steps,
            index: start_at,
        }
    }
}

/// 一次运行:只进不退,按 `advance()` 推进。
#[derive(Debug, Clone, Default)]
pub struct Run {
    steps: Vec<Step>,
    index: usize,
}

impl Run {
    pub fn new(steps: Vec<Step>) -> Self {
        Run { steps, index: 0 }
    }

    /// 当前步骤(不推进)。
    pub fn current(&self) -> Option<&Step> {
        self.steps.get(self.index)
    }

    /// 推进并返回刚离开的步骤;到末尾返回 None。
    pub fn advance(&mut self) -> Option<&Step> {
        let s = self.steps.get(self.index);
        if s.is_some() {
            self.index += 1;
        }
        s
    }

    /// 下一步骤(不推进)。
    pub fn peek_next(&self) -> Option<&Step> {
        self.steps.get(self.index + 1)
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.index, self.steps.len())
    }

    pub fn is_finished(&self) -> bool {
        self.index >= self.steps.len()
    }
}

/// 把场景编译为步骤流。
fn compile_scene(scene: &Scene) -> Vec<Step> {
    let mut steps = Vec::new();
    steps.push(Step::Label(scene.label.clone()));

    for line in &scene.lines {
        match line {
            Line::Text(idx) => {
                if let Some(t) = scene.texts.get(*idx) {
                    let character = t.character.clone().unwrap_or_default();
                    let text = t
                        .dialogues
                        .first()
                        .map(|d| match &d.content {
                            Content::Plain(s) => s.clone(),
                            Content::Lang(lm) => {
                                lm.first(&["cn", "zh", "ja", "jp"]).unwrap_or_default()
                            }
                        })
                        .unwrap_or_default();
                    if !text.is_empty() {
                        let voice = t.voices.first().and_then(|v| v.voice.clone());
                        steps.push(Step::Dialogue { character, text, voice });
                    }
                }
            }
            Line::Resource(s) => steps.push(Step::Command(s.clone())),
            Line::Event(e) => steps.push(Step::Event {
                kind: e.kind.clone(),
                fields: e.fields.iter().map(|f| f.to_string()).collect(),
            }),
            // 快照点供存档/回溯,不产生渲染步骤
            Line::SnapshotPoint(_) | Line::Raw(_) => {}
        }
    }

    for n in &scene.nexts {
        steps.push(Step::Jump {
            storage: n.storage.clone(),
            target: n.target.clone(),
        });
    }
    steps.push(Step::End);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> Scn {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/c01c.txt.scn");
        let data = std::fs::read(&path).expect("读取 c01c.txt.scn");
        yuzu_scn::parse(&data).expect("解析 SCN")
    }

    #[test]
    fn compiles_real_scn_and_runs_dialogue() {
        let scn = fixture();
        let mut machine = ScriptMachine::new();
        machine.compile(&scn);

        let labels = machine.scene_labels();
        assert!(labels.contains(&"*dummy1"), "应有 *dummy1,实际 {labels:?}");

        let mut run = machine.run("*dummy1", 0);
        let mut dialogues = 0;
        while let Some(step) = run.advance() {
            match step {
                Step::Dialogue { character, text, .. } => {
                    assert!(!character.is_empty(), "角色名不应为空");
                    assert!(!text.is_empty(), "台词不应为空");
                    dialogues += 1;
                }
                Step::End => {
                    assert!(run.is_finished() || run.progress().0 <= run.progress().1);
                }
                _ => {}
            }
        }
        assert!(dialogues >= 10, "dummy1 应含较多台词,实际 {dialogues}");
        eprintln!(
            "dummy1 编译后台词数: {dialogues}, 总步数: {}",
            run.progress().1
        );
    }

    #[test]
    fn run_advances_and_ends() {
        let steps = vec![
            Step::Label("*s".into()),
            Step::Dialogue {
                character: "翔".into(),
                text: "テスト".into(),
                voice: None,
            },
            Step::End,
        ];
        let mut run = Run::new(steps);
        assert_eq!(run.current(), Some(&Step::Label("*s".into())));
        run.advance();
        assert_eq!(
            run.current(),
            Some(&Step::Dialogue {
                character: "翔".into(),
                text: "テスト".into(),
                voice: None
            })
        );
        run.advance();
        assert_eq!(run.current(), Some(&Step::End));
        run.advance();
        assert!(run.is_finished());
    }
}
