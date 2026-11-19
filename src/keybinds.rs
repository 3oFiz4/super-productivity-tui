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
    pub fn display_keys(&self, mode: KeybindMode) -> Vec<&'static str> {
        match mode {
            KeybindMode::Normal => vec![self.normal],
            KeybindMode::Nvim => vec![self.nvim],
            KeybindMode::Both => {
                if self.normal == self.nvim {
                    vec![self.normal]
                } else {
                    vec![self.normal, self.nvim]
                }
            }
        }
    }
}

pub fn all_keybinds() -> Vec<KeybindEntry> {
    vec![
        KeybindEntry {
            normal: "Tab",
            nvim: "Ctrl+h",
            description: "Switch section (Sidebar / Main)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Shift+Tab",
            nvim: "Ctrl+l",
            description: "Switch section reverse (Sidebar / Main)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Up",
            nvim: "k",
            description: "Move up in list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Down",
            nvim: "j",
            description: "Move down in list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "ga",
            nvim: "ga",
            description: "Jump to Main area",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "gn",
            nvim: "gn",
            description: "Jump to Navigation",
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
            normal: "gg / G",
            nvim: "gg / G",
            description: "Jump to top / bottom of list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Enter",
            nvim: "l",
            description: "Toggle task done / open nav item",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "p",
            nvim: "L",
            description: "Open Properties (task / project / tag)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "s",
            nvim: "s",
            description: "Show / hide subtasks for selected task",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "N",
            nvim: "N",
            description: "Open full Markdown note view",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "/",
            nvim: "/",
            description: "Live fuzzy search tasks (title, project, tag)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "t",
            nvim: "t",
            description: "Start / stop timer on selected task",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "p",
            nvim: "O",
            description: "Open Properties menu (scoped focus)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            normal: "Right-Click",
            nvim: "Right-Click",
            description: "Open Properties menu (scoped focus)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "r (in Properties)",
            nvim: "i",
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
            normal: "u",
            nvim: "u",
            description: "Undo last task change",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: "U",
            nvim: "Ctrl+r",
            description: "Redo last undone change",
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
            normal: ":c",
            nvim: ":c",
            description: "Open Colorscheme / Theme editor",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            normal: ":w",
            nvim: "H",
            description: "Open Worklog / History view",
            is_nvim_highlight: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jk_sequence_escapes_in_order() {
        let mut tracker = JkEscapeTracker::default();
        assert_eq!(tracker.on_char('j'), JkResult::HoldJ);
        assert_eq!(tracker.on_char('k'), JkResult::Escaped);
    }

    #[test]
    fn kj_does_not_escape() {
        let mut tracker = JkEscapeTracker::default();
        assert_eq!(tracker.on_char('k'), JkResult::Char('k'));
        assert_eq!(tracker.on_char('j'), JkResult::HoldJ);
    }

    #[test]
    fn j_followed_by_other_char_flushes() {
        let mut tracker = JkEscapeTracker::default();
        assert_eq!(tracker.on_char('j'), JkResult::HoldJ);
        assert_eq!(tracker.on_char('a'), JkResult::FlushJAndChar('a'));
    }
}
