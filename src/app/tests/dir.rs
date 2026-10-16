use super::*;
use crate::app::dir::{
    DirEntryKind, manifest_source, manifest_summary, preview_lines, resolve_dir_arg,
};
use std::path::{Path, PathBuf};

const DIFF: &str = "\
diff -u -N /tmp/LIVE-1/v1.ConfigMap.default.web /tmp/MERGED-2/v1.ConfigMap.default.web
--- /tmp/LIVE-1/v1.ConfigMap.default.web\t2026-10-16
+++ /tmp/MERGED-2/v1.ConfigMap.default.web\t2026-10-16
@@ -1,3 +1,3 @@
-  hello: world
+  hello: there
";

/// A scratch tree: `app/` (plain), `kust/` (kustomization), `web.yaml`, plus
/// a hidden manifest and a README the browser must skip.
fn manifests() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "sofka-dir-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::create_dir_all(root.join("kust")).unwrap();
    std::fs::write(root.join("kust/kustomization.yaml"), "resources: []\n").unwrap();
    std::fs::write(root.join("app/cm.yml"), "kind: ConfigMap\n").unwrap();
    std::fs::write(
        root.join("web.yaml"),
        "kind: ConfigMap\nmetadata:\n  name: web\n",
    )
    .unwrap();
    std::fs::write(root.join(".hidden.yaml"), "").unwrap();
    std::fs::write(root.join("README.md"), "").unwrap();
    std::fs::canonicalize(root).unwrap()
}

fn type_command(app: &mut App, cmd: &str) {
    app.handle_key(press(KeyCode::Char(':'))).unwrap();
    for c in cmd.chars() {
        app.handle_key(press(KeyCode::Char(c))).unwrap();
    }
    app.handle_key(press(KeyCode::Enter)).unwrap();
}

fn names(app: &App) -> Vec<(&str, DirEntryKind)> {
    app.dir
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.kind))
        .collect()
}

fn select(app: &mut App, name: &str) {
    app.handle_key(press(KeyCode::Char('g'))).unwrap();
    while app.dir.entries[app.dir.state.selected().unwrap()].name != name {
        app.handle_key(press(KeyCode::Char('j'))).unwrap();
    }
}

#[tokio::test]
async fn dir_lists_folders_first_and_only_manifests() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));

    assert_eq!(app.mode, Mode::Dir);
    assert_eq!(app.dir.path, root);
    assert_eq!(
        names(&app),
        [
            ("app", DirEntryKind::Dir),
            ("kust", DirEntryKind::Kustomization),
            ("web.yaml", DirEntryKind::Manifest),
        ]
    );
}

#[tokio::test]
async fn dir_enters_folders_and_backspace_returns_to_the_folder_left() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));

    select(&mut app, "app");
    app.handle_key(press(KeyCode::Enter)).unwrap();
    assert_eq!(app.dir.path, root.join("app"));
    assert_eq!(names(&app), [("cm.yml", DirEntryKind::Manifest)]);

    app.handle_key(press(KeyCode::Backspace)).unwrap();
    assert_eq!(app.dir.path, root);
    assert_eq!(app.dir.state.selected(), Some(0));

    app.handle_key(press(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Table);
}

#[tokio::test]
async fn dir_enter_on_a_manifest_shows_it_and_esc_returns_to_the_listing() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));

    select(&mut app, "web.yaml");
    app.handle_key(press(KeyCode::Enter)).unwrap();
    assert_eq!(app.mode, Mode::Detail);
    assert!(app.detail.lines.iter().any(|l| l == "  name: web"));

    app.handle_key(press(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Dir);
}

#[tokio::test]
async fn dir_rejects_a_path_that_is_not_a_directory() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(
        &mut app,
        &format!("dir {}", root.join("web.yaml").display()),
    );
    assert_eq!(app.mode, Mode::Table);
    assert!(app.flash.contains("not a directory"), "{}", app.flash);
}

#[tokio::test]
async fn dir_apply_previews_the_diff_then_confirms_from_the_listing() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));
    select(&mut app, "web.yaml");

    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    assert_eq!(app.mode, Mode::Dir);
    assert!(app.flash.starts_with("diffing"), "{}", app.flash);

    app.handle_msg(Msg::DirPreview {
        generation: app.generation,
        claim: current_claim(&app),
        path: root.join("web.yaml"),
        kind: DirEntryKind::Manifest,
        result: Ok(Some(DIFF.into())),
    });
    assert_eq!(app.mode, Mode::Diff);
    assert_eq!(app.detail.lines[0], "── v1.ConfigMap.default.web");
    assert!(!app.detail.lines.iter().any(|l| l.starts_with("+++")));

    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    assert_eq!(app.mode, Mode::Confirm);
    assert!(
        app.confirm_label.starts_with("Apply "),
        "{}",
        app.confirm_label
    );
    assert!(matches!(
        app.confirm_action,
        Some(ConfirmAction::ManifestApply { ref path, kind: DirEntryKind::Manifest })
            if *path == root.join("web.yaml")
    ));

    app.handle_key(press(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Dir);
    assert!(app.confirm_action.is_none());
}

#[tokio::test]
async fn dir_apply_with_no_changes_stays_in_the_listing() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));
    select(&mut app, "web.yaml");
    app.handle_key(press(KeyCode::Char('a'))).unwrap();

    app.handle_msg(Msg::DirPreview {
        generation: app.generation,
        claim: current_claim(&app),
        path: root.join("web.yaml"),
        kind: DirEntryKind::Manifest,
        result: Ok(None),
    });
    assert_eq!(app.mode, Mode::Dir);
    assert!(app.flash.starts_with("no changes"), "{}", app.flash);
}

#[tokio::test]
async fn dir_leaving_the_preview_drops_the_pending_apply() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));
    select(&mut app, "web.yaml");
    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    app.handle_msg(Msg::DirPreview {
        generation: app.generation,
        claim: current_claim(&app),
        path: root.join("web.yaml"),
        kind: DirEntryKind::Manifest,
        result: Ok(Some(DIFF.into())),
    });

    app.handle_key(press(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Dir);
    assert!(app.dir.preview.is_none());

    // `a` in a diff that did not come from the browser applies nothing.
    app.mode = Mode::Diff;
    app.return_mode = Mode::Table;
    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    assert_eq!(app.mode, Mode::Diff);
    assert!(app.confirm_action.is_none());
}

#[tokio::test]
async fn dir_delete_confirms_with_the_kustomization() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    type_command(&mut app, &format!("dir {}", root.display()));
    select(&mut app, "kust");

    app.handle_key(press(KeyCode::Char('d'))).unwrap();
    assert_eq!(app.mode, Mode::Confirm);
    assert!(app.confirm_label.starts_with("Delete the resources in "));
    assert!(matches!(
        app.confirm_action,
        Some(ConfirmAction::ManifestDelete {
            kind: DirEntryKind::Kustomization,
            ..
        })
    ));
    app.handle_key(press(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Dir);
}

#[tokio::test]
async fn dir_readonly_blocks_apply_and_delete() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    app.readonly = true;
    type_command(&mut app, &format!("dir {}", root.display()));
    select(&mut app, "web.yaml");

    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    assert!(app.flash.starts_with("read-only"), "{}", app.flash);
    app.handle_key(press(KeyCode::Char('d'))).unwrap();
    assert_eq!(app.mode, Mode::Dir);
    assert!(app.confirm_action.is_none());
}

#[tokio::test]
async fn dir_guardrail_denies_apply() {
    let root = manifests();
    let (mut app, _rx) = test_app();
    app.guardrails = toml::from_str::<crate::config::Config>(
        r#"
        [[guardrails]]
        actions = ["apply"]
        deny = true
        "#,
    )
    .unwrap()
    .guardrails;
    type_command(&mut app, &format!("dir {}", root.display()));
    select(&mut app, "web.yaml");
    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    app.handle_msg(Msg::DirPreview {
        generation: app.generation,
        claim: current_claim(&app),
        path: root.join("web.yaml"),
        kind: DirEntryKind::Manifest,
        result: Ok(Some(DIFF.into())),
    });
    app.handle_key(press(KeyCode::Char('a'))).unwrap();
    assert!(app.confirm_action.is_none());
    assert!(app.flash.contains("blocked by guardrail"), "{}", app.flash);
}

#[test]
fn manifest_sources_follow_the_entry_kind() {
    let p = Path::new("/m/x");
    assert_eq!(manifest_source(p, DirEntryKind::Manifest), ["-f", "/m/x"]);
    assert_eq!(manifest_source(p, DirEntryKind::Dir), ["-f", "/m/x", "-R"]);
    assert_eq!(
        manifest_source(p, DirEntryKind::Kustomization),
        ["-k", "/m/x"]
    );
}

#[test]
fn preview_lines_replace_temp_file_headers() {
    assert_eq!(
        preview_lines(DIFF),
        [
            "── v1.ConfigMap.default.web",
            "@@ -1,3 +1,3 @@",
            "-  hello: world",
            "+  hello: there",
        ]
    );
}

#[test]
fn dir_argument_resolves_home_and_relative_paths() {
    let cwd = Path::new("/nonexistent/cwd");
    let home = Path::new("/nonexistent/home");
    assert_eq!(resolve_dir_arg("", cwd, Some(home)), cwd);
    assert_eq!(resolve_dir_arg(" ~ ", cwd, Some(home)), home);
    assert_eq!(resolve_dir_arg("~/k8s", cwd, Some(home)), home.join("k8s"));
    assert_eq!(
        resolve_dir_arg("k8s app", cwd, Some(home)),
        cwd.join("k8s app")
    );
    assert_eq!(resolve_dir_arg("/abs", cwd, Some(home)), Path::new("/abs"));
}

#[test]
fn manifest_summary_names_the_first_object() {
    assert_eq!(
        manifest_summary("apply", "web.yaml", "configmap/web configured\n"),
        "applied web.yaml: configmap/web configured"
    );
    assert_eq!(
        manifest_summary("delete", "k", "a deleted\nb deleted\nc deleted\n"),
        "deleted k: a deleted (+2 more)"
    );
    assert_eq!(
        manifest_summary("apply", "x", ""),
        "applied x: nothing to do"
    );
}
