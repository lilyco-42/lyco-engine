//! # yuzu-kag — KiriKiri KAG 场景引擎
//!
//! 执行游戏的**图层 / 音频 / 流程**逻辑(复刻 KiriKiri 引擎行为),而非仅播放台词。
//! 从 `yuzu-scn` 解析的场景数据中提取 KAG 命令,驱动一个确定性的游戏状态:
//! 背景层 / 立绘层 / 消息窗 / BGM / SE / 语音 / 章节与选择流程。
//!
//! 渲染端(web canvas 等)按状态渲染;本引擎只负责**逻辑与状态**,与 UI 解耦。

use serde_json::Value;
use yuzu_scn::{Content, Line, Scene, Scn, Text};

// ---------------------------------------------------------------------------
// 引擎命令(从场景数据提取的原子操作)
// ---------------------------------------------------------------------------

/// 图层标识(千恋万花:KiriKiri 图层系统)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerId {
    /// 背景舞台
    Stage,
    /// 背景舞台 2
    Stage2,
    /// 立绘
    Character,
    /// 事件 CG
    Event,
    /// 消息窗
    MsgWin,
    /// 中心层
    CenterLayer,
    /// 其它/无名层
    Generic,
}

impl LayerId {
    fn from_str(s: &str) -> Self {
        match s {
            "stage" => LayerId::Stage,
            "stage2" => LayerId::Stage2,
            "character" => LayerId::Character,
            "event" => LayerId::Event,
            "msgwin" => LayerId::MsgWin,
            "centerlayer" => LayerId::CenterLayer,
            _ => LayerId::Generic,
        }
    }
}

/// 引擎可执行的命令。
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// 设置图层图像(背景/立绘/事件)
    SetLayerImage {
        layer: LayerId,
        name: String,
        /// 图像文件引用(如 `画面_黒`)
        file: Option<String>,
        /// 立绘表情索引 / 服装(stand options)
        face: Option<String>,
        dress: Option<String>,
        show: bool,
        /// 可见性 0-100
        opacity: Option<i32>,
        x: Option<i32>,
        y: Option<i32>,
        level: Option<i32>,
    },
    /// 隐藏图层
    HideLayer { layer: LayerId, name: String },
    /// 台词(角色 + 文本 + 语音)
    Dialogue {
        character: String,
        text: String,
        voice: Option<String>,
    },
    /// 资源命令(如 `cg_神社...` / `bgm_BGM01`)
    Resource(String),
    /// 播放 BGM
    PlayBgm { name: String },
    /// 播放语音
    PlayVoice { file: String },
    /// 章节标题
    Chapter { title: String },
    /// 场景图导航(enter/reset)
    SceneChart {
        action: String,
        target: Option<String>,
    },
    /// 选择点(分支)
    Choice {
        prompt: String,
        options: Vec<ChoiceOption>,
    },
    /// 跳转
    Jump { storage: String, target: String },
    /// 等待(毫秒)
    Wait { ms: u32 },
    /// 其它事件(原始字段)
    Event { kind: String, fields: Vec<String> },
    /// 场景结束
    End,
}

/// 一个选择选项。
#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceOption {
    pub text: String,
    pub target: String,
}

// ---------------------------------------------------------------------------
// 引擎状态(图层 + 音频 + 流程)
// ---------------------------------------------------------------------------

/// 图层图像实例。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LayerImage {
    pub name: String,
    pub file: Option<String>,
    /// 立绘表情索引(stand options.face,如 "13"=不満げ)
    pub face: Option<String>,
    /// 立绘服装(stand options.dress,如 "私服")
    pub dress: Option<String>,
    pub visible: bool,
    pub opacity: i32,
    pub x: i32,
    pub y: i32,
    pub level: i32,
}

/// 消息窗。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MessageWindow {
    pub visible: bool,
    pub text: String,
    pub speaker: String,
    pub voice: Option<String>,
}

/// 引擎完整状态。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EngineState {
    pub stage: Option<LayerImage>,
    pub stage2: Option<LayerImage>,
    pub characters: Vec<LayerImage>,
    pub event: Option<LayerImage>,
    pub message: MessageWindow,
    pub bgm: Option<String>,
    pub se: Vec<String>,
    pub chapter: Option<String>,
}

/// 命令执行结果(供渲染端消费)。
#[derive(Debug, Clone)]
pub enum Effect {
    /// 显示一句台词(渲染端更新消息窗)
    ShowDialogue {
        character: String,
        text: String,
        voice: Option<String>,
    },
    /// 图层变更
    LayerChange {
        layer: LayerId,
        name: String,
        image: Option<LayerImage>,
    },
    /// 音频
    Audio { kind: AudioKind, name: String },
    /// 章节标题
    Chapter { title: String },
    /// 等待玩家点击(台词/选择)
    WaitForClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    Bgm,
    Se,
    Voice,
}

// ---------------------------------------------------------------------------
// 引擎:场景执行器
// ---------------------------------------------------------------------------

/// KAG 场景引擎。持有图层/音频/流程状态,按命令推进。
#[derive(Debug, Clone, Default)]
pub struct KagEngine {
    pub state: EngineState,
}

impl KagEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 重置引擎状态(新游戏)。
    pub fn reset(&mut self) {
        self.state = EngineState::default();
    }

    /// 把场景数据解析为一串命令(不执行)。
    pub fn commands_for_scene(scene: &Scene) -> Vec<Command> {
        let mut cmds = Vec::new();
        for line in &scene.lines {
            match line {
                Line::Text(idx) => {
                    if let Some(t) = scene.texts.get(*idx) {
                        let (character, text, voice) = extract_dialogue(t);
                        if !text.is_empty() {
                            cmds.push(Command::Dialogue {
                                character,
                                text,
                                voice,
                            });
                        }
                    }
                }
                Line::Resource(r) => cmds.push(Command::Resource(r.clone())),
                Line::Event(e) => cmds.extend(parse_event(e.kind.as_str(), &e.fields)),
                Line::SnapshotPoint(_) | Line::Raw(_) => {}
            }
        }
        for n in &scene.nexts {
            cmds.push(Command::Jump {
                storage: n.storage.clone(),
                target: n.target.clone(),
            });
        }
        cmds.push(Command::End);
        cmds
    }

    /// 执行一条命令,更新状态并返回效果。
    pub fn exec(&mut self, cmd: &Command) -> Option<Effect> {
        match cmd {
            Command::Dialogue {
                character,
                text,
                voice,
            } => {
                self.state.message.text.clone_from(text);
                self.state.message.speaker.clone_from(character);
                self.state.message.voice.clone_from(voice);
                Some(Effect::ShowDialogue {
                    character: character.clone(),
                    text: text.clone(),
                    voice: voice.clone(),
                })
            }
            Command::Resource(r) => parse_resource_cmd(self, r),
            Command::PlayBgm { name } => {
                self.state.bgm = Some(name.clone());
                Some(Effect::Audio {
                    kind: AudioKind::Bgm,
                    name: name.clone(),
                })
            }
            Command::PlayVoice { file } => {
                self.state.message.voice = Some(file.clone());
                Some(Effect::Audio {
                    kind: AudioKind::Voice,
                    name: file.clone(),
                })
            }
            Command::Chapter { title } => {
                self.state.chapter = Some(title.clone());
                Some(Effect::Chapter {
                    title: title.clone(),
                })
            }
            Command::SetLayerImage {
                layer,
                name,
                file,
                face,
                dress,
                show,
                opacity,
                x,
                y,
                level,
            } => {
                let mut img = LayerImage {
                    name: name.clone(),
                    file: file.clone(),
                    face: face.clone(),
                    dress: dress.clone(),
                    ..Default::default()
                };
                img.visible = *show;
                if let Some(o) = opacity {
                    img.opacity = *o;
                }
                if let Some(v) = x {
                    img.x = *v;
                }
                if let Some(v) = y {
                    img.y = *v;
                }
                if let Some(v) = level {
                    img.level = *v;
                }
                set_layer(&mut self.state, *layer, img.clone());
                Some(Effect::LayerChange {
                    layer: *layer,
                    name: name.clone(),
                    image: Some(img),
                })
            }
            Command::HideLayer { layer, name } => {
                hide_layer(&mut self.state, *layer, name);
                Some(Effect::LayerChange {
                    layer: *layer,
                    name: name.clone(),
                    image: None,
                })
            }
            Command::Choice { .. } => Some(Effect::WaitForClick),
            Command::End => None,
            _ => None,
        }
    }

    /// 完整执行一个场景(返回效果序列)。
    pub fn run_scene(&mut self, scene: &Scene) -> Vec<Effect> {
        let cmds = Self::commands_for_scene(scene);
        cmds.iter().filter_map(|c| self.exec(c)).collect()
    }

    /// 保存引擎状态为 JSON(存档)。
    pub fn save(&self) -> String {
        serde_json::to_string(&self.state).unwrap_or_else(|_| "{}".into())
    }

    /// 从 JSON 恢复引擎状态(读档)。
    pub fn load(&mut self, data: &str) -> Result<(), String> {
        self.state = serde_json::from_str(data).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 从整个剧本提取文本池(供检索/翻译)。
    pub fn all_dialogues(scn: &Scn) -> Vec<(String, String)> {
        scn.scenes
            .iter()
            .flat_map(|s| {
                s.lines.iter().filter_map(|l| {
                    if let Line::Text(idx) = l {
                        s.texts.get(*idx).map(|t| {
                            let (c, txt, _) = extract_dialogue(t);
                            (c, txt)
                        })
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// 内部解析
// ---------------------------------------------------------------------------

fn extract_dialogue(t: &Text) -> (String, String, Option<String>) {
    let character = t.character.clone().unwrap_or_default();
    let text = t
        .dialogues
        .first()
        .map(|d| match &d.content {
            Content::Plain(s) => s.clone(),
            Content::Lang(lm) => lm.first(&["cn", "zh", "ja", "jp"]).unwrap_or_default(),
        })
        .unwrap_or_default();
    let voice = t.voices.first().and_then(|v| v.voice.clone());
    (character, text, voice)
}

/// 解析事件为命令。`envupdate` 携带图层操作;`chapter` 章节;`scnchart` 场景图。
fn parse_event(kind: &str, fields: &[Value]) -> Vec<Command> {
    match kind {
        "chapter" => {
            let title = fields
                .first()
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            vec![Command::Chapter { title }]
        }
        "scnchart" => {
            let action = fields
                .first()
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let target = fields
                .get(1)
                .and_then(Value::as_str)
                .map(ToString::to_string);
            vec![Command::SceneChart { action, target }]
        }
        "wait" => vec![Command::Wait { ms: 500 }],
        "envupdate" => parse_envupdate(fields),
        _ => vec![Command::Event {
            kind: kind.to_string(),
            fields: fields.iter().map(|f| f.to_string()).collect(),
        }],
    }
}

/// envupdate 的字段是字符串化的对象,包含图层操作 `[name, class, {params}]`。
/// envupdate 的字段是 token + 载荷交替:["update", <图层数组>, "revupdate", <回滚>, ...]。
/// 只处理 `update` 载荷(新状态),忽略 `revupdate`(旧值/回滚),保证渲染顺序正确。
fn parse_envupdate(fields: &[Value]) -> Vec<Command> {
    let mut cmds = Vec::new();
    let mut i = 0;
    while i < fields.len() {
        if fields[i].as_str() == Some("update") {
            if let Some(payload) = fields.get(i + 1) {
                match payload {
                    // 字符串内嵌 JSON(部分实现/其它格式)
                    Value::String(s) => cmds.extend(parse_layer_ops(s)),
                    // 真实场景:载荷是数组 Value(千恋万花)
                    other => collect_layer_ops(other, &mut cmds),
                }
            }
        }
        i += 1;
    }
    cmds
}

/// 解析 `update` 数组中的图层对象(千恋万花用对象形式 `{class,name,redraw:{imageFile:{file}}}`,
/// 而非 `["name","class",...]` 数组)。递归收集含 `class`+`name` 的图层节点。
fn parse_layer_ops(s: &str) -> Vec<Command> {
    let mut out = Vec::new();
    if let Ok(v) = serde_json::from_str::<Value>(s) {
        collect_layer_ops(&v, &mut out);
    }
    out
}

fn collect_layer_ops(v: &Value, out: &mut Vec<Command>) {
    match v {
        Value::Array(arr) => {
            for x in arr {
                collect_layer_ops(x, out);
            }
        }
        Value::Object(map) => {
            let class = map.get("class").and_then(|x| x.as_str());
            let name = map.get("name").and_then(|x| x.as_str());
            if let (Some(class), Some(name)) = (class, name) {
                let layer = LayerId::from_str(class);
                let (file, face, dress) = match map.get("redraw").and_then(|r| r.get("imageFile")) {
                    Some(imgf) => {
                        let file = imgf.get("file").and_then(|x| x.as_str());
                        let opts = imgf.get("options");
                        let face = opts
                            .and_then(|o| o.get("face"))
                            .and_then(|x| x.as_str())
                            .map(str::to_string);
                        let dress = opts
                            .and_then(|o| o.get("dress"))
                            .and_then(|x| x.as_str())
                            .map(str::to_string);
                        (file.map(str::to_string), face, dress)
                    }
                    None => (None, None, None),
                };
                let showmode = map.get("showmode").and_then(|x| x.as_u64()).unwrap_or(1);
                let show = showmode != 2 && showmode != 0;
                out.push(Command::SetLayerImage {
                    layer,
                    name: name.to_string(),
                    file,
                    face,
                    dress,
                    show,
                    opacity: None,
                    x: None,
                    y: None,
                    level: None,
                });
            }
            for val in map.values() {
                collect_layer_ops(val, out);
            }
        }
        _ => {}
    }
}

/// 解析资源命令 `cg_XXX` / `bgm_XXX` / `voice:XXX`。
fn parse_resource_cmd(engine: &mut KagEngine, r: &str) -> Option<Effect> {
    if let Some(bgm) = r.strip_prefix("bgm_") {
        engine.state.bgm = Some(bgm.to_string());
        return Some(Effect::Audio {
            kind: AudioKind::Bgm,
            name: bgm.to_string(),
        });
    }
    if let Some(v) = r.strip_prefix("voice:") {
        engine.state.message.voice = Some(v.to_string());
        return Some(Effect::Audio {
            kind: AudioKind::Voice,
            name: v.to_string(),
        });
    }
    if let Some(cg) = r.strip_prefix("cg_") {
        // 背景/立绘命令:含 .stand 为立绘,否则背景
        let layer = if cg.ends_with(".stand") {
            LayerId::Character
        } else {
            LayerId::Stage
        };
        let name = cg.to_string();
        set_layer(
            &mut engine.state,
            layer,
            LayerImage {
                name: name.clone(),
                visible: true,
                ..Default::default()
            },
        );
        return Some(Effect::LayerChange {
            layer,
            name,
            image: None,
        });
    }
    None
}

fn set_layer(state: &mut EngineState, layer: LayerId, img: LayerImage) {
    match layer {
        LayerId::Stage => state.stage = Some(img),
        LayerId::Stage2 => state.stage2 = Some(img),
        LayerId::Event => state.event = Some(img),
        LayerId::Character => {
            if let Some(existing) = state.characters.iter_mut().find(|c| c.name == img.name) {
                *existing = img;
            } else {
                state.characters.push(img);
            }
        }
        LayerId::MsgWin => {
            state.message.visible = img.visible;
        }
        LayerId::CenterLayer | LayerId::Generic => {}
    }
}

fn hide_layer(state: &mut EngineState, layer: LayerId, name: &str) {
    match layer {
        LayerId::Stage => state.stage = None,
        LayerId::Stage2 => state.stage2 = None,
        LayerId::Event => state.event = None,
        LayerId::Character => state.characters.retain(|c| c.name != name),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scn() -> Scn {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/c01c.txt.scn");
        let data = std::fs::read(&path).expect("c01c.txt.scn");
        yuzu_scn::parse(&data).expect("解析 SCN")
    }

    fn real_scn() -> Scn {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../realgame/scn001.scn");
        let data = std::fs::read(&path).expect("scn001");
        yuzu_scn::parse(&data).expect("解析")
    }

    #[test]
    fn commands_from_fixture() {
        let scn = scn();
        let scene = scn.scene("*dummy1").expect("场景");
        let cmds = KagEngine::commands_for_scene(scene);
        assert!(!cmds.is_empty());
        assert!(
            cmds.iter().any(|c| matches!(c, Command::Dialogue { .. })),
            "应含台词"
        );
        assert!(cmds.last() == Some(&Command::End));
    }

    #[test]
    fn run_real_scene_produces_dialogues() {
        let scn = real_scn();
        let mut eng = KagEngine::new();
        let scene = scn.scene("*001_01com").expect("主场景");
        let effects = eng.run_scene(scene);
        let dlg = effects
            .iter()
            .filter(|e| matches!(e, Effect::ShowDialogue { .. }))
            .count();
        assert!(dlg > 100, "真实场景应含大量台词,实际 {dlg}");
    }

    #[test]
    fn chapter_scene_sets_title() {
        let scn = real_scn();
        let scene = scn.scene("*com_part_1:2").expect("章节场景");
        let mut eng = KagEngine::new();
        let effects = eng.run_scene(scene);
        let chapter = effects.iter().find_map(|e| match e {
            Effect::Chapter { title } => Some(title.clone()),
            _ => None,
        });
        assert!(chapter.is_some(), "应触发章节标题");
    }

    #[test]
    fn envupdate_object_format_parses_stage() {
        // 千恋万花 envupdate 的 update 载荷是对象数组 {class,name,redraw:{imageFile:{file}}}
        let update = "[{\"class\":\"stage\",\"name\":\"stage\",\"showmode\":3,\
            \"redraw\":{\"disp\":2,\"imageFile\":{\"file\":\"空_青空\"},\"posName\":null},\
            \"action\":[null,[\"leveloffset\",\"\"]],\"trans\":{\"method\":\"universal\",\"time\":1500}}]";
        let fields = vec![
            Value::String("update".into()),
            Value::String(update.into()),
            Value::String("revupdate".into()),
            Value::String("[{\"class\":\"stage\",\"name\":\"stage\",\"showmode\":3,\
                \"redraw\":{\"imageFile\":{\"file\":\"画面_黒\"}}}]".into()),
        ];
        let cmds = parse_envupdate(&fields);
        let layers: Vec<_> = cmds
            .iter()
            .filter_map(|c| match c {
                Command::SetLayerImage {
                    layer, name, file, ..
                } => Some((*layer, name.clone(), file.clone())),
                _ => None,
            })
            .collect();
        eprintln!("DBG layers = {layers:?}");
        assert_eq!(layers.len(), 1, "只处理 update,忽略 revupdate");
        assert_eq!(layers[0].0, LayerId::Stage);
        assert_eq!(layers[0].1, "stage");
        assert_eq!(layers[0].2.as_deref(), Some("空_青空"));
    }

    #[test]
    fn envupdate_sets_layers() {
        let scn = real_scn();
        let mut eng = KagEngine::new();
        let scene = scn.scene("*com_part_1:2").expect("章节场景");
        let effects = eng.run_scene(scene);
        let layers = effects
            .iter()
            .filter(|e| matches!(e, Effect::LayerChange { .. }))
            .count();
        assert!(layers > 0, "应有图层变更");
        // 台词应含真实文本
        let dlg = effects.iter().find_map(|e| match e {
            Effect::ShowDialogue { text, .. } if !text.is_empty() => Some(text.clone()),
            _ => None,
        });
        assert!(dlg.is_some(), "应有台词");
    }

    #[test]
    fn resource_bgm() {
        let mut eng = KagEngine::new();
        let e = eng.exec(&Command::Resource("bgm_BGM01".into())).unwrap();
        assert!(matches!(
            e,
            Effect::Audio {
                kind: AudioKind::Bgm,
                ..
            }
        ));
        assert_eq!(eng.state.bgm.as_deref(), Some("BGM01"));
    }

    #[test]
    fn dialogue_sets_message() {
        let mut eng = KagEngine::new();
        eng.exec(&Command::Dialogue {
            character: "将臣".into(),
            text: "テスト".into(),
            voice: Some("uts001_001".into()),
        });
        assert_eq!(eng.state.message.speaker, "将臣");
        assert_eq!(eng.state.message.text, "テスト");
        assert_eq!(eng.state.message.voice.as_deref(), Some("uts001_001"));
    }

    #[test]
    fn all_dialogues_extracts_texts() {
        let scn = real_scn();
        let ds = KagEngine::all_dialogues(&scn);
        assert!(ds.len() > 200, "应有大量台词,实际 {}", ds.len());
    }
    #[test]
    fn save_load_roundtrip() {
        let mut eng = KagEngine::new();
        eng.exec(&Command::Dialogue { character: "将臣".into(), text: "テスト".into(), voice: None });
        eng.exec(&Command::Resource("bgm_BGM01".into()));
        eng.state.chapter = Some("1-1".into());
        let saved = eng.save();
        let mut eng2 = KagEngine::new();
        eng2.load(&saved).expect("读档");
        assert_eq!(eng2.state.message.speaker, "将臣");
        assert_eq!(eng2.state.bgm.as_deref(), Some("BGM01"));
        assert_eq!(eng2.state.chapter.as_deref(), Some("1-1"));
    }
}
