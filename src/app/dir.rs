//! `:dir [path]` — browse local manifests and apply or delete them against the
//! current context. Apply always goes through a `kubectl diff` preview first.

use super::*;

use std::path::{Path, PathBuf};

/// File names that make a directory a kustomization, so it is applied with
/// `-k` instead of `-f`.
const KUSTOMIZATION_FILES: &[&str] = &["kustomization.yaml", "kustomization.yml", "Kustomization"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirEntryKind {
    Dir,
    Kustomization,
    Manifest,
}

impl DirEntryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dir => "dir",
            Self::Kustomization => "kustomize",
            Self::Manifest => "manifest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: DirEntryKind,
    pub size: Option<u64>,
}

#[derive(Default)]
pub struct DirBrowser {
    pub path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub state: ListState,
    /// The manifest whose apply preview is open in the diff view.
    pub preview: Option<(PathBuf, DirEntryKind)>,
}

impl DirBrowser {
    fn selected(&self) -> Option<&DirEntry> {
        self.state.selected().and_then(|i| self.entries.get(i))
    }

    fn selected_path(&self) -> Option<(PathBuf, DirEntryKind)> {
        self.selected().map(|e| (self.path.join(&e.name), e.kind))
    }
}

/// The directories, kustomizations, and YAML/JSON manifests in `dir`,
/// directories first. Hidden entries are skipped.
pub(super) fn read_manifest_dir(dir: &Path) -> Result<Vec<DirEntry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = std::fs::metadata(entry.path()) else {
            continue;
        };
        let kind = if meta.is_dir() {
            if is_kustomization(&entry.path()) {
                DirEntryKind::Kustomization
            } else {
                DirEntryKind::Dir
            }
        } else if is_manifest(&name) {
            DirEntryKind::Manifest
        } else {
            continue;
        };
        out.push(DirEntry {
            name,
            kind,
            size: (kind == DirEntryKind::Manifest).then_some(meta.len()),
        });
    }
    out.sort_by(|a, b| {
        (b.kind != DirEntryKind::Manifest)
            .cmp(&(a.kind != DirEntryKind::Manifest))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

fn is_manifest(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".yaml") || lower.ends_with(".yml") || lower.ends_with(".json")
}

fn is_kustomization(dir: &Path) -> bool {
    KUSTOMIZATION_FILES.iter().any(|f| dir.join(f).is_file())
}

/// The `-f`/`-k` source arguments for `path`: a kustomization is built with
/// `-k`, a directory is read recursively, a file is read as is.
pub(super) fn manifest_source(path: &Path, kind: DirEntryKind) -> Vec<String> {
    let path = path.to_string_lossy().into_owned();
    match kind {
        DirEntryKind::Kustomization => vec!["-k".into(), path],
        DirEntryKind::Dir => vec!["-f".into(), path, "-R".into()],
        DirEntryKind::Manifest => vec!["-f".into(), path],
    }
}

/// Resolve the `:dir` argument: empty means the working directory, `~` is the
/// home directory, and a relative path starts at the working directory.
pub(super) fn resolve_dir_arg(arg: &str, cwd: &Path, home: Option<&Path>) -> PathBuf {
    let arg = arg.trim();
    let path = if arg.is_empty() {
        cwd.to_path_buf()
    } else if arg == "~" {
        home.map(Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf())
    } else if let (Some(rest), Some(home)) = (arg.strip_prefix("~/"), home) {
        home.join(rest)
    } else {
        cwd.join(arg)
    };
    std::fs::canonicalize(&path).unwrap_or(path)
}

struct KubectlOutput {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl KubectlOutput {
    fn error(&self) -> String {
        self.stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("kubectl failed")
            .to_string()
    }
}

async fn run_kubectl(argv: Vec<String>) -> Result<KubectlOutput, String> {
    match tokio::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .await
    {
        Ok(out) => Ok(KubectlOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("kubectl not found on PATH".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

impl App {
    /// `:dir [path]` — open the manifest browser at `path`.
    pub(super) fn open_dir(&mut self, arg: &str) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let path = resolve_dir_arg(arg, &cwd, home.as_deref());
        if !path.is_dir() {
            self.flash_warn(&format!("not a directory: {}", path.display()));
            return;
        }
        if self.load_dir(path, None) {
            self.mode = Mode::Dir;
        }
    }

    /// List `path` and select `keep` (or the first entry). Returns whether
    /// the listing succeeded.
    fn load_dir(&mut self, path: PathBuf, keep: Option<&str>) -> bool {
        match read_manifest_dir(&path) {
            Ok(entries) => {
                let at = keep
                    .and_then(|k| entries.iter().position(|e| e.name == k))
                    .unwrap_or(0);
                self.dir.entries = entries;
                self.dir.path = path;
                self.dir
                    .state
                    .select((!self.dir.entries.is_empty()).then_some(at));
                true
            }
            Err(e) => {
                self.flash_warn(&e);
                false
            }
        }
    }

    pub(super) fn key_dir(&mut self, key: KeyInput) {
        let len = self.dir.entries.len();
        match key.action {
            Some(Action::Back | Action::Close) => self.mode = Mode::Table,
            Some(Action::Down) => list_step(&mut self.dir.state, len, true),
            Some(Action::Up) => list_step(&mut self.dir.state, len, false),
            Some(Action::First) if len > 0 => self.dir.state.select(Some(0)),
            Some(Action::Last) if len > 0 => self.dir.state.select(Some(len - 1)),
            Some(a @ (Action::PageDown | Action::HalfPageDown)) => list_page(
                &mut self.dir.state,
                len,
                a.page_size(self.picker_page_items),
                true,
            ),
            Some(a @ (Action::PageUp | Action::HalfPageUp)) => list_page(
                &mut self.dir.state,
                len,
                a.page_size(self.picker_page_items),
                false,
            ),
            Some(Action::Accept) => self.dir_accept(),
            Some(Action::Parent) => self.dir_parent(),
            Some(Action::Refresh) => {
                let keep = self.dir.selected().map(|e| e.name.clone());
                self.load_dir(self.dir.path.clone(), keep.as_deref());
            }
            Some(Action::Edit) => self.dir_edit(),
            Some(Action::Apply) => self.dir_preview_apply(),
            Some(Action::Delete) => self.dir_request_delete(),
            _ => {}
        }
    }

    fn dir_accept(&mut self) {
        let Some((path, kind)) = self.dir.selected_path() else {
            return;
        };
        if kind == DirEntryKind::Manifest {
            self.dir_view_file(&path);
        } else {
            self.load_dir(path, None);
        }
    }

    fn dir_parent(&mut self) {
        let Some(parent) = self.dir.path.parent().map(Path::to_path_buf) else {
            return;
        };
        let child = self
            .dir
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.load_dir(parent, child.as_deref());
    }

    fn dir_view_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(body) => {
                self.detail = Scrollable {
                    wrap: self.detail.wrap,
                    title: display_path(path),
                    lines: body.lines().map(String::from).collect::<Vec<_>>().into(),
                    yaml: true,
                    ..Default::default()
                };
                self.return_mode = Mode::Dir;
                self.mode = Mode::Detail;
            }
            Err(e) => self.flash_warn(&format!("{}: {e}", path.display())),
        }
    }

    fn dir_edit(&mut self) {
        let Some((path, kind)) = self.dir.selected_path() else {
            return;
        };
        if kind != DirEntryKind::Manifest {
            self.flash_warn("select a manifest to edit");
            return;
        }
        self.pending = Some(Suspend::Shell(secret_edit::editor_argv(
            secret_edit::configured_editor(),
            &path,
        )));
    }

    /// `a` — diff the selection against the cluster. The apply itself is
    /// offered from the diff view.
    fn dir_preview_apply(&mut self) {
        if self.deny_readonly() {
            return;
        }
        let Some((path, kind)) = self.dir.selected_path() else {
            return;
        };
        let mut argv = self.kubectl_base();
        argv.push("diff".into());
        argv.extend(manifest_source(&path, kind));
        let claim = self.claim_status(format!("diffing {}…", display_path(&path)));
        let tx = self.tx.clone();
        let genr = self.generation;
        tokio::spawn(async move {
            // `kubectl diff` exits 0 when nothing would change and 1 when
            // something would; anything else is an error.
            let result = run_kubectl(argv).await.and_then(|out| match out.code {
                Some(0) => Ok(None),
                Some(1) if !out.stdout.trim().is_empty() => Ok(Some(out.stdout)),
                _ => Err(out.error()),
            });
            let _ = tx
                .send(Msg::DirPreview {
                    generation: genr,
                    claim,
                    path,
                    kind,
                    result,
                })
                .await;
        });
    }

    pub(super) fn finish_dir_preview(
        &mut self,
        claim: StatusClaim,
        path: PathBuf,
        kind: DirEntryKind,
        result: Result<Option<String>, String>,
    ) {
        match result {
            Err(e) => self.set_claimed_status(
                claim,
                format!("diff {} failed: {e}", display_path(&path)),
                true,
            ),
            Ok(None) => self.set_claimed_status(
                claim,
                format!("no changes: {} matches the cluster", display_path(&path)),
                false,
            ),
            Ok(Some(text)) => {
                let apply = self.keymap.first_label("diff", Action::Apply).to_string();
                self.set_claimed_status(
                    claim,
                    format!("review the diff, {apply} applies it"),
                    false,
                );
                // A diff that arrives after the browser was left is dropped:
                // the apply belongs to the view it was asked from.
                if self.mode != Mode::Dir {
                    return;
                }
                self.detail = Scrollable {
                    wrap: self.detail.wrap,
                    title: format!("apply preview ({apply} to apply) · {}", display_path(&path)),
                    lines: preview_lines(&text).into(),
                    ..Default::default()
                };
                self.dir.preview = Some((path, kind));
                self.return_mode = Mode::Dir;
                self.mode = Mode::Diff;
            }
        }
    }

    /// `a` in an apply preview — confirm and apply.
    pub(super) fn dir_request_apply(&mut self) {
        let Some((path, kind)) = self.dir.preview.clone() else {
            return;
        };
        if self.deny_readonly() {
            return;
        }
        let targets = vec![(path.display().to_string(), String::new())];
        let Some(level) = self.guard("apply", "manifests", &targets, ConfirmLevel::Plain) else {
            return;
        };
        self.dir.preview = None;
        self.mode = Mode::Dir;
        let label = format!("Apply {} to {}?", display_path(&path), self.cluster.context);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.begin_guarded(
            ConfirmAction::ManifestApply { path, kind },
            label,
            level,
            name,
        );
    }

    fn dir_request_delete(&mut self) {
        if self.deny_readonly() {
            return;
        }
        let Some((path, kind)) = self.dir.selected_path() else {
            return;
        };
        let targets = vec![(path.display().to_string(), String::new())];
        let Some(level) = self.guard("delete", "manifests", &targets, ConfirmLevel::Plain) else {
            return;
        };
        let label = format!(
            "Delete the resources in {} from {}?",
            display_path(&path),
            self.cluster.context
        );
        let name = self
            .dir
            .selected()
            .map(|e| e.name.clone())
            .unwrap_or_default();
        self.begin_guarded(
            ConfirmAction::ManifestDelete { path, kind },
            label,
            level,
            name,
        );
    }

    pub(super) fn do_manifest(&mut self, verb: &'static str, path: PathBuf, kind: DirEntryKind) {
        self.note_action(verb, path.display().to_string());
        let mut argv = self.kubectl_base();
        argv.push(verb.into());
        argv.extend(manifest_source(&path, kind));
        if verb == "delete" {
            argv.push("--ignore-not-found".into());
        }
        let shown = display_path(&path);
        let claim = self.claim_status(format!("{verb} {shown}…"));
        let tx = self.tx.clone();
        let genr = self.generation;
        tokio::spawn(async move {
            let (message, err) = match run_kubectl(argv).await {
                Ok(out) if out.code == Some(0) => {
                    (manifest_summary(verb, &shown, &out.stdout), false)
                }
                Ok(out) => (format!("{verb} {shown} failed: {}", out.error()), true),
                Err(e) => (format!("{verb} {shown} failed: {e}"), true),
            };
            let _ = tx
                .send(Msg::Flash {
                    generation: genr,
                    claim,
                    message,
                    err,
                })
                .await;
        });
    }
}

/// `kubectl diff` output with the temp-file headers replaced by one line per
/// object, named the way kubectl names it (`apps.v1.Deployment.ns.name`).
pub(super) fn preview_lines(diff: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff ") {
            let object = rest.rsplit(['/', '\\']).next().unwrap_or(rest).to_string();
            if !out.is_empty() {
                out.push(String::new());
            }
            out.push(format!("── {object}"));
        } else if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        } else {
            out.push(line.to_string());
        }
    }
    out
}

/// `path` with the home directory shown as `~`.
pub fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `applied k8s/app.yaml: deployment.apps/web configured (+2 more)`.
pub(super) fn manifest_summary(verb: &str, path: &str, out: &str) -> String {
    let past = if verb == "apply" {
        "applied"
    } else {
        "deleted"
    };
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    match lines.as_slice() {
        [] => format!("{past} {path}: nothing to do"),
        [one] => format!("{past} {path}: {one}"),
        [first, rest @ ..] => format!("{past} {path}: {first} (+{} more)", rest.len()),
    }
}
