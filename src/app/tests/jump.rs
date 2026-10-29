use super::*;
use ratatui::{Terminal, backend::TestBackend, style::Modifier};

/// Codes: `xk2lp`, `wq9zt`, and the whole web name, so `w` is shared.
const NAMES: [&str; 3] = [
    "api-7d8f9b5c4-xk2lp",
    "api-7d8f9b5c4-wq9zt",
    "web-5f6c7d8d9-r2kpl",
];

fn pod(name: &str) -> serde_json::Value {
    json!({"apiVersion": "v1", "kind": "Pod",
           "metadata": {"name": name, "namespace": "default"}})
}

fn jump_app() -> (App, Receiver<Msg>) {
    let (mut app, rx) = test_app();
    app.switch_kind("pods");
    for name in NAMES {
        apply(&mut app, pod(name));
    }
    app.table_state.select(Some(0));
    (app, rx)
}

fn typing(app: &mut App, keys: &str) {
    for c in keys.chars() {
        app.handle_key(press(KeyCode::Char(c))).unwrap();
    }
}

fn start(app: &mut App) {
    app.handle_key(press(KeyCode::Char('v'))).unwrap();
    assert_eq!(app.mode, Mode::Jump);
}

#[tokio::test]
async fn v_starts_a_jump_by_default() {
    let (mut app, _rx) = jump_app();
    assert_eq!(app.keymap.label("table", Action::Jump), "v");
    start(&mut app);
}

fn selected_name(app: &App) -> String {
    app.selected_ref()
        .and_then(|o| o.metadata.name.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn typing_a_code_moves_the_cursor_to_its_row() {
    let (mut app, _rx) = jump_app();
    start(&mut app);
    typing(&mut app, "w");
    assert_eq!(app.mode, Mode::Jump, "w is shared by two codes");
    typing(&mut app, "q");
    assert_eq!(app.mode, Mode::Table);
    assert_eq!(selected_name(&app), "api-7d8f9b5c4-wq9zt");

    start(&mut app);
    typing(&mut app, "we");
    assert_eq!(app.mode, Mode::Table);
    assert_eq!(selected_name(&app), "web-5f6c7d8d9-r2kpl");
}

#[tokio::test]
async fn letters_are_codes_not_navigation_while_jumping() {
    let (mut app, _rx) = jump_app();
    let before = selected_name(&app);
    start(&mut app);
    typing(&mut app, "jk");
    assert_eq!(app.mode, Mode::Jump, "no code starts with j or k");
    assert_eq!(selected_name(&app), before);
    typing(&mut app, "x");
    assert_eq!(selected_name(&app), "api-7d8f9b5c4-xk2lp");
}

#[tokio::test]
async fn escape_cancels_without_moving() {
    let (mut app, _rx) = jump_app();
    let before = selected_name(&app);
    start(&mut app);
    typing(&mut app, "w");
    app.handle_key(press(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Table);
    assert_eq!(selected_name(&app), before);
}

#[tokio::test]
async fn enter_takes_the_first_remaining_row() {
    let (mut app, _rx) = jump_app();
    let first_w = app
        .rows()
        .iter()
        .filter_map(|o| o.metadata.name.clone())
        .find(|n| n.ends_with("-wq9zt") || n.starts_with("web-"))
        .unwrap();
    start(&mut app);
    typing(&mut app, "w");
    app.handle_key(press(KeyCode::Enter)).unwrap();
    assert_eq!(app.mode, Mode::Table);
    assert_eq!(selected_name(&app), first_w);
}

#[tokio::test]
async fn backspace_takes_back_a_typed_character() {
    let (mut app, _rx) = jump_app();
    start(&mut app);
    typing(&mut app, "w");
    app.handle_key(press(KeyCode::Backspace)).unwrap();
    typing(&mut app, "x");
    assert_eq!(app.mode, Mode::Table);
    assert_eq!(selected_name(&app), "api-7d8f9b5c4-xk2lp");
}

#[tokio::test]
async fn uppercase_input_matches_lowercase_names() {
    let (mut app, _rx) = jump_app();
    start(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT))
        .unwrap();
    assert_eq!(selected_name(&app), "api-7d8f9b5c4-xk2lp");
}

#[tokio::test]
async fn codes_stay_with_their_object_when_rows_shift() {
    let (mut app, _rx) = jump_app();
    start(&mut app);
    // A new pod sorts in above the others while the user is typing.
    apply(&mut app, pod("api-7d8f9b5c4-aaaaa"));
    typing(&mut app, "x");
    assert_eq!(selected_name(&app), "api-7d8f9b5c4-xk2lp");
}

#[tokio::test]
async fn a_row_that_disappeared_is_reported_not_jumped_to() {
    let (mut app, _rx) = jump_app();
    let before = selected_name(&app);
    start(&mut app);
    app.handle_msg(Msg::Deleted {
        generation: app.generation,
        key: "default/api-7d8f9b5c4-xk2lp".into(),
    });
    typing(&mut app, "x");
    assert_eq!(app.mode, Mode::Table);
    assert_eq!(selected_name(&app), before);
    assert!(app.flash.contains("api-7d8f9b5c4-xk2lp"), "{}", app.flash);
}

#[tokio::test]
async fn only_rows_on_screen_get_codes() {
    let (mut app, _rx) = jump_app();
    let last = app.rows().last().unwrap().metadata.name.clone().unwrap();
    assert_eq!(last, "web-5f6c7d8d9-r2kpl");
    app.table_page_rows = 2;
    start(&mut app);
    typing(&mut app, "w");
    assert_eq!(app.mode, Mode::Table, "with web off screen, w is unique");
    assert_eq!(selected_name(&app), "api-7d8f9b5c4-wq9zt");
}

#[tokio::test]
async fn an_empty_table_does_not_start_a_jump() {
    let (mut app, _rx) = test_app();
    app.switch_kind("pods");
    app.handle_key(press(KeyCode::Char('v'))).unwrap();
    assert_eq!(app.mode, Mode::Table);
}

fn draw(app: &mut App) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// The cell of the character at `offset` within `needle` on the first line
/// that holds it.
fn char_at(
    buffer: &ratatui::buffer::Buffer,
    needle: &str,
    offset: usize,
) -> Option<ratatui::buffer::Cell> {
    (0..buffer.area.height).find_map(|y| {
        let line: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let at = line.find(needle)?;
        let x = u16::try_from(line[..at].chars().count() + offset).unwrap();
        Some(buffer[(x, y)].clone())
    })
}

/// Which of the first `n` characters of `needle` stand out as code.
fn marked(buffer: &ratatui::buffer::Buffer, needle: &str, n: usize) -> Vec<bool> {
    (0..n)
        .map(|i| {
            char_at(buffer, needle, i)
                .unwrap()
                .modifier
                .contains(Modifier::REVERSED)
        })
        .collect()
}

#[tokio::test]
async fn every_character_still_to_type_is_highlighted_while_jumping() {
    let (mut app, _rx) = jump_app();
    let buffer = draw(&mut app);
    assert_eq!(
        marked(&buffer, "api-7d8f9b5c4-xk2lp", 19),
        [false; 19],
        "no codes outside jump mode"
    );

    start(&mut app);
    let buffer = draw(&mut app);
    let mut wq = [false; 19];
    wq[14] = true;
    wq[15] = true;
    assert_eq!(marked(&buffer, "api-7d8f9b5c4-wq9zt", 19), wq, "both of wq");
    let mut x = [false; 19];
    x[14] = true;
    assert_eq!(marked(&buffer, "api-7d8f9b5c4-xk2lp", 19), x, "x alone");
    assert_eq!(
        marked(&buffer, "web-5f6c7d8d9-r2kpl", 3),
        [true, true, false]
    );
    let cell = char_at(&buffer, "api-7d8f9b5c4-xk2lp", 14).unwrap();
    assert_eq!(cell.fg, crate::theme::pink(), "codes have their own color");

    typing(&mut app, "w");
    let buffer = draw(&mut app);
    let mut q = [false; 19];
    q[15] = true;
    assert_eq!(
        marked(&buffer, "api-7d8f9b5c4-wq9zt", 19),
        q,
        "only what is left to type"
    );
    assert_eq!(
        marked(&buffer, "web-5f6c7d8d9-r2kpl", 3),
        [false, true, false]
    );
    assert_eq!(
        marked(&buffer, "api-7d8f9b5c4-xk2lp", 19),
        [false; 19],
        "rows that no longer match"
    );
}

#[tokio::test]
async fn the_prompt_line_shows_what_was_typed() {
    let (mut app, _rx) = jump_app();
    start(&mut app);
    typing(&mut app, "w");
    let buffer = draw(&mut app);
    let text: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .map(|p| buffer[p].symbol().to_string())
        .collect();
    assert!(text.contains("jump: w"), "{text}");
}

#[tokio::test]
async fn a_cut_off_code_shows_at_the_left_edge() {
    let (mut app, _rx) = test_app();
    app.switch_kind("pods");
    for suffix in ["xk2lp", "wq9zt"] {
        apply(
            &mut app,
            pod(&format!(
                "kube-prometheus-stack-operator-6d8f7c9b5-{suffix}"
            )),
        );
    }
    // Shares `w` with the cut-off `wq9zt`, so that code needs two characters.
    apply(&mut app, pod("web"));
    start(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(60, 24)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    let edges: Vec<(String, Modifier)> = (0..buffer.area.height)
        .filter_map(|y| {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            line.contains("kube-pro").then(|| {
                assert!(
                    !line.contains("-6d8f7c9b5-"),
                    "the suffix is cut off: {line}"
                );
                let edge = format!("{}{}", buffer[(1, y)].symbol(), buffer[(2, y)].symbol());
                (edge, buffer[(1, y)].modifier)
            })
        })
        .collect();
    let mut letters: Vec<&str> = edges.iter().map(|(c, _)| c.as_str()).collect();
    letters.sort_unstable();
    assert_eq!(letters, ["wq", "x "]);
    assert!(edges.iter().all(|(_, m)| m.contains(Modifier::REVERSED)));
}
