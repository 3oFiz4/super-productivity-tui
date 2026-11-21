use crate::config::KeybindMode;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct KeybindEntry {
    pub section: &'static str,
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
            section: "Movement",
            normal: "Up",
            nvim: "k",
            description: "Move up in list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Movement",
            normal: "Down",
            nvim: "j",
            description: "Move down in list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Movement",
            normal: "gg / G",
            nvim: "gg / G",
            description: "Jump to top / bottom of list",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Navigation",
            normal: "Tab",
            nvim: "Ctrl+h",
            description: "Switch section (Sidebar / Main)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Navigation",
            normal: "Shift+Tab",
            nvim: "Ctrl+l",
            description: "Switch section reverse (Sidebar / Main)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Navigation",
            normal: "ga",
            nvim: "ga",
            description: "Jump to Main area",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Navigation",
            normal: "gn",
            nvim: "gn",
            description: "Jump to Navigation",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Navigation",
            normal: "hn",
            nvim: "zn",
            description: "Hide / show navigation sidebar",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Navigation",
            normal: "/ (in Sidebar)",
            nvim: "/ (in Sidebar)",
            description: "Fuzzy-search sidebar (n/N next/prev, Esc/jk clear)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "Space / Enter",
            nvim: "o / l / Space",
            description: "Toggle task done / open nav item",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "a",
            nvim: "a",
            description: "Add task (or Project/Tag when in Sidebar)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "d",
            nvim: "d",
            description: "Delete task / Project / Tag / Visual selection",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "v",
            nvim: "v",
            description: "Visual mode: toggle task multi-select",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "V",
            nvim: "V",
            description: "Visual line mode: select task range (Esc/jk exit)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "e",
            nvim: "i / e",
            description: "Edit task inline (+proj, #tag, t1h, @due, !deadline)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "s",
            nvim: "s",
            description: "Show / hide subtasks for selected task",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "n",
            nvim: "K",
            description: "Show / hide task note",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "N",
            nvim: "N",
            description: "Open full Markdown note view",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "t",
            nvim: "t",
            description: "Start / stop timer on selected task",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "R",
            nvim: "R",
            description: "Reschedule date & time picker",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Tasks",
            normal: "/",
            nvim: "/",
            description: "Live fuzzy search tasks (title, project, tag)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Undo / Redo",
            normal: "u",
            nvim: "u",
            description: "Undo last task change (up to 100)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Undo / Redo",
            normal: "U",
            nvim: "Ctrl+r",
            description: "Redo last undone change",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: "p",
            nvim: "O",
            description: "Open Properties dropdown (task / project / tag)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: "Right-Click",
            nvim: "L",
            description: "Open Properties dropdown (task / project / tag)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":h",
            nvim: ":h",
            description: "Open searchable Help / Keybinds",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":s",
            nvim: ":s",
            description: "Open Settings (CLI configuration)",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":c",
            nvim: ":c",
            description: "Open Colorscheme & Color Picker",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":w",
            nvim: "H",
            description: "Open Worklog / History view",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":n",
            nvim: "C",
            description: "Open Counters & Simple Habits",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":p",
            nvim: "P",
            description: "Open Focus Timer (Pomodoro / Flowtime / Countdown)",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: ":r",
            nvim: "E",
            description: "Open Daily Summary & End-of-Day Review",
            is_nvim_highlight: true,
        },
        KeybindEntry {
            section: "Views & Modals",
            normal: "jk (in modals)",
            nvim: "jk (in modals)",
            description: "Alternative to Esc in modals, dropdowns, Visual & search",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "System",
            normal: "S",
            nvim: "S",
            description: "Sync now with Super Productivity",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "System",
            normal: ":vc / :vi / :vI",
            nvim: ":vc / :vi / :vI",
            description: "Version changelog / ignore release / ignore forever",
            is_nvim_highlight: false,
        },
        KeybindEntry {
            section: "System",
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
