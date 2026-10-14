use super::*;

impl App {
    /// The kind the table lists, unless it's a synthetic view backed by an
    /// unrelated kind (helm releases on `secrets`).
    fn layout_kind(&self) -> Option<&Kind> {
        self.kind
            .as_ref()
            .filter(|kind| kind.ar.plural.to_lowercase() == self.kind_plural)
    }

    /// The named layout active for the current kind, if it isn't the default.
    pub fn active_layout(&self) -> Option<&str> {
        let kind = self.layout_kind()?;
        self.layouts
            .get(&kind.title())
            .map(|(_, name)| name.as_str())
    }

    /// The named layouts configured for the current kind.
    pub(super) fn kind_layouts(&self) -> Vec<String> {
        self.layout_kind()
            .map(|kind| crate::views::layouts_for(&self.user_views, &kind.ar))
            .unwrap_or_default()
    }

    /// Make `name` the current kind's layout without rebuilding the table:
    /// `default` drops back to the default layout and `wide` turns wide mode
    /// on. Returns the warning to show when the kind has no such layout.
    pub(super) fn select_layout(&mut self, name: &str) -> Result<(), String> {
        let Some(kind) = self.layout_kind().cloned() else {
            return Err("this view has no layouts".into());
        };
        match name {
            "default" => {
                self.layouts.remove(&kind.title());
            }
            "wide" => self.wide = true,
            _ if crate::views::layouts_for(&self.user_views, &kind.ar)
                .iter()
                .any(|layout| layout == name) =>
            {
                self.layouts
                    .insert(kind.title(), (kind.ar.clone(), name.to_string()));
            }
            _ => return Err(format!("{} has no layout '{name}'", kind.ar.plural)),
        }
        Ok(())
    }

    /// `:layout <name>`: switch the current kind's layout and rebuild the
    /// table. A sort the old layout configured gives way to the new one's.
    pub(super) fn set_layout(&mut self, name: &str) {
        if let Err(warning) = self.select_layout(name) {
            self.flash_warn(&warning);
            return;
        }
        if self.sort_origin == SortOrigin::Configured {
            self.reset_sort();
        }
        self.refresh_view_spec();
        self.apply_remembered_sort();
        self.apply_view_sort();
        self.save_history_filter();
        self.flash = match (name, self.active_layout()) {
            ("wide", _) => "wide columns: on".to_string(),
            (_, Some(layout)) => format!("layout: {}:{layout}", self.list_title()),
            (_, None) => format!("layout: {} default", self.list_title()),
        };
        self.flash_err = false;
    }

    /// `:layout` without a name: the palette, listing the current kind's
    /// layouts with `default` first.
    pub(super) fn open_layout_picker(&mut self) {
        let Some(kind) = self.layout_kind() else {
            self.flash_warn("this view has no layouts");
            return;
        };
        if self.kind_layouts().is_empty() {
            let plural = kind.ar.plural.clone();
            self.flash_warn(&format!("{plural} has no named layouts"));
            return;
        }
        self.open_palette();
        self.command = "layout ".into();
        self.update_suggestions();
    }

    /// Forget remembered layouts the config no longer defines. Returns a
    /// warning when the current kind's layout was one of them.
    pub(super) fn prune_layouts(&mut self) -> Option<String> {
        let current = self.layout_kind().map(Kind::title);
        let views = &self.user_views;
        let mut warning = None;
        self.layouts.retain(|title, (ar, name)| {
            let keep = crate::views::layouts_for(views, ar).contains(name);
            if !keep && current.as_deref() == Some(title.as_str()) {
                warning = Some(format!(
                    "layout '{name}' no longer exists for {}; using the default",
                    ar.plural
                ));
            }
            keep
        });
        warning
    }

    /// Palette entries for every configured layout, as `plural:layout`.
    pub(super) fn layout_entries(&self) -> Vec<String> {
        let mut entries: Vec<String> = self
            .user_views
            .keys()
            .filter_map(|key| {
                let layout = crate::views::key_layout(key)?;
                let kind = self.cluster.resolve(crate::views::key_plural(key))?;
                crate::views::layouts_for(&self.user_views, &kind.ar)
                    .iter()
                    .any(|name| name == layout)
                    .then(|| format!("{}:{layout}", kind.title()))
            })
            .collect();
        entries.sort();
        entries.dedup();
        entries
    }
}
