use glaux_docs::Library;
use glaux_sense::Insertion;
use pretty_assertions::assert_eq;

use super::*;

fn api_candidate(id: &str, label: &str, kind: CandidateKind) -> Candidate {
    Candidate {
        label: label.to_owned(),
        kind,
        origin: glaux_sense::Origin::Api,
        insertion: Insertion::Name(label.to_owned()),
        detail: String::new(),
        doc: Doc::Api(DocId::parse(id).unwrap()),
    }
}

fn panel(candidate: &Candidate, language: ScriptLanguage) -> Option<DocPanel> {
    let summary = |id: &DocId| Some(format!("About {id}."));
    let line_of = |byte: usize| byte / 10 + 1;
    panel_for(
        candidate,
        &PanelSource {
            language,
            api: Library::embedded().api(),
            summary: &summary,
            line_of: &line_of,
        },
    )
}

fn signature(panel: &DocPanel) -> String {
    panel
        .signature
        .iter()
        .map(|(piece, _)| piece.as_str())
        .collect()
}

#[test]
fn a_function_shows_its_call_and_the_type_of_each_parameter() {
    let candidate = api_candidate("api/graphics/rectfill", "rectfill", CandidateKind::Function);
    let panel = panel(&candidate, ScriptLanguage::Luau).unwrap();
    assert_eq!(signature(&panel), "rectfill(x, y, width, height, color)");
    assert_eq!(
        panel.params.as_deref(),
        Some("x: number  y: number  width: int  height: int  color: int")
    );
    assert_eq!(
        panel.summary.as_deref(),
        Some("About api/graphics/rectfill.")
    );
    assert_eq!(panel.signature[0].1, SyntaxKind::Function);
    assert_eq!(panel.signature[2].1, SyntaxKind::Variable);
}

#[test]
fn a_member_function_is_named_with_its_namespace_in_the_language_asked() {
    let candidate = api_candidate("api/audio/tone", "tone", CandidateKind::Function);
    let luau = panel(&candidate, ScriptLanguage::Luau).unwrap();
    let rhai = panel(&candidate, ScriptLanguage::Rhai).unwrap();
    assert!(signature(&luau).starts_with("audio.tone("));
    assert!(signature(&rhai).starts_with("audio::tone("));
}

#[test]
fn a_constant_names_its_group_and_a_group_names_itself() {
    let member = api_candidate("api/input/keys", "Space", CandidateKind::Constant);
    assert_eq!(
        signature(&panel(&member, ScriptLanguage::Luau).unwrap()),
        "Key.Space"
    );
    assert_eq!(
        signature(&panel(&member, ScriptLanguage::Rhai).unwrap()),
        "Key::Space"
    );
    let group = api_candidate("api/input/keys", "Key", CandidateKind::Constant);
    assert_eq!(
        signature(&panel(&group, ScriptLanguage::Luau).unwrap()),
        "Key"
    );
}

#[test]
fn a_callback_previews_its_declaration_in_the_languages_syntax() {
    let candidate = api_candidate("api/lifecycle/tick", "tick", CandidateKind::Callback);
    let luau = panel(&candidate, ScriptLanguage::Luau).unwrap();
    let rhai = panel(&candidate, ScriptLanguage::Rhai).unwrap();
    assert_eq!(signature(&luau), "function tick()");
    assert_eq!(signature(&rhai), "fn tick()");
}

#[test]
fn locals_say_what_they_are_and_where_they_were_declared() {
    let local = |kind, name: &str, params: &[&str]| Candidate {
        label: name.to_owned(),
        kind: CandidateKind::Variable,
        origin: glaux_sense::Origin::Local,
        insertion: Insertion::Name(name.to_owned()),
        detail: String::new(),
        doc: Doc::Local(Definition {
            name: name.to_owned(),
            kind,
            declared: 25..26,
            visible_from: 30,
            params: params.iter().map(|&p| p.to_owned()).collect(),
        }),
    };
    let variable = panel(
        &local(DefinitionKind::Variable, "speed", &[]),
        ScriptLanguage::Luau,
    )
    .unwrap();
    assert_eq!(signature(&variable), "speed");
    assert_eq!(
        variable.summary.as_deref(),
        Some("Local variable declared on line 3.")
    );
    let parameter = panel(
        &local(DefinitionKind::Parameter, "dt", &[]),
        ScriptLanguage::Luau,
    )
    .unwrap();
    assert_eq!(
        parameter.summary.as_deref(),
        Some("Parameter declared on line 3.")
    );
    let function = panel(
        &local(DefinitionKind::Function, "step", &["a", "b"]),
        ScriptLanguage::Luau,
    )
    .unwrap();
    assert_eq!(signature(&function), "step(a, b)");
}

#[test]
fn a_keyword_has_no_panel() {
    let keyword = Candidate {
        label: "local".to_owned(),
        kind: CandidateKind::Keyword,
        origin: glaux_sense::Origin::Keyword,
        insertion: Insertion::Name("local".to_owned()),
        detail: "keyword".to_owned(),
        doc: Doc::None,
    };
    assert_eq!(panel(&keyword, ScriptLanguage::Luau), None);
}
