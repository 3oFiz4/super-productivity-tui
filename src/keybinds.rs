use crate::config::KeybindMode;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct KeybindEntry {
    pub normal: &'static str,
    pub nvim: &'static str,
    pub description: &'static str,
    pub is_nvim_highlight: bool,
}

impl KeybindEntry {
    pub fn display_key(&self, mode: KeybindMode) -> String {
        match mode {
            KeybindMode::Normal => self.normal.to_string(),
            KeybindMode::Nvim => self.nvim.to_string(),
            KeybindMode::Both => {
                if self.normal == self.nvim {
                    self.normal.to_string()
                } else {
                    format!("{} / {}", self.normal, self.nvim)
                }
            }
        }
    }
}

pub fn all_keybinds() -> Vec<KeybindEntry> {
    vec![
        KeybindEntry {
            normal: "Tab / Shift+Tab",
            nvim: "Ctrl+h / Ctrl+l",
            description: "Switch section (Sidebar / Main)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Up / Down",
            nvim: "k / j",
            description: "Move up / down in list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "ga / gn",
            nvim: "ga / gn",
            description: "Jump to Main area / Navigation",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "hn",
            nvim: "zn",
            description: "Hide / show navigation sidebar",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Enter",
            nvim: "o",
            description: "Toggle task done / open nav item",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "p / Right-Click",
            nvim: "O",
            description: "Open Properties menu (scoped focus)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "r (in Properties)",
            nvim: "i / r (in Properties)",
            description: "Rename task inline (Enter or jk to save)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "a",
            nvim: "a",
            description: "Add new task (title, note, top/bottom, project, tags)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "d",
            nvim: "d",
            description: "Delete selected task",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "n",
            nvim: "K",
            description: "Show / hide task note",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: ":h",
            nvim: ":h",
            description: "Open searchable Help / Keybinds",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: ":s",
            nvim: ":s",
            description: "Open Settings (CLI configuration)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "q",
            nvim: "q",
            description: "Quit application",
            is_nvim_highlight: false,
        },
    ]
}

#[derive(Debug, Default)]
pub struct SequenceState {
    pub pending: Option<(char, Instant)>,
}

impl SequenceState {
    const SEQ_TIMEOUT: Duration = Duration::from_millis(600);

    pub fn push(&mut self, ch: char) -> Option<(char, char)> {
        let now = Instant::now();
        if let Some((prev, at)) = self.pending.take()
            && now.duration_since(at) <= Self::SEQ_TIMEOUT
        {
            return Some((prev, ch));
        }
        self.pending = Some((ch, now));
        None
    }

    pub fn clear(&mut self) {
        self.pending = None;
    }
}

/// Tracks `j` -> `k` within 200ms in insert mode so users can still type `j` normally.
#[derive(Debug, Default)]
pub struct JkEscapeTracker {
    pending_j_at: Option<Instant>,
}

impl JkEscapeTracker {
    const JK_WINDOW: Duration = Duration::from_millis(200);

    pub fn on_char(&mut self, ch: char) -> JkResult {
        let now = Instant::now();
        if let Some(at) = self.pending_j_at.take() {
            if ch == 'k' && now.duration_since(at) <= Self::JK_WINDOW {
                return JkResult::Escaped;
            }
            if ch == 'j' {
                self.pending_j_at = Some(now);
                return JkResult::FlushJAndHoldNewJ;
            }
            return JkResult::FlushJAndChar(ch);
        }

        if ch == 'j' {
            self.pending_j_at = Some(now);
            JkResult::HoldJ
        } else {
            JkResult::Char(ch)
        }
    }

    pub fn poll_expired(&mut self) -> bool {
        if let Some(at) = self.pending_j_at
            && Instant::now().duration_since(at) > Self::JK_WINDOW
        {
            self.pending_j_at = None;
            return true;
        }
        false
    }

    pub fn take_pending(&mut self) -> bool {
        self.pending_j_at.take().is_some()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum JkResult {
    HoldJ,
    Escaped,
    FlushJAndHoldNewJ,
    FlushJAndChar(char),
    Char(char),
}
