use super::*;

fn save_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("sofka-save-{tag}-{}", std::process::id()))
}

async fn finish_save(app: &mut App, rx: &mut Receiver<Msg>) -> Result<std::path::PathBuf, String> {
    let msg = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let msg = rx.recv().await.expect("channel closed");
            if matches!(msg, Msg::Saved { .. }) {
                break msg;
            }
        }
    })
    .await
    .expect("save did not report");
    let Msg::Saved { result, .. } = &msg else {
        unreachable!()
    };
    let result = result.clone();
    app.handle_msg(msg);
    result
}

fn document(app: &mut App, mode: Mode, title: &str, yaml: bool) {
    app.detail = Scrollable {
        title: title.into(),
        yaml,
        lines: vec!["first".to_string(), "second".to_string()].into(),
        ..Default::default()
    };
    app.mode = mode;
}

fn saved_name(path: &std::path::Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

#[tokio::test]
async fn ctrl_s_saves_the_whole_rendered_yaml_into_a_created_save_dir() {
    let (mut app, mut rx) = test_app();
    let root = save_dir("yaml");
    let dir = root.join("nested").join("dumps");
    app.save_dir = Some(dir.display().to_string());
    app.switch_kind("pods");
    apply(
        &mut app,
        json!({"apiVersion": "v1", "kind": "Pod",
            "metadata": {"name": "web", "namespace": "default",
                "managedFields": [{"manager": "kubectl", "operation": "Apply"}]},
            "spec": {"containers": [{"name": "app", "image": "nginx"}]}}),
    );
    app.handle_key(press(KeyCode::Char('y'))).unwrap();
    assert_eq!(app.mode, Mode::Detail);
    app.handle_key(press(KeyCode::Char('/'))).unwrap();
    for c in "nginx".chars() {
        app.handle_key(press(KeyCode::Char(c))).unwrap();
    }
    app.handle_key(press(KeyCode::Enter)).unwrap();
    app.handle_key(press(KeyCode::Char('G'))).unwrap();
    let expected = app.doc_text();

    app.handle_key(ctrl(KeyCode::Char('s'))).unwrap();
    let path = finish_save(&mut app, &mut rx).await.unwrap();

    assert_eq!(path.parent(), Some(dir.as_path()));
    let name = saved_name(&path);
    assert!(name.starts_with("sofka-web---YAML-"), "{name}");
    assert!(name.ends_with(".yaml"), "{name}");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(saved, expected);
    assert!(saved.contains("name: web"));
    assert!(saved.contains("image: nginx"));
    assert!(!saved.contains("managedFields"), "{saved}");
    assert!(!app.flash_err);
    assert_eq!(app.flash, format!("saved document → {}", path.display()));
    assert_eq!(app.mode, Mode::Detail);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn ctrl_s_names_the_file_after_the_view() {
    let (mut app, mut rx) = test_app();
    let dir = save_dir("ext");
    app.save_dir = Some(dir.display().to_string());
    for (mode, title, yaml, ext) in [
        (Mode::Detail, "web - describe", false, "txt"),
        (Mode::Events, "events: web", false, "txt"),
        (
            Mode::Diff,
            "web — diff (last-applied → live)",
            false,
            "yaml",
        ),
    ] {
        document(&mut app, mode, title, yaml);
        app.handle_key(ctrl(KeyCode::Char('s'))).unwrap();
        let path = finish_save(&mut app, &mut rx).await.unwrap();
        let name = saved_name(&path);
        assert!(name.ends_with(&format!(".{ext}")), "{title}: {name}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond");
        assert_eq!(app.mode, mode);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn ctrl_s_saves_the_helm_manifest_a_helm_row_shows() {
    let (mut app, mut rx) = test_app();
    let dir = save_dir("helm");
    app.save_dir = Some(dir.display().to_string());
    app.open_helm_releases();
    apply(
        &mut app,
        helm_release_secret("myapp", "default", 1, "deployed"),
    );
    app.handle_key(press(KeyCode::Char('y'))).unwrap();
    assert!(app.detail.title.contains("manifest"));

    app.handle_key(ctrl(KeyCode::Char('s'))).unwrap();
    let path = finish_save(&mut app, &mut rx).await.unwrap();

    assert!(saved_name(&path).ends_with(".yaml"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "apiVersion: v1\nkind: ConfigMap"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn ctrl_s_on_an_empty_document_warns_without_writing() {
    let (mut app, mut rx) = test_app();
    app.detail = Scrollable {
        title: "empty".into(),
        ..Default::default()
    };
    app.mode = Mode::Detail;
    app.handle_key(ctrl(KeyCode::Char('s'))).unwrap();
    assert!(app.flash_err);
    assert_eq!(app.flash, "nothing to save");
    while let Ok(msg) = rx.try_recv() {
        assert!(!matches!(msg, Msg::Saved { .. }));
    }
}

#[tokio::test]
async fn ctrl_s_reports_a_save_dir_that_cannot_be_created() {
    let (mut app, mut rx) = test_app();
    let blocker = save_dir("blocked");
    std::fs::write(&blocker, "not a directory").unwrap();
    app.save_dir = Some(blocker.join("dumps").display().to_string());
    document(&mut app, Mode::Detail, "web — YAML", true);

    app.handle_key(ctrl(KeyCode::Char('s'))).unwrap();
    let err = finish_save(&mut app, &mut rx).await.unwrap_err();

    assert!(err.starts_with("cannot create "), "{err}");
    assert!(app.flash_err);
    assert_eq!(app.flash, format!("save failed: {err}"));
    std::fs::remove_file(blocker).unwrap();
}

#[tokio::test]
async fn log_saves_use_save_dir() {
    let (mut app, mut rx) = test_app();
    let dir = save_dir("logs");
    app.save_dir = Some(dir.display().to_string());
    app.mode = Mode::Logs;
    app.logs.view.title = "web".into();
    app.logs.view.lines = vec!["line one".to_string()].into();

    app.handle_key(ctrl(KeyCode::Char('s'))).unwrap();
    let path = finish_save(&mut app, &mut rx).await.unwrap();

    assert_eq!(path.parent(), Some(dir.as_path()));
    assert!(saved_name(&path).ends_with(".log"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "line one");
    assert_eq!(app.flash, format!("saved logs → {}", path.display()));
    std::fs::remove_dir_all(dir).unwrap();
}
