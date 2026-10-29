use super::*;
use crate::theme;
use ratatui::{Terminal, backend::TestBackend, style::Color};

fn badge_dir(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("sofka-context-badge-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The first rendered line holding `needle`, with the foreground color of the
/// needle's first cell.
fn find(app: &mut App, needle: &str) -> Option<(String, Color)> {
    let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height).find_map(|y| {
        let line: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let at = line.find(needle)?;
        let x = u16::try_from(line[..at].chars().count()).unwrap();
        let fg = buffer[(x, y)].fg;
        Some((line, fg))
    })
}

fn context_line(app: &mut App) -> String {
    find(app, "Context:").expect("full header is drawn").0
}

fn switch_to(app: &mut App, context: &str) {
    palette(app, &format!("ctx {context}"));
    land_context(app, context);
}

#[tokio::test]
async fn context_badge_follows_context_switches_in_both_headers() {
    let dir = badge_dir("switch");
    write_config(
        &dir.join("clusters/test-cluster/prod"),
        "readonly = true\n[context_badge]\nlabel = \"prod\"\ncolor = \"red\"\n",
    );
    let (mut app, _rx) = test_app();
    app.all_contexts = vec!["prod".into(), "staging".into(), "test".into()];
    app.config = crate::config::ConfigLoader::from_dir(Some(dir.clone()));
    app.switch_kind("pods");
    assert!(find(&mut app, "[prod]").is_none());

    switch_to(&mut app, "prod");
    let (line, color) = find(&mut app, "[prod]").expect("full header shows the badge");
    assert!(line.contains("Context:"), "{line}");
    assert!(line.contains("prod  [prod]  [read-only]"), "{line}");
    assert_eq!(color, theme::red());

    app.handle_key(ctrl(KeyCode::Char('e'))).unwrap();
    assert!(app.compact);
    let (line, color) = find(&mut app, "[prod]").expect("compact header shows the badge");
    assert!(line.contains(" sofka "), "{line}");
    assert!(line.contains("prod [prod] [ro]"), "{line}");
    assert_eq!(color, theme::red());

    switch_to(&mut app, "staging");
    assert!(find(&mut app, "[prod]").is_none());
    app.handle_key(ctrl(KeyCode::Char('e'))).unwrap();
    assert!(!app.compact);
    assert!(find(&mut app, "[prod]").is_none());
    assert!(!context_line(&mut app).contains("[read-only]"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn context_picker_rows_show_each_contexts_badge() {
    let dir = badge_dir("picker");
    write_config(
        &dir.join("clusters/prod-cluster"),
        "[context_badge]\nlabel = \"prod\"\ncolor = \"yellow\"\n",
    );
    write_config(
        &dir.join("clusters/prod-cluster/prod-admin"),
        "[context_badge]\nlabel = \"prod admin\"\n",
    );
    let (mut app, _rx) = test_app();
    app.config = crate::config::ConfigLoader::from_dir(Some(dir.clone()));
    palette(&mut app, "ctx");
    assert_eq!(app.mode, Mode::Contexts);
    app.handle_msg(Msg::Contexts {
        generation: app.generation,
        list: vec!["prod-admin".into(), "prod-view".into(), "test".into()],
    });
    // The list renders before the override files are read.
    assert!(find(&mut app, "prod-admin").is_some());
    assert!(find(&mut app, "[prod").is_none());

    let contexts = [
        ("prod-admin", "prod-cluster"),
        ("prod-view", "prod-cluster"),
        ("test", "test-cluster"),
    ]
    .map(|(context, cluster)| (context.to_string(), cluster.to_string()));
    let badges = app.config.context_badges(&contexts);
    app.handle_msg(Msg::ContextBadges {
        generation: app.generation,
        badges,
    });
    let (line, color) = find(&mut app, "[prod admin]").unwrap();
    assert!(line.contains("prod-admin  [prod admin]"), "{line}");
    assert_eq!(color, theme::yellow(), "color merges from the cluster file");
    let (line, color) = find(&mut app, "[prod]").unwrap();
    assert!(line.contains("prod-view  [prod]"), "{line}");
    assert_eq!(color, theme::yellow());
    let (line, _) = find(&mut app, "● test").unwrap();
    assert!(!line.contains('['), "{line}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn context_badge_color_without_label_warns_and_shows_no_badge() {
    let dir = badge_dir("warn");
    write_config(
        &dir.join("clusters/test-cluster/prod"),
        "[context_badge]\ncolor = \"red\"\n",
    );
    write_config(
        &dir.join("clusters/test-cluster/stage"),
        "[context_badge]\nlabel = \"stage\"\ncolor = \"crimson\"\n",
    );
    let (mut app, _rx) = test_app();
    app.all_contexts = vec!["prod".into(), "stage".into(), "test".into()];
    app.config = crate::config::ConfigLoader::from_dir(Some(dir.clone()));
    app.switch_kind("pods");

    switch_to(&mut app, "prod");
    let warning = "context_badge: color needs a label; no badge shown";
    assert_eq!(app.config_warnings, [warning]);
    assert!(app.flash_err);
    assert!(app.flash.contains(warning), "{}", app.flash);
    assert!(app.context_badge.is_none());
    let line = context_line(&mut app);
    assert!(!line.contains('['), "{line}");

    switch_to(&mut app, "stage");
    assert_eq!(app.config_warnings.len(), 1);
    assert!(
        app.config_warnings[0].starts_with("context_badge.color: unknown color 'crimson'"),
        "{:?}",
        app.config_warnings
    );
    let (_, color) = find(&mut app, "[stage]").expect("the label still shows");
    assert_eq!(color, theme::text());

    write_config(&dir, "[context_badge]\ncolor = \"red\"\n");
    switch_to(&mut app, "test");
    palette(&mut app, "reload");
    assert_eq!(app.config_warnings, [warning]);
    assert!(app.context_badge.is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn reload_applies_an_edited_badge() {
    let dir = badge_dir("reload");
    write_config(&dir, "");
    let (mut app, _rx) = test_app();
    app.config = crate::config::ConfigLoader::from_dir(Some(dir.clone()));
    app.switch_kind("pods");
    assert!(find(&mut app, "[local]").is_none());
    write_config(
        &dir.join("clusters/test-cluster"),
        "[context_badge]\nlabel = \"local\"\ncolor = \"green\"\n",
    );
    palette(&mut app, "reload");
    assert!(app.config_warnings.is_empty(), "{:?}", app.config_warnings);
    let (line, color) = find(&mut app, "[local]").unwrap();
    assert!(line.contains("test  [local]"), "{line}");
    assert_eq!(color, theme::green());
    std::fs::remove_dir_all(dir).unwrap();
}
