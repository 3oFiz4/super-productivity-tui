use crate::api::SuperProductivityClient;
use crate::config::{AppConfig, KeybindMode};
use crate::keybinds::{JkEscapeTracker, JkResult, SequenceState};
use crate::models::{CreateTaskPayload, Project, Tag, Task};
use crate::theme::ThemeConfig;
use chrono::{Duration as ChronoDuration, Local};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use serde_json::json;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSection {
    Nav,
    Main,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavFilter {
    Today,
    Dashboard,
    Inbox,
    Upcoming,
    Project(String),
    Tag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavItemKind {
    Filter(NavFilter),
    ToggleProjects,
    ToggleTags,
    Help,
    Settings,
}

#[derive(Debug, Clone)]
pub struct NavRow {
    pub label: String,
    pub count: Option<usize>,
    pub kind: NavItemKind,
    pub indent: u16,
    pub icon: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    Properties,
    Help,
    Settings,
    AddTask,
    Theme,
    FullNote,
    AddEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddEntityKind {
    Project,
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddEntityField {
    Name,
    Color,
}

#[derive(Debug, Clone)]
pub struct AddEntityState {
    pub kind: AddEntityKind,
    pub title: String,
    pub color: String,
    pub active_field: AddEntityField,
}

impl AddEntityState {
    pub fn new(kind: AddEntityKind) -> Self {
        Self {
            kind,
            title: String::new(),
            color: if kind == AddEntityKind::Project {
                "#89b4fa".to_string()
            } else {
                "#a6e3a1".to_string()
            },
            active_field: AddEntityField::Name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertiesTarget {
    Task(String),
    Project(String),
    Tag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertiesMode {
    Menu,
    MoveToProject { selected: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatOption {
    None,
    Daily,
    Weekly,
    Monthly,
}

impl RepeatOption {
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Daily,
            Self::Daily => Self::Weekly,
            Self::Weekly => Self::Monthly,
            Self::Monthly => Self::None,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::None => Self::Monthly,
            Self::Daily => Self::None,
            Self::Weekly => Self::Daily,
            Self::Monthly => Self::Weekly,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    Tag,
    Time,
}

#[derive(Debug, Clone)]
pub struct CompletionPopup {
    pub kind: CompletionKind,
    pub items: Vec<String>,
    pub selected: usize,
    pub trigger_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddTaskField {
    Title,
    Note,
    Position,
    Repeat,
    Project,
    Tags,
}

impl AddTaskField {
    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::Note,
            Self::Note => Self::Position,
            Self::Position => Self::Repeat,
            Self::Repeat => Self::Project,
            Self::Project => Self::Tags,
            Self::Tags => Self::Title,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Title => Self::Tags,
            Self::Note => Self::Title,
            Self::Position => Self::Note,
            Self::Repeat => Self::Position,
            Self::Project => Self::Repeat,
            Self::Tags => Self::Project,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddTaskState {
    pub title: String,
    pub note: String,
    pub add_to_top: bool,
    pub repeat: RepeatOption,
    pub project_idx: usize,
    pub selected_tag_ids: Vec<String>,
    pub tag_cursor: usize,
    pub parent_id: Option<String>,
    pub active_field: AddTaskField,
    pub completion: Option<CompletionPopup>,
}

impl Default for AddTaskState {
    fn default() -> Self {
        Self {
            title: String::new(),
            note: String::new(),
            add_to_top: false,
            repeat: RepeatOption::None,
            project_idx: 0,
            selected_tag_ids: Vec::new(),
            tag_cursor: 0,
            parent_id: None,
            active_field: AddTaskField::Title,
            completion: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    ApiUrl,
    ApiToken,
    KeybindMode,
    HintKeybind,
}

impl SettingsField {
    pub fn next(self) -> Self {
        match self {
            Self::ApiUrl => Self::ApiToken,
            Self::ApiToken => Self::KeybindMode,
            Self::KeybindMode => Self::HintKeybind,
            Self::HintKeybind => Self::ApiUrl,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::ApiUrl => Self::HintKeybind,
            Self::ApiToken => Self::ApiUrl,
            Self::KeybindMode => Self::ApiToken,
            Self::HintKeybind => Self::KeybindMode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub api_url: String,
    pub api_token: String,
    pub keybind_mode: KeybindMode,
    pub hint_keybind: bool,
    pub active_field: SettingsField,
}

impl From<&AppConfig> for SettingsState {
    fn from(cfg: &AppConfig) -> Self {
        Self {
            api_url: cfg.api_url.clone(),
            api_token: cfg.api_token.clone(),
            keybind_mode: cfg.keybind_mode,
            hint_keybind: cfg.hint_keybind,
            active_field: SettingsField::ApiUrl,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeEditorState {
    pub original: ThemeConfig,
    pub editing: ThemeConfig,
    pub preset_idx: usize,
    pub field_idx: usize,
}

impl ThemeEditorState {
    pub const FIELD_LABELS: [&'static str; 13] = [
        "Preset / Name",
        "Background (bg)",
        "Sidebar Bg",
        "Status Bg",
        "Foreground (fg)",
        "Accent",
        "Muted",
        "Border",
        "Success",
        "Warning",
        "Danger",
        "Selection Bg",
        "Hint Fg",
    ];

    pub fn new(current: &ThemeConfig, saved: &[ThemeConfig]) -> Self {
        let mut all = ThemeConfig::builtin_themes();
        all.extend(saved.iter().cloned());
        let preset_idx = all.iter().position(|t| t.name == current.name).unwrap_or(0);
        Self {
            original: current.clone(),
            editing: current.clone(),
            preset_idx,
            field_idx: 0,
        }
    }

    pub fn field_value_mut(&mut self) -> &mut String {
        match self.field_idx {
            0 => &mut self.editing.name,
            1 => &mut self.editing.bg,
            2 => &mut self.editing.sidebar_bg,
            3 => &mut self.editing.status_bg,
            4 => &mut self.editing.fg,
            5 => &mut self.editing.accent,
            6 => &mut self.editing.muted,
            7 => &mut self.editing.border,
            8 => &mut self.editing.success,
            9 => &mut self.editing.warning,
            10 => &mut self.editing.danger,
            11 => &mut self.editing.selection_bg,
            _ => &mut self.editing.hint_fg,
        }
    }

    pub fn field_value(&self, idx: usize) -> &str {
        match idx {
            0 => &self.editing.name,
            1 => &self.editing.bg,
            2 => &self.editing.sidebar_bg,
            3 => &self.editing.status_bg,
            4 => &self.editing.fg,
            5 => &self.editing.accent,
            6 => &self.editing.muted,
            7 => &self.editing.border,
            8 => &self.editing.success,
            9 => &self.editing.warning,
            10 => &self.editing.danger,
            11 => &self.editing.selection_bg,
            _ => &self.editing.hint_fg,
        }
    }
}

#[derive(Debug, Clone)]
pub enum UndoOp {
    ToggleDone {
        id: String,
        prev_done: bool,
    },
    RenameTask {
        id: String,
        prev_title: String,
        new_title: String,
    },
    ScheduleTask {
        id: String,
        prev_due: Option<String>,
        new_due: Option<String>,
    },
    MoveIndex {
        id: String,
        prev_idx: usize,
        new_idx: usize,
    },
    MoveProject {
        id: String,
        prev_project: Option<String>,
        new_project: Option<String>,
    },
    DeleteTask {
        task: Task,
        index: usize,
    },
    CreateTask {
        task: Task,
        to_top: bool,
    },
}

#[derive(Debug, Clone)]
pub enum RenameTarget {
    Task(String),
    Project(String),
    Tag(String),
}

#[derive(Debug)]
pub enum ApiEvent {
    Loaded {
        tasks: Vec<Task>,
        projects: Vec<Project>,
        tags: Vec<Tag>,
        current_task_id: Option<String>,
        connected: bool,
        message: String,
    },
    TaskUpdated(Task),
    TaskCreated(Task, bool, bool),
    TaskDeleted(String),
    CurrentTaskUpdated(Option<String>),
    ProjectCreated(Project),
    ProjectUpdated(Project),
    ProjectDeleted(String),
    TagCreated(Tag),
    TagUpdated(Tag),
    TagDeleted(String),
    Error(String),
}

#[derive(Default)]
pub struct UiAreas {
    pub nav: Rect,
    pub divider: Rect,
    pub main: Rect,
    pub nav_rows: Vec<(Rect, usize)>,
    pub task_rows: Vec<(Rect, usize)>,
}

pub struct App {
    pub config: AppConfig,
    pub client: SuperProductivityClient,
    pub should_quit: bool,
    pub focus: FocusSection,
    pub show_sidebar: bool,
    pub active_filter: NavFilter,
    pub projects_expanded: bool,
    pub tags_expanded: bool,
    pub nav_selected: usize,
    pub task_selected: usize,
    pub task_scroll: usize,

    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub tags: Vec<Tag>,
    pub connected: bool,
    pub status_message: String,

    pub show_note: bool,
    pub expanded_subtasks: HashSet<String>,
    pub search_active: bool,
    pub search_query: String,
    pub rename_target: Option<RenameTarget>,
    pub rename_buffer: String,
    pub jk_tracker: JkEscapeTracker,
    pub prop_jk_tracker: JkEscapeTracker,
    pub sequence: SequenceState,
    pub prop_sequence: SequenceState,

    pub current_task_id: Option<String>,
    pub timer_started_at: Option<Instant>,
    pub typed_key_display: String,
    pub typed_key_at: Option<Instant>,

    pub active_modal: ActiveModal,
    pub properties_target: Option<PropertiesTarget>,
    pub properties_mode: PropertiesMode,
    pub properties_anchor: Option<(u16, u16)>,
    pub help_search: String,
    pub settings_state: SettingsState,
    pub add_task_state: AddTaskState,
    pub add_entity_state: AddEntityState,
    pub theme_editor: Option<ThemeEditorState>,

    pub undo_stack: Vec<UndoOp>,
    pub redo_stack: Vec<UndoOp>,

    pub areas: UiAreas,
    pub api_tx: mpsc::UnboundedSender<ApiEvent>,
}

impl App {
    pub fn new(config: AppConfig, api_tx: mpsc::UnboundedSender<ApiEvent>) -> Self {
        let client = SuperProductivityClient::new(&config.api_url, &config.api_token);
        let settings_state = SettingsState::from(&config);
        Self {
            config,
            client,
            should_quit: false,
            focus: FocusSection::Main,
            show_sidebar: true,
            active_filter: NavFilter::Today,
            projects_expanded: true,
            tags_expanded: true,
            nav_selected: 0,
            task_selected: 0,
            task_scroll: 0,
            tasks: Vec::new(),
            projects: Vec::new(),
            tags: Vec::new(),
            connected: false,
            status_message: "Connecting...".to_string(),
            show_note: false,
            expanded_subtasks: HashSet::new(),
            search_active: false,
            search_query: String::new(),
            rename_target: None,
            rename_buffer: String::new(),
            jk_tracker: JkEscapeTracker::default(),
            prop_jk_tracker: JkEscapeTracker::default(),
            sequence: SequenceState::default(),
            prop_sequence: SequenceState::default(),
            current_task_id: None,
            timer_started_at: None,
            typed_key_display: String::new(),
            typed_key_at: None,
            active_modal: ActiveModal::None,
            properties_target: None,
            properties_mode: PropertiesMode::Menu,
            properties_anchor: None,
            help_search: String::new(),
            settings_state,
            add_task_state: AddTaskState::default(),
            add_entity_state: AddEntityState::new(AddEntityKind::Project),
            theme_editor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            areas: UiAreas::default(),
            api_tx,
        }
    }

    pub fn refresh_all(&self) {
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.health().await {
                Ok(h) if h.server == "up" && h.renderer_ready => {
                    let tasks = client.get_tasks().await;
                    let projects = client.get_projects().await.unwrap_or_default();
                    let tags = client.get_tags().await.unwrap_or_default();
                    let current = client.get_current_task().await.ok().flatten();
                    let current_task_id = current.map(|t| t.id);
                    match tasks {
                        Ok(tasks) => {
                            let _ = tx.send(ApiEvent::Loaded {
                                tasks,
                                projects,
                                tags,
                                current_task_id,
                                connected: true,
                                message: "Connected to Super Productivity".to_string(),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Loaded {
                                tasks: Vec::new(),
                                projects,
                                tags,
                                current_task_id: None,
                                connected: false,
                                message: format!("API error: {e}"),
                            });
                        }
                    }
                }
                Ok(h) => {
                    let _ = tx.send(ApiEvent::Loaded {
                        tasks: Vec::new(),
                        projects: Vec::new(),
                        tags: Vec::new(),
                        current_task_id: None,
                        connected: false,
                        message: format!("Server {} (ready={})", h.server, h.renderer_ready),
                    });
                }
                Err(e) => {
                    let _ = tx.send(ApiEvent::Loaded {
                        tasks: Vec::new(),
                        projects: Vec::new(),
                        tags: Vec::new(),
                        current_task_id: None,
                        connected: false,
                        message: format!("Offline: {e}"),
                    });
                }
            }
        });
    }

    pub fn handle_api_event(&mut self, event: ApiEvent) {
        match event {
            ApiEvent::Loaded {
                tasks,
                projects,
                tags,
                current_task_id,
                connected,
                message,
            } => {
                self.tasks = tasks;
                self.projects = projects;
                self.tags = tags;
                self.timer_started_at = current_task_id.as_ref().map(|_| Instant::now());
                self.current_task_id = current_task_id;
                self.connected = connected;
                self.status_message = message;
                self.clamp_selections();
            }
            ApiEvent::CurrentTaskUpdated(cid) => {
                self.timer_started_at = cid.as_ref().map(|_| Instant::now());
                self.current_task_id = cid;
            }
            ApiEvent::TaskUpdated(updated) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == updated.id) {
                    *t = updated;
                }
                self.status_message = "Task updated".to_string();
            }
            ApiEvent::TaskCreated(created, to_top, record_undo) => {
                if record_undo {
                    self.push_undo(UndoOp::CreateTask {
                        task: created.clone(),
                        to_top,
                    });
                } else if let Some(UndoOp::DeleteTask { task, .. }) = self.redo_stack.last_mut() {
                    task.id = created.id.clone();
                }
                if to_top {
                    self.tasks.insert(0, created);
                    self.task_selected = 0;
                } else {
                    self.tasks.push(created);
                }
                self.status_message = "Task created".to_string();
                self.clamp_selections();
            }
            ApiEvent::TaskDeleted(id) => {
                self.tasks.retain(|t| t.id != id);
                self.status_message = "Task deleted".to_string();
                self.clamp_selections();
            }
            ApiEvent::ProjectCreated(p) => {
                self.projects.push(p);
                self.status_message = "Project created".to_string();
                self.clamp_selections();
            }
            ApiEvent::ProjectUpdated(updated) => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == updated.id) {
                    *p = updated;
                }
                self.status_message = "Project updated".to_string();
            }
            ApiEvent::ProjectDeleted(id) => {
                self.projects.retain(|p| p.id != id);
                self.status_message = "Project deleted".to_string();
                self.clamp_selections();
            }
            ApiEvent::TagCreated(t) => {
                self.tags.push(t);
                self.status_message = "Tag created".to_string();
                self.clamp_selections();
            }
            ApiEvent::TagUpdated(updated) => {
                if let Some(t) = self.tags.iter_mut().find(|t| t.id == updated.id) {
                    *t = updated;
                }
                self.status_message = "Tag updated".to_string();
            }
            ApiEvent::TagDeleted(id) => {
                self.tags.retain(|t| t.id != id);
                self.status_message = "Tag deleted".to_string();
                self.clamp_selections();
            }
            ApiEvent::Error(msg) => {
                self.status_message = msg;
            }
        }
    }

    pub fn nav_rows(&self) -> (Vec<NavRow>, Vec<NavRow>) {
        let mut top = vec![
            NavRow {
                label: "Today".to_string(),
                count: Some(self.count_for_filter(&NavFilter::Today)),
                kind: NavItemKind::Filter(NavFilter::Today),
                indent: 0,
                icon: "\u{f015}",
            },
            NavRow {
                label: "Dashboard".to_string(),
                count: Some(self.tasks.len()),
                kind: NavItemKind::Filter(NavFilter::Dashboard),
                indent: 0,
                icon: "\u{f0e4}",
            },
            NavRow {
                label: "Inbox".to_string(),
                count: Some(self.count_for_filter(&NavFilter::Inbox)),
                kind: NavItemKind::Filter(NavFilter::Inbox),
                indent: 0,
                icon: "\u{f01c}",
            },
            NavRow {
                label: if self.projects_expanded {
                    "[-] Projects".to_string()
                } else {
                    "[+] Projects".to_string()
                },
                count: None,
                kind: NavItemKind::ToggleProjects,
                indent: 0,
                icon: "\u{f07b}",
            },
        ];

        if self.projects_expanded {
            for proj in &self.projects {
                let f = NavFilter::Project(proj.id.clone());
                top.push(NavRow {
                    label: proj.title.clone(),
                    count: Some(self.count_for_filter(&f)),
                    kind: NavItemKind::Filter(f),
                    indent: 2,
                    icon: "\u{f07c}",
                });
            }
        }

        top.push(NavRow {
            label: if self.tags_expanded {
                "[-] Tags".to_string()
            } else {
                "[+] Tags".to_string()
            },
            count: None,
            kind: NavItemKind::ToggleTags,
            indent: 0,
            icon: "\u{f02b}",
        });

        if self.tags_expanded {
            for tag in &self.tags {
                let f = NavFilter::Tag(tag.id.clone());
                top.push(NavRow {
                    label: tag.title.clone(),
                    count: Some(self.count_for_filter(&f)),
                    kind: NavItemKind::Filter(f),
                    indent: 2,
                    icon: "\u{f292}",
                });
            }
        }

        let bottom = vec![
            NavRow {
                label: "Upcoming".to_string(),
                count: Some(self.count_for_filter(&NavFilter::Upcoming)),
                kind: NavItemKind::Filter(NavFilter::Upcoming),
                indent: 0,
                icon: "\u{f0c9}",
            },
            NavRow {
                label: "Help".to_string(),
                count: None,
                kind: NavItemKind::Help,
                indent: 0,
                icon: "\u{f059}",
            },
            NavRow {
                label: "Settings".to_string(),
                count: None,
                kind: NavItemKind::Settings,
                indent: 0,
                icon: "\u{f013}",
            },
        ];

        (top, bottom)
    }

    pub fn all_nav_rows(&self) -> Vec<NavRow> {
        let (mut top, bottom) = self.nav_rows();
        top.extend(bottom);
        top
    }

    fn count_for_filter(&self, filter: &NavFilter) -> usize {
        self.tasks
            .iter()
            .filter(|t| Self::matches_filter(t, filter))
            .count()
    }

    fn matches_filter(task: &Task, filter: &NavFilter) -> bool {
        match filter {
            NavFilter::Today | NavFilter::Dashboard => true,
            NavFilter::Inbox => task.project_id.is_none(),
            NavFilter::Upcoming => task.due_day.is_some(),
            NavFilter::Project(pid) => task.project_id.as_deref() == Some(pid.as_str()),
            NavFilter::Tag(tid) => task.tag_ids.iter().any(|id| id == tid),
        }
    }

    pub fn filtered_tasks(&self) -> Vec<&Task> {
        let q = self.search_query.trim().to_lowercase();
        self.tasks
            .iter()
            .filter(|t| t.parent_id.is_none() && Self::matches_filter(t, &self.active_filter))
            .filter(|t| {
                if q.is_empty() {
                    return true;
                }
                if fuzzy_match(&t.title.to_lowercase(), &q) {
                    return true;
                }
                if let Some(pid) = &t.project_id
                    && let Some(p) = self.projects.iter().find(|p| &p.id == pid)
                    && fuzzy_match(&p.title.to_lowercase(), &q)
                {
                    return true;
                }
                t.tag_ids.iter().any(|tid| {
                    self.tags
                        .iter()
                        .find(|tg| &tg.id == tid)
                        .is_some_and(|tg| fuzzy_match(&tg.title.to_lowercase(), &q))
                })
            })
            .collect()
    }

    pub fn subtasks_for(&self, parent_id: &str) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent_id))
            .collect()
    }

    pub fn visible_task_rows(&self) -> Vec<(&Task, bool)> {
        let mut rows = Vec::new();
        for task in self.filtered_tasks() {
            rows.push((task, false));
            if self.expanded_subtasks.contains(&task.id) {
                for sub in self.subtasks_for(&task.id) {
                    rows.push((sub, true));
                }
            }
        }
        rows
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.visible_task_rows()
            .get(self.task_selected)
            .map(|(t, _)| *t)
    }

    fn clamp_selections(&mut self) {
        let nav_len = self.all_nav_rows().len();
        if nav_len == 0 {
            self.nav_selected = 0;
        } else if self.nav_selected >= nav_len {
            self.nav_selected = nav_len - 1;
        }

        let task_len = self.visible_task_rows().len();
        if task_len == 0 {
            self.task_selected = 0;
            self.task_scroll = 0;
        } else if self.task_selected >= task_len {
            self.task_selected = task_len - 1;
        }
    }

    pub fn tick(&mut self) {
        if self.rename_target.is_some() && self.jk_tracker.poll_expired() {
            self.rename_buffer.push('j');
        }
        if self.active_modal == ActiveModal::Properties {
            self.prop_jk_tracker.poll_expired();
        }
        if self.sequence.pending.is_none()
            && let Some(at) = self.typed_key_at
            && Instant::now().duration_since(at) > Duration::from_millis(1500)
        {
            self.typed_key_display.clear();
            self.typed_key_at = None;
        }
    }

    fn record_typed_key(&mut self, key: &KeyEvent) {
        let label = match key.code {
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                format!("<C-{c}>")
            }
            KeyCode::Char('\u{12}') => "<C-r>".to_string(),
            KeyCode::Char(c) => {
                if let Some((p, _)) = self.sequence.pending {
                    format!("{p}{c}")
                } else {
                    c.to_string()
                }
            }
            KeyCode::Enter => "<CR>".to_string(),
            KeyCode::Tab => "<Tab>".to_string(),
            KeyCode::BackTab => "<S-Tab>".to_string(),
            KeyCode::Up => "<Up>".to_string(),
            KeyCode::Down => "<Down>".to_string(),
            KeyCode::Esc => "<Esc>".to_string(),
            _ => return,
        };
        self.typed_key_display = label;
        self.typed_key_at = Some(Instant::now());
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if self.rename_target.is_some() {
            self.handle_rename_key(key);
            return;
        }

        match self.active_modal {
            ActiveModal::Properties => {
                self.handle_properties_key(key);
                return;
            }
            ActiveModal::Help => {
                self.handle_help_key(key);
                return;
            }
            ActiveModal::Settings => {
                self.handle_settings_key(key);
                return;
            }
            ActiveModal::AddTask => {
                self.handle_add_task_key(key);
                return;
            }
            ActiveModal::Theme => {
                self.handle_theme_key(key);
                return;
            }
            ActiveModal::FullNote => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('N') | KeyCode::Char('q')
                ) {
                    self.active_modal = ActiveModal::None;
                }
                return;
            }
            ActiveModal::AddEntity => {
                self.handle_add_entity_key(key);
                return;
            }
            ActiveModal::None => {}
        }

        if self.search_active {
            match key.code {
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.clamp_selections();
                }
                KeyCode::Enter => {
                    self.search_active = false;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.clamp_selections();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.clamp_selections();
                }
                _ => {}
            }
            return;
        }

        let mode = self.config.keybind_mode;
        self.record_typed_key(&key);

        if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r'))
            || key.code == KeyCode::Char('\u{12}')
        {
            if mode.is_nvim_active() {
                self.sequence.clear();
                self.redo();
            }
            return;
        }

        if key.code == KeyCode::Tab && mode.is_normal_active() {
            self.sequence.clear();
            self.toggle_focus();
            return;
        }
        if key.code == KeyCode::BackTab && mode.is_normal_active() {
            self.sequence.clear();
            self.toggle_focus();
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('h') | KeyCode::Char('l'))
            && mode == KeybindMode::Nvim
        {
            self.sequence.clear();
            self.toggle_focus();
            return;
        }

        if mode.is_normal_active() {
            match key.code {
                KeyCode::Up => {
                    self.sequence.clear();
                    self.move_up();
                    return;
                }
                KeyCode::Down => {
                    self.sequence.clear();
                    self.move_down();
                    return;
                }
                KeyCode::Enter => {
                    self.sequence.clear();
                    self.activate_selected();
                    return;
                }
                _ => {}
            }
        }

        if let KeyCode::Char(ch) = key.code {
            if let Some((first, second)) = self.sequence.push(ch) {
                match (first, second) {
                    (':', 'h') => {
                        self.sequence.clear();
                        self.help_search.clear();
                        self.active_modal = ActiveModal::Help;
                        return;
                    }
                    (':', 's') => {
                        self.sequence.clear();
                        self.settings_state = SettingsState::from(&self.config);
                        self.active_modal = ActiveModal::Settings;
                        return;
                    }
                    (':', 'c') => {
                        self.sequence.clear();
                        self.theme_editor = Some(ThemeEditorState::new(
                            &self.config.theme,
                            &self.config.saved_themes,
                        ));
                        self.active_modal = ActiveModal::Theme;
                        return;
                    }
                    ('g', 'g') => {
                        self.sequence.clear();
                        self.move_to_top();
                        return;
                    }
                    ('g', 'a') => {
                        self.sequence.clear();
                        self.focus = FocusSection::Main;
                        return;
                    }
                    ('g', 'n') => {
                        self.sequence.clear();
                        self.show_sidebar = true;
                        self.focus = FocusSection::Nav;
                        return;
                    }
                    ('h', 'n') if mode.is_normal_active() => {
                        self.sequence.clear();
                        self.show_sidebar = !self.show_sidebar;
                        if !self.show_sidebar {
                            self.focus = FocusSection::Main;
                        }
                        return;
                    }
                    ('z', 'n') if mode == KeybindMode::Nvim => {
                        self.sequence.clear();
                        self.show_sidebar = !self.show_sidebar;
                        if !self.show_sidebar {
                            self.focus = FocusSection::Main;
                        }
                        return;
                    }
                    _ => {}
                }
            }

            if matches!(ch, ':' | 'g' | 'h' | 'z') {
                return;
            }

            match ch {
                'q' => self.should_quit = true,
                'G' => self.move_to_bottom(),
                'j' if mode.is_nvim_active() => self.move_down(),
                'k' if mode.is_nvim_active() => self.move_up(),
                'o' if mode.is_nvim_active() => self.activate_selected(),
                'l' if mode.is_nvim_active() => self.activate_selected(),
                't' => self.toggle_timer(),
                's' => self.toggle_subtasks(),
                '/' => {
                    self.search_active = true;
                }
                'N' => {
                    if self.selected_task().is_some() {
                        self.active_modal = ActiveModal::FullNote;
                    }
                }
                'p' if mode.is_normal_active() => self.open_properties(),
                'O' | 'L' if mode.is_nvim_active() => self.open_properties(),
                'i' if mode.is_nvim_active() => self.start_rename_selected_task(),
                'a' => self.handle_add_shortcut(),
                'd' => self.delete_selected_task(),
                'n' if mode.is_normal_active() => self.show_note = !self.show_note,
                'K' if mode.is_nvim_active() => self.show_note = !self.show_note,
                'u' => self.undo(),
                'U' if mode.is_normal_active() => self.redo(),
                _ => {}
            }
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.active_modal != ActiveModal::None || self.rename_target.is_some() {
            return;
        }
        let col = mouse.column;
        let row = mouse.row;
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.areas.divider.contains((col, row).into()) {
                    self.show_sidebar = !self.show_sidebar;
                    return;
                }
                for (rect, idx) in &self.areas.nav_rows {
                    if rect.contains((col, row).into()) {
                        self.focus = FocusSection::Nav;
                        self.nav_selected = *idx;
                        self.activate_selected();
                        return;
                    }
                }
                for (rect, idx) in &self.areas.task_rows {
                    if rect.contains((col, row).into()) {
                        self.focus = FocusSection::Main;
                        self.task_selected = *idx;
                        self.activate_selected();
                        return;
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                for (rect, idx) in &self.areas.nav_rows {
                    if rect.contains((col, row).into()) {
                        self.focus = FocusSection::Nav;
                        self.nav_selected = *idx;
                        self.open_properties();
                        return;
                    }
                }
                for (rect, idx) in &self.areas.task_rows {
                    if rect.contains((col, row).into()) {
                        self.focus = FocusSection::Main;
                        self.task_selected = *idx;
                        self.open_properties();
                        return;
                    }
                }
            }
            MouseEventKind::ScrollUp => self.move_up(),
            MouseEventKind::ScrollDown => self.move_down(),
            _ => {}
        }
    }

    fn toggle_focus(&mut self) {
        if !self.show_sidebar {
            self.focus = FocusSection::Main;
            return;
        }
        self.focus = match self.focus {
            FocusSection::Nav => FocusSection::Main,
            FocusSection::Main => FocusSection::Nav,
        };
    }

    fn move_up(&mut self) {
        match self.focus {
            FocusSection::Nav => {
                if self.nav_selected > 0 {
                    self.nav_selected -= 1;
                }
            }
            FocusSection::Main => {
                if self.task_selected > 0 {
                    self.task_selected -= 1;
                }
            }
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            FocusSection::Nav => {
                let len = self.all_nav_rows().len();
                if self.nav_selected + 1 < len {
                    self.nav_selected += 1;
                }
            }
            FocusSection::Main => {
                let len = self.visible_task_rows().len();
                if self.task_selected + 1 < len {
                    self.task_selected += 1;
                }
            }
        }
    }

    fn move_to_top(&mut self) {
        match self.focus {
            FocusSection::Nav => self.nav_selected = 0,
            FocusSection::Main => {
                self.task_selected = 0;
                self.task_scroll = 0;
            }
        }
    }

    fn move_to_bottom(&mut self) {
        match self.focus {
            FocusSection::Nav => {
                self.nav_selected = self.all_nav_rows().len().saturating_sub(1);
            }
            FocusSection::Main => {
                self.task_selected = self.visible_task_rows().len().saturating_sub(1);
            }
        }
    }

    fn toggle_subtasks(&mut self) {
        if self.focus != FocusSection::Main {
            return;
        }
        if let Some(t) = self.selected_task() {
            let id = t.id.clone();
            if !self.expanded_subtasks.remove(&id) {
                self.expanded_subtasks.insert(id);
            }
        }
    }

    fn default_add_task_state(&self) -> AddTaskState {
        let mut st = AddTaskState::default();
        match &self.active_filter {
            NavFilter::Project(pid) => {
                if let Some(idx) = self.projects.iter().position(|p| &p.id == pid) {
                    st.project_idx = idx + 1;
                }
            }
            NavFilter::Tag(tid) => {
                st.selected_tag_ids.push(tid.clone());
            }
            _ => {}
        }
        st
    }

    fn handle_add_shortcut(&mut self) {
        if self.focus == FocusSection::Nav
            && let Some(row) = self.all_nav_rows().get(self.nav_selected)
        {
            match &row.kind {
                NavItemKind::ToggleProjects | NavItemKind::Filter(NavFilter::Project(_)) => {
                    self.add_entity_state = AddEntityState::new(AddEntityKind::Project);
                    self.active_modal = ActiveModal::AddEntity;
                    return;
                }
                NavItemKind::ToggleTags | NavItemKind::Filter(NavFilter::Tag(_)) => {
                    self.add_entity_state = AddEntityState::new(AddEntityKind::Tag);
                    self.active_modal = ActiveModal::AddEntity;
                    return;
                }
                _ => {}
            }
        }
        self.add_task_state = self.default_add_task_state();
        self.active_modal = ActiveModal::AddTask;
    }

    fn activate_selected(&mut self) {
        match self.focus {
            FocusSection::Nav => {
                if let Some(row) = self.all_nav_rows().get(self.nav_selected).cloned() {
                    match row.kind {
                        NavItemKind::Filter(f) => {
                            self.active_filter = f;
                            self.task_selected = 0;
                        }
                        NavItemKind::ToggleProjects => {
                            self.projects_expanded = !self.projects_expanded;
                        }
                        NavItemKind::ToggleTags => {
                            self.tags_expanded = !self.tags_expanded;
                        }
                        NavItemKind::Help => {
                            self.help_search.clear();
                            self.active_modal = ActiveModal::Help;
                        }
                        NavItemKind::Settings => {
                            self.settings_state = SettingsState::from(&self.config);
                            self.active_modal = ActiveModal::Settings;
                        }
                    }
                }
            }
            FocusSection::Main => {
                if let Some(task) = self.selected_task().cloned() {
                    let next_done = !task.is_done;
                    self.push_undo(UndoOp::ToggleDone {
                        id: task.id.clone(),
                        prev_done: task.is_done,
                    });
                    if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task.id) {
                        t.is_done = next_done;
                    }
                    let client = self.client.clone();
                    let tx = self.api_tx.clone();
                    tokio::spawn(async move {
                        match client.toggle_done(&task.id, next_done).await {
                            Ok(t) => {
                                let _ = tx.send(ApiEvent::TaskUpdated(t));
                            }
                            Err(e) => {
                                let _ = tx.send(ApiEvent::Error(format!("Update failed: {e}")));
                            }
                        }
                    });
                }
            }
        }
    }

    fn open_properties(&mut self) {
        self.prop_sequence.clear();
        self.prop_jk_tracker = JkEscapeTracker::default();
        self.properties_mode = PropertiesMode::Menu;
        match self.focus {
            FocusSection::Main => {
                if let Some(t) = self.selected_task() {
                    self.properties_target = Some(PropertiesTarget::Task(t.id.clone()));
                    self.properties_anchor = self
                        .areas
                        .task_rows
                        .iter()
                        .find(|(_, idx)| *idx == self.task_selected)
                        .map(|(r, _)| (r.x + 2, r.y + 1));
                    self.active_modal = ActiveModal::Properties;
                }
            }
            FocusSection::Nav => {
                if let Some(row) = self.all_nav_rows().get(self.nav_selected) {
                    let anchor = self
                        .areas
                        .nav_rows
                        .iter()
                        .find(|(_, idx)| *idx == self.nav_selected)
                        .map(|(r, _)| (r.x + 2, r.y + 1));
                    match &row.kind {
                        NavItemKind::Filter(NavFilter::Project(pid)) => {
                            self.properties_target = Some(PropertiesTarget::Project(pid.clone()));
                            self.properties_anchor = anchor;
                            self.active_modal = ActiveModal::Properties;
                        }
                        NavItemKind::Filter(NavFilter::Tag(tid)) => {
                            self.properties_target = Some(PropertiesTarget::Tag(tid.clone()));
                            self.properties_anchor = anchor;
                            self.active_modal = ActiveModal::Properties;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn start_rename_selected_task(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.active_modal = ActiveModal::None;
            self.rename_target = Some(RenameTarget::Task(task.id));
            self.rename_buffer = task.title;
            self.jk_tracker = JkEscapeTracker::default();
        }
    }

    fn delete_selected_task(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            let idx = self.tasks.iter().position(|t| t.id == task.id).unwrap_or(0);
            self.push_undo(UndoOp::DeleteTask {
                task: task.clone(),
                index: idx,
            });
            self.tasks.retain(|t| t.id != task.id);
            self.clamp_selections();
            let client = self.client.clone();
            let tx = self.api_tx.clone();
            tokio::spawn(async move {
                match client.delete_task(&task.id).await {
                    Ok(()) => {
                        let _ = tx.send(ApiEvent::TaskDeleted(task.id));
                    }
                    Err(e) => {
                        let _ = tx.send(ApiEvent::Error(format!("Delete failed: {e}")));
                    }
                }
            });
        }
    }

    fn toggle_timer(&mut self) {
        let Some(task) = self.selected_task().cloned() else {
            return;
        };
        let next_id = if self.current_task_id.as_deref() == Some(task.id.as_str()) {
            None
        } else {
            Some(task.id.clone())
        };
        self.timer_started_at = next_id.as_ref().map(|_| Instant::now());
        self.current_task_id = next_id.clone();
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.set_current_task(next_id.as_deref()).await {
                Ok(()) => {
                    let _ = tx.send(ApiEvent::CurrentTaskUpdated(next_id));
                }
                Err(e) => {
                    let _ = tx.send(ApiEvent::Error(format!("Timer failed: {e}")));
                }
            }
        });
    }

    fn push_undo(&mut self, op: UndoOp) {
        while self.undo_stack.len() >= 100 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(op);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        let Some(op) = self.undo_stack.pop() else {
            self.status_message = "Nothing to undo".to_string();
            return;
        };
        match &op {
            UndoOp::ToggleDone { id, prev_done } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.is_done = *prev_done;
                }
                self.spawn_patch(id.clone(), json!({ "isDone": prev_done }));
            }
            UndoOp::RenameTask { id, prev_title, .. } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.title = prev_title.clone();
                }
                self.spawn_patch(id.clone(), json!({ "title": prev_title }));
            }
            UndoOp::ScheduleTask { id, prev_due, .. } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.due_day = prev_due.clone();
                }
                self.spawn_patch(id.clone(), json!({ "dueDay": prev_due }));
            }
            UndoOp::MoveIndex {
                id,
                prev_idx,
                new_idx: _,
            } => {
                if let Some(cur) = self.tasks.iter().position(|t| t.id == *id) {
                    let item = self.tasks.remove(cur);
                    let target = (*prev_idx).min(self.tasks.len());
                    self.tasks.insert(target, item);
                }
            }
            UndoOp::MoveProject {
                id, prev_project, ..
            } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.project_id = prev_project.clone();
                }
                self.spawn_patch(id.clone(), json!({ "projectId": prev_project }));
            }
            UndoOp::DeleteTask { task, index } => {
                let target = (*index).min(self.tasks.len());
                let payload = CreateTaskPayload {
                    title: task.title.clone(),
                    notes: task.notes.clone(),
                    project_id: task.project_id.clone(),
                    parent_id: task.parent_id.clone(),
                    tag_ids: task.tag_ids.clone(),
                    due_day: task.due_day.clone(),
                    time_estimate: task.time_estimate,
                    is_add_to_top: Some(target == 0),
                };
                self.spawn_create(payload, target == 0, false);
            }
            UndoOp::CreateTask { task, .. } => {
                let id = task.id.clone();
                self.tasks.retain(|t| t.id != id);
                self.spawn_delete(id);
            }
        }
        self.redo_stack.push(op);
        self.status_message = "Undone".to_string();
        self.clamp_selections();
    }

    pub fn redo(&mut self) {
        let Some(op) = self.redo_stack.pop() else {
            self.status_message = "Nothing to redo".to_string();
            return;
        };
        match &op {
            UndoOp::ToggleDone { id, prev_done } => {
                let next_done = !prev_done;
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.is_done = next_done;
                }
                self.spawn_patch(id.clone(), json!({ "isDone": next_done }));
            }
            UndoOp::RenameTask { id, new_title, .. } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.title = new_title.clone();
                }
                self.spawn_patch(id.clone(), json!({ "title": new_title }));
            }
            UndoOp::ScheduleTask { id, new_due, .. } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.due_day = new_due.clone();
                }
                self.spawn_patch(id.clone(), json!({ "dueDay": new_due }));
            }
            UndoOp::MoveIndex { id, new_idx, .. } => {
                if let Some(cur) = self.tasks.iter().position(|t| t.id == *id) {
                    let item = self.tasks.remove(cur);
                    let target = (*new_idx).min(self.tasks.len());
                    self.tasks.insert(target, item);
                }
            }
            UndoOp::MoveProject {
                id, new_project, ..
            } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.project_id = new_project.clone();
                }
                self.spawn_patch(id.clone(), json!({ "projectId": new_project }));
            }
            UndoOp::DeleteTask { task, .. } => {
                let id = task.id.clone();
                self.tasks.retain(|t| t.id != id);
                self.spawn_delete(id);
            }
            UndoOp::CreateTask { task, to_top } => {
                let payload = CreateTaskPayload {
                    title: task.title.clone(),
                    notes: task.notes.clone(),
                    project_id: task.project_id.clone(),
                    parent_id: task.parent_id.clone(),
                    tag_ids: task.tag_ids.clone(),
                    due_day: task.due_day.clone(),
                    time_estimate: task.time_estimate,
                    is_add_to_top: Some(*to_top),
                };
                self.spawn_create(payload, *to_top, false);
            }
        }
        self.undo_stack.push(op);
        self.status_message = "Redone".to_string();
        self.clamp_selections();
    }

    fn spawn_patch(&self, id: String, patch: serde_json::Value) {
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.update_task(&id, patch).await {
                Ok(t) => {
                    let _ = tx.send(ApiEvent::TaskUpdated(t));
                }
                Err(e) => {
                    let _ = tx.send(ApiEvent::Error(format!("Update failed: {e}")));
                }
            }
        });
    }

    fn spawn_create(&self, payload: CreateTaskPayload, to_top: bool, record_undo: bool) {
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.create_task(payload).await {
                Ok(t) => {
                    let _ = tx.send(ApiEvent::TaskCreated(t, to_top, record_undo));
                }
                Err(e) => {
                    let _ = tx.send(ApiEvent::Error(format!("Create failed: {e}")));
                }
            }
        });
    }

    fn spawn_delete(&self, id: String) {
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.delete_task(&id).await {
                Ok(()) => {
                    let _ = tx.send(ApiEvent::TaskDeleted(id));
                }
                Err(e) => {
                    let _ = tx.send(ApiEvent::Error(format!("Delete failed: {e}")));
                }
            }
        });
    }

    fn handle_properties_key(&mut self, key: KeyEvent) {
        if let PropertiesMode::MoveToProject { ref mut selected } = self.properties_mode {
            let max_idx = self.projects.len();
            match key.code {
                KeyCode::Esc => {
                    self.properties_mode = PropertiesMode::Menu;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected < max_idx {
                        *selected += 1;
                    }
                }
                KeyCode::Enter => {
                    let sel = *selected;
                    let target_pid = if sel == 0 {
                        None
                    } else {
                        self.projects.get(sel - 1).map(|p| p.id.clone())
                    };
                    if let Some(PropertiesTarget::Task(tid)) = self.properties_target.clone()
                        && let Some(t) = self.tasks.iter_mut().find(|t| t.id == tid)
                    {
                        let prev = t.project_id.clone();
                        t.project_id = target_pid.clone();
                        self.push_undo(UndoOp::MoveProject {
                            id: tid.clone(),
                            prev_project: prev,
                            new_project: target_pid.clone(),
                        });
                        self.spawn_patch(tid, json!({ "projectId": target_pid }));
                    }
                    self.active_modal = ActiveModal::None;
                }
                _ => {}
            }
            return;
        }

        if key.code == KeyCode::Esc {
            self.prop_sequence.clear();
            self.prop_jk_tracker = JkEscapeTracker::default();
            self.active_modal = ActiveModal::None;
            return;
        }

        let Some(target) = self.properties_target.clone() else {
            return;
        };

        let KeyCode::Char(ch) = key.code else {
            return;
        };

        if self.prop_jk_tracker.on_char(ch) == JkResult::Escaped {
            self.prop_sequence.clear();
            self.active_modal = ActiveModal::None;
            return;
        }

        match target {
            PropertiesTarget::Project(pid) => match ch {
                'r' => {
                    if let Some(p) = self.projects.iter().find(|p| p.id == pid) {
                        self.active_modal = ActiveModal::None;
                        self.rename_target = Some(RenameTarget::Project(pid));
                        self.rename_buffer = p.title.clone();
                        self.jk_tracker = JkEscapeTracker::default();
                    }
                }
                'd' => {
                    self.active_modal = ActiveModal::None;
                    self.projects.retain(|p| p.id != pid);
                    let client = self.client.clone();
                    let tx = self.api_tx.clone();
                    tokio::spawn(async move {
                        match client.delete_project(&pid).await {
                            Ok(()) => {
                                let _ = tx.send(ApiEvent::ProjectDeleted(pid));
                            }
                            Err(e) => {
                                let _ =
                                    tx.send(ApiEvent::Error(format!("Delete project failed: {e}")));
                            }
                        }
                    });
                }
                _ => {}
            },
            PropertiesTarget::Tag(tid) => match ch {
                'r' => {
                    if let Some(t) = self.tags.iter().find(|t| t.id == tid) {
                        self.active_modal = ActiveModal::None;
                        self.rename_target = Some(RenameTarget::Tag(tid));
                        self.rename_buffer = t.title.clone();
                        self.jk_tracker = JkEscapeTracker::default();
                    }
                }
                'd' => {
                    self.active_modal = ActiveModal::None;
                    self.tags.retain(|t| t.id != tid);
                    let client = self.client.clone();
                    let tx = self.api_tx.clone();
                    tokio::spawn(async move {
                        match client.delete_tag(&tid).await {
                            Ok(()) => {
                                let _ = tx.send(ApiEvent::TagDeleted(tid));
                            }
                            Err(e) => {
                                let _ = tx.send(ApiEvent::Error(format!("Delete tag failed: {e}")));
                            }
                        }
                    });
                }
                _ => {}
            },
            PropertiesTarget::Task(tid) => {
                if let Some((first, second)) = self.prop_sequence.push(ch) {
                    match (first, second) {
                        ('s', 'd') => {
                            self.prop_sequence.clear();
                            let today = Local::now().format("%Y-%m-%d").to_string();
                            self.schedule_task(&tid, Some(today));
                            self.active_modal = ActiveModal::None;
                            return;
                        }
                        ('s', 'm') => {
                            self.prop_sequence.clear();
                            let tomorrow = (Local::now() + ChronoDuration::days(1))
                                .format("%Y-%m-%d")
                                .to_string();
                            self.schedule_task(&tid, Some(tomorrow));
                            self.active_modal = ActiveModal::None;
                            return;
                        }
                        ('s', 'n') => {
                            self.prop_sequence.clear();
                            let next_week = (Local::now() + ChronoDuration::days(7))
                                .format("%Y-%m-%d")
                                .to_string();
                            self.schedule_task(&tid, Some(next_week));
                            self.active_modal = ActiveModal::None;
                            return;
                        }
                        ('m', 't') => {
                            self.prop_sequence.clear();
                            self.move_task_edge(&tid, true);
                            self.active_modal = ActiveModal::None;
                            return;
                        }
                        ('m', 'b') => {
                            self.prop_sequence.clear();
                            self.move_task_edge(&tid, false);
                            self.active_modal = ActiveModal::None;
                            return;
                        }
                        ('m', 'p') => {
                            self.prop_sequence.clear();
                            self.properties_mode = PropertiesMode::MoveToProject { selected: 0 };
                            return;
                        }
                        ('a', 's') => {
                            self.prop_sequence.clear();
                            self.add_task_state = AddTaskState {
                                parent_id: Some(tid),
                                ..AddTaskState::default()
                            };
                            self.active_modal = ActiveModal::AddTask;
                            return;
                        }
                        _ => {}
                    }
                }

                if matches!(ch, 's' | 'm' | 'a') {
                    return;
                }

                self.prop_sequence.clear();
                match ch {
                    'r' => self.start_rename_selected_task(),
                    'u' => {
                        self.schedule_task(&tid, None);
                        self.active_modal = ActiveModal::None;
                    }
                    'R' => {
                        self.status_message = "Reschedule: not implemented yet".to_string();
                        self.active_modal = ActiveModal::None;
                    }
                    'd' => {
                        if let Some(orig) = self.tasks.iter().find(|t| t.id == tid).cloned() {
                            let payload = CreateTaskPayload {
                                title: format!("{} (copy)", orig.title),
                                notes: orig.notes,
                                project_id: orig.project_id,
                                parent_id: orig.parent_id,
                                tag_ids: orig.tag_ids,
                                due_day: orig.due_day,
                                time_estimate: orig.time_estimate,
                                is_add_to_top: Some(false),
                            };
                            self.spawn_create(payload, false, true);
                        }
                        self.active_modal = ActiveModal::None;
                    }
                    _ => {}
                }
            }
        }
    }

    fn schedule_task(&mut self, tid: &str, due_day: Option<String>) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == tid) {
            let prev_due = t.due_day.clone();
            t.due_day = due_day.clone();
            self.push_undo(UndoOp::ScheduleTask {
                id: tid.to_string(),
                prev_due,
                new_due: due_day.clone(),
            });
            self.spawn_patch(tid.to_string(), json!({ "dueDay": due_day }));
        }
    }

    fn move_task_edge(&mut self, tid: &str, to_top: bool) {
        if let Some(cur) = self.tasks.iter().position(|t| t.id == tid) {
            let item = self.tasks.remove(cur);
            let new_idx = if to_top { 0 } else { self.tasks.len() };
            self.tasks.insert(new_idx, item);
            self.push_undo(UndoOp::MoveIndex {
                id: tid.to_string(),
                prev_idx: cur,
                new_idx,
            });
            self.task_selected = if to_top {
                0
            } else {
                self.filtered_tasks().len().saturating_sub(1)
            };
        }
    }

    fn handle_rename_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if self.jk_tracker.take_pending() {
                    self.rename_buffer.push('j');
                }
                self.commit_rename();
            }
            KeyCode::Esc => {
                self.jk_tracker = JkEscapeTracker::default();
                self.rename_target = None;
                self.rename_buffer.clear();
            }
            KeyCode::Backspace => {
                if !self.jk_tracker.take_pending() {
                    self.rename_buffer.pop();
                }
            }
            KeyCode::Char(c) => match self.jk_tracker.on_char(c) {
                JkResult::HoldJ => {}
                JkResult::Escaped => self.commit_rename(),
                JkResult::FlushJAndHoldNewJ => self.rename_buffer.push('j'),
                JkResult::FlushJAndChar(ch) => {
                    self.rename_buffer.push('j');
                    self.rename_buffer.push(ch);
                }
                JkResult::Char(ch) => self.rename_buffer.push(ch),
            },
            _ => {}
        }
    }

    fn commit_rename(&mut self) {
        let Some(target) = self.rename_target.take() else {
            return;
        };
        let new_title = self.rename_buffer.trim().to_string();
        self.rename_buffer.clear();
        if new_title.is_empty() {
            return;
        }
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        match target {
            RenameTarget::Task(id) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                    let prev_title = t.title.clone();
                    t.title = new_title.clone();
                    self.push_undo(UndoOp::RenameTask {
                        id: id.clone(),
                        prev_title,
                        new_title: new_title.clone(),
                    });
                }
                tokio::spawn(async move {
                    match client.rename_task(&id, &new_title).await {
                        Ok(t) => {
                            let _ = tx.send(ApiEvent::TaskUpdated(t));
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Error(format!("Rename failed: {e}")));
                        }
                    }
                });
            }
            RenameTarget::Project(id) => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
                    p.title = new_title.clone();
                }
                tokio::spawn(async move {
                    match client.rename_project(&id, &new_title).await {
                        Ok(p) => {
                            let _ = tx.send(ApiEvent::ProjectUpdated(p));
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Error(format!("Rename project failed: {e}")));
                        }
                    }
                });
            }
            RenameTarget::Tag(id) => {
                if let Some(t) = self.tags.iter_mut().find(|t| t.id == id) {
                    t.title = new_title.clone();
                }
                tokio::spawn(async move {
                    match client.rename_tag(&id, &new_title).await {
                        Ok(t) => {
                            let _ = tx.send(ApiEvent::TagUpdated(t));
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Error(format!("Rename tag failed: {e}")));
                        }
                    }
                });
            }
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.active_modal = ActiveModal::None,
            KeyCode::Backspace => {
                self.help_search.pop();
            }
            KeyCode::Char(c) => self.help_search.push(c),
            _ => {}
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.active_modal = ActiveModal::None,
            KeyCode::Tab | KeyCode::Down => {
                self.settings_state.active_field = self.settings_state.active_field.next();
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.settings_state.active_field = self.settings_state.active_field.prev();
            }
            KeyCode::Enter => {
                self.config.api_url = self.settings_state.api_url.clone();
                self.config.api_token = self.settings_state.api_token.clone();
                self.config.keybind_mode = self.settings_state.keybind_mode;
                self.config.hint_keybind = self.settings_state.hint_keybind;
                let _ = self.config.save();
                self.client
                    .update_credentials(&self.config.api_url, &self.config.api_token);
                self.active_modal = ActiveModal::None;
                self.refresh_all();
            }
            KeyCode::Char(' ') => match self.settings_state.active_field {
                SettingsField::KeybindMode => {
                    self.settings_state.keybind_mode = self.settings_state.keybind_mode.next();
                }
                SettingsField::HintKeybind => {
                    self.settings_state.hint_keybind = !self.settings_state.hint_keybind;
                }
                SettingsField::ApiUrl => self.settings_state.api_url.push(' '),
                SettingsField::ApiToken => self.settings_state.api_token.push(' '),
            },
            KeyCode::Backspace => match self.settings_state.active_field {
                SettingsField::ApiUrl => {
                    self.settings_state.api_url.pop();
                }
                SettingsField::ApiToken => {
                    self.settings_state.api_token.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match self.settings_state.active_field {
                SettingsField::ApiUrl => self.settings_state.api_url.push(c),
                SettingsField::ApiToken => self.settings_state.api_token.push(c),
                SettingsField::KeybindMode => {
                    self.settings_state.keybind_mode = self.settings_state.keybind_mode.next();
                }
                SettingsField::HintKeybind => {
                    self.settings_state.hint_keybind = !self.settings_state.hint_keybind;
                }
            },
            _ => {}
        }
    }

    fn update_title_completion(&mut self) {
        if self.add_task_state.active_field != AddTaskField::Title {
            self.add_task_state.completion = None;
            return;
        }
        let title = &self.add_task_state.title;
        let last_word_start = title.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let token = &title[last_word_start..];
        if let Some(prefix) = token.strip_prefix('#') {
            let p_low = prefix.to_lowercase();
            let items: Vec<String> = self
                .tags
                .iter()
                .filter(|t| p_low.is_empty() || t.title.to_lowercase().contains(&p_low))
                .map(|t| t.title.clone())
                .collect();
            if items.is_empty() {
                self.add_task_state.completion = None;
            } else {
                self.add_task_state.completion = Some(CompletionPopup {
                    kind: CompletionKind::Tag,
                    items,
                    selected: 0,
                    trigger_start: last_word_start,
                });
            }
        } else if let Some(prefix) = token.strip_prefix('@') {
            let p_low = prefix.to_lowercase();
            let presets = ["today", "tomorrow", "15m", "30m", "1h", "2h"];
            let items: Vec<String> = presets
                .iter()
                .filter(|s| p_low.is_empty() || s.starts_with(&p_low))
                .map(|s| (*s).to_string())
                .collect();
            if items.is_empty() {
                self.add_task_state.completion = None;
            } else {
                self.add_task_state.completion = Some(CompletionPopup {
                    kind: CompletionKind::Time,
                    items,
                    selected: 0,
                    trigger_start: last_word_start,
                });
            }
        } else {
            self.add_task_state.completion = None;
        }
    }

    fn accept_completion(&mut self) {
        let Some(comp) = self.add_task_state.completion.take() else {
            return;
        };
        let Some(choice) = comp.items.get(comp.selected).cloned() else {
            return;
        };
        let prefix_char = match comp.kind {
            CompletionKind::Tag => {
                if let Some(tag) = self.tags.iter().find(|t| t.title == choice)
                    && !self.add_task_state.selected_tag_ids.contains(&tag.id)
                {
                    self.add_task_state.selected_tag_ids.push(tag.id.clone());
                }
                '#'
            }
            CompletionKind::Time => '@',
        };
        self.add_task_state.title.truncate(comp.trigger_start);
        self.add_task_state
            .title
            .push_str(&format!("{prefix_char}{choice} "));
    }

    fn parse_inline_tokens(
        &self,
        raw_title: &str,
    ) -> (String, Vec<String>, Option<String>, Option<u64>) {
        let mut clean_words = Vec::new();
        let mut tag_ids = self.add_task_state.selected_tag_ids.clone();
        let mut due_day = None;
        let mut time_estimate = None;

        for word in raw_title.split_whitespace() {
            if let Some(tag_name) = word.strip_prefix('#') {
                if let Some(tag) = self
                    .tags
                    .iter()
                    .find(|t| t.title.eq_ignore_ascii_case(tag_name))
                {
                    if !tag_ids.contains(&tag.id) {
                        tag_ids.push(tag.id.clone());
                    }
                } else {
                    clean_words.push(word);
                }
            } else if let Some(time_tok) = word.strip_prefix('@') {
                match time_tok.to_lowercase().as_str() {
                    "today" => due_day = Some(Local::now().format("%Y-%m-%d").to_string()),
                    "tomorrow" => {
                        due_day = Some(
                            (Local::now() + ChronoDuration::days(1))
                                .format("%Y-%m-%d")
                                .to_string(),
                        )
                    }
                    "15m" => time_estimate = Some(15 * 60_000),
                    "30m" => time_estimate = Some(30 * 60_000),
                    "1h" => time_estimate = Some(60 * 60_000),
                    "2h" => time_estimate = Some(120 * 60_000),
                    _ => clean_words.push(word),
                }
            } else {
                clean_words.push(word);
            }
        }
        (clean_words.join(" "), tag_ids, due_day, time_estimate)
    }

    fn handle_add_task_key(&mut self, key: KeyEvent) {
        if let Some(ref mut comp) = self.add_task_state.completion {
            match key.code {
                KeyCode::Esc => {
                    self.add_task_state.completion = None;
                    return;
                }
                KeyCode::Down => {
                    if comp.selected + 1 < comp.items.len() {
                        comp.selected += 1;
                    }
                    return;
                }
                KeyCode::Up => {
                    if comp.selected > 0 {
                        comp.selected -= 1;
                    }
                    return;
                }
                KeyCode::Tab => {
                    self.accept_completion();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => self.active_modal = ActiveModal::None,
            KeyCode::Tab | KeyCode::Down => {
                self.add_task_state.active_field = self.add_task_state.active_field.next();
                self.add_task_state.completion = None;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.add_task_state.active_field = self.add_task_state.active_field.prev();
                self.add_task_state.completion = None;
            }
            KeyCode::Enter => {
                let raw_title = self.add_task_state.title.trim().to_string();
                if raw_title.is_empty() {
                    return;
                }
                let (title, tag_ids, due_day, time_estimate) = self.parse_inline_tokens(&raw_title);
                if title.is_empty() {
                    return;
                }
                let mut notes = self.add_task_state.note.trim().to_string();
                if self.add_task_state.repeat != RepeatOption::None {
                    let rep_note = format!("Repeat: {}", self.add_task_state.repeat.label());
                    if notes.is_empty() {
                        notes = rep_note;
                    } else {
                        notes = format!("{notes}\n{rep_note}");
                    }
                }
                let project_id = if self.add_task_state.project_idx == 0 {
                    None
                } else {
                    self.projects
                        .get(self.add_task_state.project_idx - 1)
                        .map(|p| p.id.clone())
                };
                let to_top = self.add_task_state.add_to_top;
                let payload = CreateTaskPayload {
                    title: title.clone(),
                    notes: if notes.is_empty() { None } else { Some(notes) },
                    project_id,
                    parent_id: self.add_task_state.parent_id.clone(),
                    tag_ids,
                    due_day,
                    time_estimate,
                    is_add_to_top: Some(to_top),
                };
                self.active_modal = ActiveModal::None;
                let client = self.client.clone();
                let tx = self.api_tx.clone();
                tokio::spawn(async move {
                    match client.create_task(payload).await {
                        Ok(t) => {
                            let _ = tx.send(ApiEvent::TaskCreated(t, to_top, true));
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Error(format!("Create failed: {e}")));
                        }
                    }
                });
            }
            KeyCode::Left => match self.add_task_state.active_field {
                AddTaskField::Position => {
                    self.add_task_state.add_to_top = !self.add_task_state.add_to_top;
                }
                AddTaskField::Repeat => {
                    self.add_task_state.repeat = self.add_task_state.repeat.prev();
                }
                AddTaskField::Project if self.add_task_state.project_idx > 0 => {
                    self.add_task_state.project_idx -= 1;
                }
                AddTaskField::Tags if self.add_task_state.tag_cursor > 0 => {
                    self.add_task_state.tag_cursor -= 1;
                }
                _ => {}
            },
            KeyCode::Right => match self.add_task_state.active_field {
                AddTaskField::Position => {
                    self.add_task_state.add_to_top = !self.add_task_state.add_to_top;
                }
                AddTaskField::Repeat => {
                    self.add_task_state.repeat = self.add_task_state.repeat.next();
                }
                AddTaskField::Project if self.add_task_state.project_idx < self.projects.len() => {
                    self.add_task_state.project_idx += 1;
                }
                AddTaskField::Tags if self.add_task_state.tag_cursor + 1 < self.tags.len() => {
                    self.add_task_state.tag_cursor += 1;
                }
                _ => {}
            },
            KeyCode::Char(' ') => match self.add_task_state.active_field {
                AddTaskField::Position => {
                    self.add_task_state.add_to_top = !self.add_task_state.add_to_top;
                }
                AddTaskField::Repeat => {
                    self.add_task_state.repeat = self.add_task_state.repeat.next();
                }
                AddTaskField::Tags => {
                    if let Some(tag) = self.tags.get(self.add_task_state.tag_cursor) {
                        if let Some(pos) = self
                            .add_task_state
                            .selected_tag_ids
                            .iter()
                            .position(|id| id == &tag.id)
                        {
                            self.add_task_state.selected_tag_ids.remove(pos);
                        } else {
                            self.add_task_state.selected_tag_ids.push(tag.id.clone());
                        }
                    }
                }
                AddTaskField::Title => {
                    self.add_task_state.title.push(' ');
                    self.update_title_completion();
                }
                AddTaskField::Note => self.add_task_state.note.push(' '),
                AddTaskField::Project => {}
            },
            KeyCode::Backspace => match self.add_task_state.active_field {
                AddTaskField::Title => {
                    self.add_task_state.title.pop();
                    self.update_title_completion();
                }
                AddTaskField::Note => {
                    self.add_task_state.note.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match self.add_task_state.active_field {
                AddTaskField::Title => {
                    self.add_task_state.title.push(c);
                    self.update_title_completion();
                }
                AddTaskField::Note => self.add_task_state.note.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_theme_key(&mut self, key: KeyEvent) {
        let Some(ref mut ed) = self.theme_editor else {
            self.active_modal = ActiveModal::None;
            return;
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            let tpl = ed.editing.clone();
            if let Some(existing) = self
                .config
                .saved_themes
                .iter_mut()
                .find(|t| t.name == tpl.name)
            {
                *existing = tpl.clone();
            } else {
                self.config.saved_themes.push(tpl.clone());
            }
            self.config.theme = tpl;
            let _ = self.config.save();
            self.status_message = "Theme template saved".to_string();
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.config.theme = ed.original.clone();
                self.theme_editor = None;
                self.active_modal = ActiveModal::None;
            }
            KeyCode::Tab | KeyCode::Down => {
                ed.field_idx = (ed.field_idx + 1) % ThemeEditorState::FIELD_LABELS.len();
            }
            KeyCode::BackTab | KeyCode::Up => {
                ed.field_idx = if ed.field_idx == 0 {
                    ThemeEditorState::FIELD_LABELS.len() - 1
                } else {
                    ed.field_idx - 1
                };
            }
            KeyCode::Left if ed.field_idx == 0 => {
                let mut all = ThemeConfig::builtin_themes();
                all.extend(self.config.saved_themes.iter().cloned());
                if !all.is_empty() {
                    ed.preset_idx = if ed.preset_idx == 0 {
                        all.len() - 1
                    } else {
                        ed.preset_idx - 1
                    };
                    ed.editing = all[ed.preset_idx].clone();
                    self.config.theme = ed.editing.clone();
                }
            }
            KeyCode::Right if ed.field_idx == 0 => {
                let mut all = ThemeConfig::builtin_themes();
                all.extend(self.config.saved_themes.iter().cloned());
                if !all.is_empty() {
                    ed.preset_idx = (ed.preset_idx + 1) % all.len();
                    ed.editing = all[ed.preset_idx].clone();
                    self.config.theme = ed.editing.clone();
                }
            }
            KeyCode::Enter => {
                self.config.theme = ed.editing.clone();
                let _ = self.config.save();
                self.theme_editor = None;
                self.active_modal = ActiveModal::None;
                self.status_message = "Theme applied".to_string();
            }
            KeyCode::Backspace => {
                ed.field_value_mut().pop();
                self.config.theme = ed.editing.clone();
            }
            KeyCode::Char(c) => {
                ed.field_value_mut().push(c);
                self.config.theme = ed.editing.clone();
            }
            _ => {}
        }
    }

    fn handle_add_entity_key(&mut self, key: KeyEvent) {
        const PRESET_COLORS: [&str; 6] = [
            "#89b4fa", "#a6e3a1", "#f9e2af", "#f38ba8", "#cba6f7", "#94e2d5",
        ];
        match key.code {
            KeyCode::Esc => self.active_modal = ActiveModal::None,
            KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
                self.add_entity_state.active_field = match self.add_entity_state.active_field {
                    AddEntityField::Name => AddEntityField::Color,
                    AddEntityField::Color => AddEntityField::Name,
                };
            }
            KeyCode::Left if self.add_entity_state.active_field == AddEntityField::Color => {
                let pos = PRESET_COLORS
                    .iter()
                    .position(|c| *c == self.add_entity_state.color)
                    .unwrap_or(0);
                let next = if pos == 0 {
                    PRESET_COLORS.len() - 1
                } else {
                    pos - 1
                };
                self.add_entity_state.color = PRESET_COLORS[next].to_string();
            }
            KeyCode::Right if self.add_entity_state.active_field == AddEntityField::Color => {
                let pos = PRESET_COLORS
                    .iter()
                    .position(|c| *c == self.add_entity_state.color)
                    .unwrap_or(0);
                let next = (pos + 1) % PRESET_COLORS.len();
                self.add_entity_state.color = PRESET_COLORS[next].to_string();
            }
            KeyCode::Enter => {
                let title = self.add_entity_state.title.trim().to_string();
                if title.is_empty() {
                    return;
                }
                let color = self.add_entity_state.color.clone();
                let kind = self.add_entity_state.kind;
                self.active_modal = ActiveModal::None;
                let client = self.client.clone();
                let tx = self.api_tx.clone();
                tokio::spawn(async move {
                    match kind {
                        AddEntityKind::Project => match client.create_project(&title, &color).await
                        {
                            Ok(p) => {
                                let _ = tx.send(ApiEvent::ProjectCreated(p));
                            }
                            Err(e) => {
                                let msg = e.to_string();
                                if msg.contains("Route not found") || msg.contains("NOT_FOUND") {
                                    let _ = tx.send(ApiEvent::ProjectCreated(Project {
                                        id: format!(
                                            "local-proj-{}",
                                            Local::now().timestamp_millis()
                                        ),
                                        title,
                                        theme: Some(crate::models::EntityTheme {
                                            primary: Some(color),
                                        }),
                                        color: None,
                                    }));
                                    let _ = tx.send(ApiEvent::Error(
                                        "Added locally (SP REST API has no POST /projects)"
                                            .to_string(),
                                    ));
                                } else {
                                    let _ = tx.send(ApiEvent::Error(format!(
                                        "Create project failed: {e}"
                                    )));
                                }
                            }
                        },
                        AddEntityKind::Tag => match client.create_tag(&title, &color).await {
                            Ok(t) => {
                                let _ = tx.send(ApiEvent::TagCreated(t));
                            }
                            Err(e) => {
                                let msg = e.to_string();
                                if msg.contains("Route not found") || msg.contains("NOT_FOUND") {
                                    let _ = tx.send(ApiEvent::TagCreated(Tag {
                                        id: format!(
                                            "local-tag-{}",
                                            Local::now().timestamp_millis()
                                        ),
                                        title,
                                        color: Some(color),
                                        theme: None,
                                    }));
                                    let _ = tx.send(ApiEvent::Error(
                                        "Added locally (SP REST API has no POST /tags)".to_string(),
                                    ));
                                } else {
                                    let _ =
                                        tx.send(ApiEvent::Error(format!("Create tag failed: {e}")));
                                }
                            }
                        },
                    }
                });
            }
            KeyCode::Backspace => match self.add_entity_state.active_field {
                AddEntityField::Name => {
                    self.add_entity_state.title.pop();
                }
                AddEntityField::Color => {
                    self.add_entity_state.color.pop();
                }
            },
            KeyCode::Char(c) => match self.add_entity_state.active_field {
                AddEntityField::Name => self.add_entity_state.title.push(c),
                AddEntityField::Color => self.add_entity_state.color.push(c),
            },
            _ => {}
        }
    }
}

pub fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    if haystack.contains(needle) {
        return true;
    }
    let mut chars = needle.chars();
    let Some(mut cur) = chars.next() else {
        return true;
    };
    for h in haystack.chars() {
        if h == cur {
            match chars.next() {
                Some(n) => cur = n,
                None => return true,
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_key_events_are_ignored() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        app.focus = FocusSection::Nav;

        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.nav_selected, 1);

        let mut release = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        app.handle_key(release);
        assert_eq!(app.nav_selected, 1);
    }

    #[tokio::test]
    async fn undo_and_redo_toggle_done() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        app.tasks.push(Task {
            id: "t1".into(),
            title: "Task 1".into(),
            is_done: false,
            notes: None,
            project_id: None,
            parent_id: None,
            tag_ids: vec![],
            due_day: None,
            time_estimate: None,
            time_spent: None,
            repeat_cfg_id: None,
        });
        app.push_undo(UndoOp::ToggleDone {
            id: "t1".into(),
            prev_done: false,
        });
        app.tasks[0].is_done = true;

        app.undo();
        assert!(!app.tasks[0].is_done);
        app.redo();
        assert!(app.tasks[0].is_done);
    }

    #[test]
    fn undo_stack_is_capped_at_100() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        for i in 0..120 {
            app.push_undo(UndoOp::ToggleDone {
                id: format!("t{i}"),
                prev_done: false,
            });
        }
        assert_eq!(app.undo_stack.len(), 100);
    }

    #[test]
    fn jk_closes_properties_modal() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        app.active_modal = ActiveModal::Properties;
        app.properties_target = Some(PropertiesTarget::Task("t1".into()));

        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.active_modal, ActiveModal::Properties);
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.active_modal, ActiveModal::None);
    }

    #[test]
    fn add_task_preselects_active_project() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        app.projects.push(Project {
            id: "p1".into(),
            title: "Work".into(),
            theme: None,
            color: None,
        });
        app.active_filter = NavFilter::Project("p1".into());
        app.focus = FocusSection::Main;

        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(app.active_modal, ActiveModal::AddTask);
        assert_eq!(app.add_task_state.project_idx, 1);
    }
}
