use super::*;

/// The rows a jump can reach and what has been typed so far. The codes are
/// fixed when the jump starts, so a watch update cannot move one while it is
/// being typed.
#[derive(Debug, Default)]
pub(super) struct Jump {
    targets: Vec<Target>,
    typed: String,
}

#[derive(Debug)]
struct Target {
    key: String,
    name: String,
    /// Char offset into `name` where the code to type starts.
    start: usize,
    /// How many characters of the code reach this row alone.
    len: usize,
}

impl Target {
    /// Whether `typed` is the start of this row's code, ignoring case.
    fn matches(&self, typed: &str) -> bool {
        let mut code = self.name.chars().skip(self.start);
        typed.chars().all(|t| {
            code.next()
                .is_some_and(|c| c.to_lowercase().eq(t.to_lowercase()))
        })
    }
}

impl App {
    /// Give every row on screen a code and start reading it from the keyboard.
    pub(super) fn start_jump(&mut self) {
        let targets: Vec<(String, String)> = self
            .rows_window(self.table_state.offset(), self.table_page_rows)
            .into_iter()
            .map(|o| (self.key_of(o), o.metadata.name.clone().unwrap_or_default()))
            .collect();
        if targets.is_empty() {
            return;
        }
        let names: Vec<&str> = targets.iter().map(|(_, name)| name.as_str()).collect();
        let starts = crate::jump::starts(&names);
        let lengths = crate::jump::lengths(&names, &starts);
        self.jump = Jump {
            targets: targets
                .into_iter()
                .zip(starts.into_iter().zip(lengths))
                .map(|((key, name), (start, len))| Target {
                    key,
                    name,
                    start,
                    len,
                })
                .collect(),
            typed: String::new(),
        };
        self.mode = Mode::Jump;
    }

    pub(super) fn key_jump(&mut self, key: KeyInput) {
        if edit_action(key.action, &mut self.jump.typed) {
            return;
        }
        match (key.action, key.code) {
            (Some(Action::Back), _) => self.end_jump(),
            (Some(Action::Accept), _) => self.land_jump(&self.jump.typed.clone()),
            (Some(Action::Backspace), _) => {
                self.jump.typed.pop();
            }
            (None, KeyCode::Char(c)) if key.is_text() => {
                let typed = format!("{}{c}", self.jump.typed);
                let mut hits = self.jump.targets.iter().filter(|t| t.matches(&typed));
                match (hits.next(), hits.next()) {
                    // A character no code continues with is ignored.
                    (None, _) => {}
                    (Some(_), None) => self.land_jump(&typed),
                    (Some(_), Some(_)) => self.jump.typed = typed,
                }
            }
            _ => {}
        }
    }

    /// Move the cursor to the first row `typed` still leads to, wherever the
    /// row is now, and leave the jump.
    fn land_jump(&mut self, typed: &str) {
        let jump = std::mem::take(&mut self.jump);
        self.mode = Mode::Table;
        let Some(target) = jump.targets.iter().find(|t| t.matches(typed)) else {
            return;
        };
        match self
            .rows()
            .iter()
            .position(|o| self.key_of(o) == target.key)
        {
            Some(i) => self.table_state.select(Some(i)),
            None => self.flash_warn(&format!("jump: {} is gone", target.name)),
        }
    }

    fn end_jump(&mut self) {
        self.jump = Jump::default();
        self.mode = Mode::Table;
    }

    /// What has been typed of a code so far.
    pub(crate) fn jump_typed(&self) -> &str {
        &self.jump.typed
    }

    /// The char range in `name` that is still to type to reach the row
    /// `key`, while that row can still be reached.
    pub(crate) fn jump_marks(&self, key: &str, name: &str) -> Option<std::ops::Range<usize>> {
        if self.mode != Mode::Jump {
            return None;
        }
        let target = self
            .jump
            .targets
            .iter()
            .find(|t| t.key == key && t.name == name)?;
        let from = target.start + self.jump.typed.chars().count();
        let to = target.start + target.len;
        (target.matches(&self.jump.typed) && from < to).then_some(from..to)
    }
}
