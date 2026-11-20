use crate::api::SuperProductivityClient;
use crate::bridge::{BridgeAck, BridgeAction, BridgeCommand, BridgeHandle};
use crate::config::{AppConfig, KeybindMode};
use crate::keybinds::{JkEscapeTracker, JkResult, SequenceState};
use crate::models::{CreateTaskPayload, Project, SimpleCounter, Tag, Task};
use crate::offline::{
    OfflineCache, QueuedOp, apply_order, sanitize_create_payload_for_sp, sanitize_patch_for_sp,
};
use crate::theme::{ThemeConfig, format_hex, map_coords_to_rgb, parse_rgb, rgb_to_map_coords};
use chrono::{Duration as ChronoDuration, Local, NaiveDate};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Offline,
    Syncing,
    Online,
}

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
    Counters,
    History,
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
    History,
    Counters,
    Pomodoro,
    Reschedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PomodoroPhase {
    #[default]
    Focus,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    pub fn duration_secs(self) -> u64 {
        match self {
            Self::Focus => 25 * 60,
            Self::ShortBreak => 5 * 60,
            Self::LongBreak => 15 * 60,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Focus => "FOCUS",
            Self::ShortBreak => "SHORT BREAK",
            Self::LongBreak => "LONG BREAK",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusMode {
    #[default]
    Pomodoro,
    Flowtime,
    Countdown,
}

impl FocusMode {
    pub fn all() -> [Self; 3] {
        [Self::Pomodoro, Self::Flowtime, Self::Countdown]
    }

    pub fn next(self) -> Self {
        match self {
            Self::Pomodoro => Self::Flowtime,
            Self::Flowtime => Self::Countdown,
            Self::Countdown => Self::Pomodoro,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pomodoro => "Pomodoro",
            Self::Flowtime => "Flowtime",
            Self::Countdown => "Countdown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PomodoroState {
    pub mode: FocusMode,
    pub running: bool,
    pub phase: PomodoroPhase,
    pub remaining_secs: u64,
    pub elapsed_secs: u64,
    pub countdown_mins: u64,
    pub last_tick: Option<Instant>,
    pub completed_sessions: u32,
    pub linked_task_id: Option<String>,
    pub reminder: Option<String>,
}

impl Default for PomodoroState {
    fn default() -> Self {
        Self {
            mode: FocusMode::Pomodoro,
            running: false,
            phase: PomodoroPhase::Focus,
            remaining_secs: PomodoroPhase::Focus.duration_secs(),
            elapsed_secs: 0,
            countdown_mins: 30,
            last_tick: None,
            completed_sessions: 0,
            linked_task_id: None,
            reminder: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RescheduleState {
    pub task_id: String,
    pub selected_date: NaiveDate,
    pub time_input: String,
    pub focus_time: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CountersState {
    pub selected: usize,
    pub adding: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryTab {
    #[default]
    Completed,
    Projects,
    Tags,
}

impl HistoryTab {
    pub fn next(self) -> Self {
        match self {
            Self::Completed => Self::Projects,
            Self::Projects => Self::Tags,
            Self::Tags => Self::Completed,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Completed => Self::Tags,
            Self::Projects => Self::Completed,
            Self::Tags => Self::Projects,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryTimeframe {
    #[default]
    All,
    Today,
    Week,
}

impl HistoryTimeframe {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Today,
            Self::Today => Self::Week,
            Self::Week => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All Time",
            Self::Today => "Today",
            Self::Week => "This Week",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HistoryState {
    pub tab: HistoryTab,
    pub timeframe: HistoryTimeframe,
    pub scroll: usize,
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
    pub editing_id: Option<String>,
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
            editing_id: None,
        }
    }

    pub fn edit_project(project: &Project) -> Self {
        let color = project
            .color
            .clone()
            .or_else(|| project.theme.as_ref().and_then(|t| t.primary.clone()))
            .unwrap_or_else(|| "#89b4fa".to_string());
        Self {
            kind: AddEntityKind::Project,
            title: project.title.clone(),
            color,
            active_field: AddEntityField::Color,
            editing_id: Some(project.id.clone()),
        }
    }

    pub fn edit_tag(tag: &Tag) -> Self {
        let color = tag
            .color
            .clone()
            .or_else(|| tag.theme.as_ref().and_then(|t| t.primary.clone()))
            .unwrap_or_else(|| "#a6e3a1".to_string());
        Self {
            kind: AddEntityKind::Tag,
            title: tag.title.clone(),
            color,
            active_field: AddEntityField::Color,
            editing_id: Some(tag.id.clone()),
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
    Project,
    Time,
    Due,
    Deadline,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePickerFocus {
    #[default]
    FieldList,
    Map2D,
    Red,
    Green,
    Blue,
    Hex,
}

impl ThemePickerFocus {
    pub fn next_in_picker(self) -> Self {
        match self {
            Self::FieldList => Self::Map2D,
            Self::Map2D => Self::Red,
            Self::Red => Self::Green,
            Self::Green => Self::Blue,
            Self::Blue => Self::Hex,
            Self::Hex => Self::Map2D,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeEditorState {
    pub original: ThemeConfig,
    pub editing: ThemeConfig,
    pub preset_idx: usize,
    pub field_idx: usize,
    pub picker_focus: ThemePickerFocus,
    pub rgb: (u8, u8, u8),
    pub rgb_input: [String; 3],
    pub map_x: usize,
    pub map_y: usize,
}

impl ThemeEditorState {
    pub const MAP_W: usize = 24;
    pub const MAP_H: usize = 8;
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
        let mut st = Self {
            original: current.clone(),
            editing: current.clone(),
            preset_idx,
            field_idx: 0,
            picker_focus: ThemePickerFocus::FieldList,
            rgb: (34, 34, 34),
            rgb_input: ["34".into(), "34".into(), "34".into()],
            map_x: 0,
            map_y: 0,
        };
        st.sync_picker_from_field();
        st
    }

    pub fn sync_picker_from_field(&mut self) {
        if self.field_idx == 0 {
            return;
        }
        let hex = self.field_value(self.field_idx).to_string();
        if let Some((r, g, b)) = parse_rgb(&hex) {
            self.rgb = (r, g, b);
            self.rgb_input = [r.to_string(), g.to_string(), b.to_string()];
            let (mx, my) = rgb_to_map_coords(r, g, b, Self::MAP_W, Self::MAP_H);
            self.map_x = mx;
            self.map_y = my;
        }
    }

    pub fn apply_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.rgb = (r, g, b);
        self.rgb_input = [r.to_string(), g.to_string(), b.to_string()];
        let (mx, my) = rgb_to_map_coords(r, g, b, Self::MAP_W, Self::MAP_H);
        self.map_x = mx;
        self.map_y = my;
        if self.field_idx > 0 {
            *self.field_value_mut() = format_hex(r, g, b);
        }
    }

    pub fn apply_map_coords(&mut self, mx: usize, my: usize) {
        self.map_x = mx.min(Self::MAP_W - 1);
        self.map_y = my.min(Self::MAP_H - 1);
        let (r, g, b) = map_coords_to_rgb(self.map_x, self.map_y, Self::MAP_W, Self::MAP_H);
        self.rgb = (r, g, b);
        self.rgb_input = [r.to_string(), g.to_string(), b.to_string()];
        if self.field_idx > 0 {
            *self.field_value_mut() = format_hex(r, g, b);
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
        counters: Option<Vec<SimpleCounter>>,
        current_task_id: Option<String>,
        connected: bool,
        message: String,
    },
    CounterUpdated(SimpleCounter),
    TaskUpdated(Task),
    TaskCreated(Task, bool, bool),
    TaskDeleted(String),
    CurrentTaskUpdated(Option<String>),
    #[allow(dead_code)]
    ProjectCreated(Project),
    ProjectUpdated(Project),
    ProjectDeleted(String),
    #[allow(dead_code)]
    TagCreated(Tag),
    TagUpdated(Tag),
    TagDeleted(String),
    HealthCheck(bool),
    ReplayFinished {
        replayed: usize,
        remaining: Vec<QueuedOp>,
        task_id_map: HashMap<String, String>,
    },
    BridgeAcked {
        cmd: BridgeCommand,
        ack: BridgeAck,
    },
    MutationOfflineFallback(QueuedOp),
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
    pub counters: Vec<SimpleCounter>,
    pub connected: bool,
    pub conn_state: ConnectionState,
    pub status_message: String,
    pub last_health_check_at: Instant,
    pub health_check_in_flight: bool,
    pub sync_in_flight: bool,
    pub offline_cache: OfflineCache,
    pub bridge: BridgeHandle,

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
    pub help_scroll: usize,
    pub help_search_focused: bool,
    pub last_sync_at: Instant,
    pub settings_state: SettingsState,
    pub add_task_state: AddTaskState,
    pub add_entity_state: AddEntityState,
    pub history_state: HistoryState,
    pub counters_state: CountersState,
    pub pomodoro: PomodoroState,
    pub reschedule_state: Option<RescheduleState>,
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
        #[cfg(not(test))]
        let mut cache = OfflineCache::load();
        #[cfg(test)]
        let mut cache = OfflineCache::default();
        let counters = if cache.counters.is_empty() {
            config.counters.clone()
        } else {
            cache.counters.clone()
        };
        let mut projects = cache.projects.clone();
        for lp in &cache.local_projects {
            if !projects.iter().any(|p| p.id == lp.id) {
                projects.push(lp.clone());
            }
        }
        apply_order(&mut projects, &cache.project_order, |p| &p.id);
        let mut tags = cache.tags.clone();
        for lt in &cache.local_tags {
            if !tags.iter().any(|t| t.id == lt.id) {
                tags.push(lt.clone());
            }
        }
        apply_order(&mut tags, &cache.tag_order, |t| &t.id);
        let tasks = std::mem::take(&mut cache.tasks);
        let current_task_id = cache.current_task_id.clone();
        cache.tasks = tasks.clone();
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
            tasks,
            projects,
            tags,
            counters,
            connected: false,
            conn_state: ConnectionState::Offline,
            status_message: String::new(),
            last_health_check_at: Instant::now(),
            health_check_in_flight: false,
            sync_in_flight: false,
            offline_cache: cache,
            bridge: BridgeHandle::new(),
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
            current_task_id,
            timer_started_at: None,
            typed_key_display: String::new(),
            typed_key_at: None,
            active_modal: ActiveModal::None,
            properties_target: None,
            properties_mode: PropertiesMode::Menu,
            properties_anchor: None,
            help_search: String::new(),
            help_scroll: 0,
            help_search_focused: false,
            last_sync_at: Instant::now(),
            settings_state,
            add_task_state: AddTaskState::default(),
            add_entity_state: AddEntityState::new(AddEntityKind::Project),
            history_state: HistoryState::default(),
            counters_state: CountersState::default(),
            pomodoro: PomodoroState::default(),
            reschedule_state: None,
            theme_editor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            areas: UiAreas::default(),
            api_tx,
        }
    }

    pub fn save_cache(&mut self) {
        self.offline_cache.tasks = self.tasks.clone();
        self.offline_cache.projects = self
            .projects
            .iter()
            .filter(|p| !p.id.starts_with("local-proj-"))
            .cloned()
            .collect();
        self.offline_cache.local_projects = self
            .projects
            .iter()
            .filter(|p| p.id.starts_with("local-proj-"))
            .cloned()
            .collect();
        self.offline_cache.tags = self
            .tags
            .iter()
            .filter(|t| !t.id.starts_with("local-tag-"))
            .cloned()
            .collect();
        self.offline_cache.local_tags = self
            .tags
            .iter()
            .filter(|t| t.id.starts_with("local-tag-"))
            .cloned()
            .collect();
        self.offline_cache.counters = self.counters.clone();
        self.offline_cache.current_task_id = self.current_task_id.clone();
        self.offline_cache.project_order = self.projects.iter().map(|p| p.id.clone()).collect();
        self.offline_cache.tag_order = self.tags.iter().map(|t| t.id.clone()).collect();
        #[cfg(not(test))]
        {
            let _ = self.offline_cache.save();
        }
    }

    pub fn enqueue_offline_op(&mut self, op: QueuedOp) {
        self.offline_cache.pending_ops.push(op);
        let n = self.offline_cache.pending_ops.len();
        self.status_message = format!("Queued offline ({n} pending)");
        self.save_cache();
    }

    pub fn sync_with_replay(&mut self) {
        if self.sync_in_flight {
            return;
        }
        self.sync_in_flight = true;
        self.conn_state = ConnectionState::Syncing;
        self.last_sync_at = Instant::now();

        let pending = std::mem::take(&mut self.offline_cache.pending_ops);
        if pending.is_empty() {
            self.refresh_all_inner();
            return;
        }

        let client = self.client.clone();
        let tx = self.api_tx.clone();
        let proj_map = self.offline_cache.project_id_map.clone();
        let tag_map = self.offline_cache.tag_id_map.clone();
        tokio::spawn(async move {
            let mut task_id_map: HashMap<String, String> = HashMap::new();
            let mut replayed = 0usize;
            let mut remaining = Vec::new();
            let mut failed = false;

            for op in pending {
                if failed {
                    remaining.push(op);
                    continue;
                }
                let ok = match &op {
                    QueuedOp::CreateTask {
                        temp_id, payload, ..
                    } => {
                        let clean = sanitize_create_payload_for_sp(
                            payload.clone(),
                            &task_id_map,
                            &proj_map,
                            &tag_map,
                        );
                        match client.create_task(clean).await {
                            Ok(created) => {
                                task_id_map.insert(temp_id.clone(), created.id);
                                true
                            }
                            Err(_) => false,
                        }
                    }
                    QueuedOp::UpdateTask { id, patch } => {
                        let real_id = task_id_map.get(id).cloned().unwrap_or_else(|| id.clone());
                        if real_id.starts_with("offline-task-") {
                            true
                        } else {
                            let clean_patch =
                                sanitize_patch_for_sp(patch.clone(), &proj_map, &tag_map);
                            client.update_task(&real_id, clean_patch).await.is_ok()
                        }
                    }
                    QueuedOp::DeleteTask { id } => {
                        let real_id = task_id_map.get(id).cloned().unwrap_or_else(|| id.clone());
                        if real_id.starts_with("offline-task-") {
                            true
                        } else {
                            client.delete_task(&real_id).await.is_ok()
                        }
                    }
                    QueuedOp::SetCurrentTask { task_id } => {
                        let real_id = task_id
                            .as_ref()
                            .map(|id| task_id_map.get(id).cloned().unwrap_or_else(|| id.clone()));
                        client.set_current_task(real_id.as_deref()).await.is_ok()
                    }
                };
                if ok {
                    replayed += 1;
                } else {
                    failed = true;
                    remaining.push(op);
                }
            }

            let _ = tx.send(ApiEvent::ReplayFinished {
                replayed,
                remaining,
                task_id_map,
            });
        });
    }

    pub fn refresh_all(&mut self) {
        self.sync_with_replay();
    }

    fn refresh_all_inner(&self) {
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.health().await {
                Ok(h) if h.server == "up" && h.renderer_ready => {
                    let tasks = client.get_tasks().await;
                    let projects = client.get_projects().await.unwrap_or_default();
                    let tags = client.get_tags().await.unwrap_or_default();
                    let counters = client.get_counters().await.ok();
                    let current = client.get_current_task().await.ok().flatten();
                    let current_task_id = current.map(|t| t.id);
                    match tasks {
                        Ok(tasks) => {
                            let _ = tx.send(ApiEvent::Loaded {
                                tasks,
                                projects,
                                tags,
                                counters,
                                current_task_id,
                                connected: true,
                                message: String::new(),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Loaded {
                                tasks: Vec::new(),
                                projects,
                                tags,
                                counters,
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
                        counters: None,
                        current_task_id: None,
                        connected: false,
                        message: format!("Server {} (ready={})", h.server, h.renderer_ready),
                    });
                }
                Err(_) => {
                    let _ = tx.send(ApiEvent::Loaded {
                        tasks: Vec::new(),
                        projects: Vec::new(),
                        tags: Vec::new(),
                        counters: None,
                        current_task_id: None,
                        connected: false,
                        message: String::new(),
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
                counters,
                current_task_id,
                connected,
                message,
            } => {
                self.sync_in_flight = false;
                self.connected = connected;
                self.conn_state = if connected {
                    ConnectionState::Online
                } else {
                    ConnectionState::Offline
                };
                if connected {
                    self.tasks = tasks;
                    let mut merged_projects = projects;
                    for lp in &self.offline_cache.local_projects {
                        if !merged_projects.iter().any(|p| p.id == lp.id) {
                            merged_projects.push(lp.clone());
                        }
                    }
                    apply_order(
                        &mut merged_projects,
                        &self.offline_cache.project_order,
                        |p| &p.id,
                    );
                    self.projects = merged_projects;

                    let mut merged_tags = tags;
                    for lt in &self.offline_cache.local_tags {
                        if !merged_tags.iter().any(|t| t.id == lt.id) {
                            merged_tags.push(lt.clone());
                        }
                    }
                    apply_order(&mut merged_tags, &self.offline_cache.tag_order, |t| &t.id);
                    self.tags = merged_tags;

                    if let Some(c) = counters {
                        self.counters = c;
                    }
                    self.timer_started_at = current_task_id.as_ref().map(|_| Instant::now());
                    self.current_task_id = current_task_id;
                    self.save_cache();
                }
                if !message.is_empty() {
                    self.status_message = message;
                } else if connected && self.status_message == "Syncing..." {
                    self.status_message.clear();
                }
                self.last_sync_at = Instant::now();
                self.clamp_selections();
            }
            ApiEvent::HealthCheck(is_up) => {
                self.health_check_in_flight = false;
                if !self.connected && is_up {
                    self.connected = true;
                    self.sync_with_replay();
                } else if self.connected && !is_up {
                    self.connected = false;
                    self.conn_state = ConnectionState::Offline;
                }
            }
            ApiEvent::ReplayFinished {
                replayed,
                remaining,
                task_id_map,
            } => {
                if !remaining.is_empty() {
                    let mut restored = remaining;
                    restored.append(&mut self.offline_cache.pending_ops);
                    self.offline_cache.pending_ops = restored;
                }
                for t in &mut self.tasks {
                    if let Some(real) = task_id_map.get(&t.id) {
                        t.id = real.clone();
                    }
                    if let Some(pid) = &t.parent_id
                        && let Some(real) = task_id_map.get(pid)
                    {
                        t.parent_id = Some(real.clone());
                    }
                }
                if let Some(cid) = &self.current_task_id
                    && let Some(real) = task_id_map.get(cid)
                {
                    self.current_task_id = Some(real.clone());
                }
                self.save_cache();
                if replayed > 0 {
                    self.status_message = format!("Synced {replayed} queued change(s)");
                }
                self.refresh_all_inner();
            }
            ApiEvent::BridgeAcked { cmd, ack } => {
                if ack.ok {
                    match &cmd.action {
                        BridgeAction::CreateProject { local_id, .. } => {
                            if let Some(real_id) = &ack.entity_id {
                                self.offline_cache
                                    .project_id_map
                                    .insert(local_id.clone(), real_id.clone());
                                self.offline_cache
                                    .local_projects
                                    .retain(|p| &p.id != local_id);
                                for p in &mut self.projects {
                                    if &p.id == local_id {
                                        p.id = real_id.clone();
                                    }
                                }
                                for t in &mut self.tasks {
                                    if t.project_id.as_deref() == Some(local_id.as_str()) {
                                        t.project_id = Some(real_id.clone());
                                    }
                                }
                                self.save_cache();
                                self.status_message = "Project synced to SP".to_string();
                                self.sync_with_replay();
                            }
                        }
                        BridgeAction::CreateTag { local_id, .. } => {
                            if let Some(real_id) = &ack.entity_id {
                                self.offline_cache
                                    .tag_id_map
                                    .insert(local_id.clone(), real_id.clone());
                                self.offline_cache.local_tags.retain(|t| &t.id != local_id);
                                for t in &mut self.tags {
                                    if &t.id == local_id {
                                        t.id = real_id.clone();
                                    }
                                }
                                for task in &mut self.tasks {
                                    for tid in &mut task.tag_ids {
                                        if tid == local_id {
                                            *tid = real_id.clone();
                                        }
                                    }
                                }
                                self.save_cache();
                                self.status_message = "Tag synced to SP".to_string();
                                self.sync_with_replay();
                            }
                        }
                        BridgeAction::UpdateProject { .. }
                        | BridgeAction::DeleteProject { .. }
                        | BridgeAction::UpdateTag { .. }
                        | BridgeAction::DeleteTag { .. } => {
                            self.status_message = "Synced to SP via bridge".to_string();
                            self.sync_with_replay();
                        }
                    }
                } else if let Some(err) = ack.error {
                    self.status_message = format!("Bridge error: {err}");
                }
            }
            ApiEvent::MutationOfflineFallback(op) => {
                self.connected = false;
                self.conn_state = ConnectionState::Offline;
                self.enqueue_offline_op(op);
            }
            ApiEvent::CounterUpdated(updated) => {
                if let Some(c) = self.counters.iter_mut().find(|c| c.id == updated.id) {
                    *c = updated;
                }
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
                label: "History".to_string(),
                count: Some(self.tasks.iter().filter(|t| t.is_done).count()),
                kind: NavItemKind::History,
                indent: 0,
                icon: "\u{f1da}",
            },
            NavRow {
                label: "Counters".to_string(),
                count: Some(self.counters.len()),
                kind: NavItemKind::Counters,
                indent: 0,
                icon: "\u{f080}",
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
        if self.pomodoro.running
            && let Some(last) = self.pomodoro.last_tick
            && last.elapsed() >= Duration::from_secs(1)
        {
            self.pomodoro.last_tick = Some(Instant::now());
            if self.pomodoro.mode == FocusMode::Flowtime
                && self.pomodoro.phase == PomodoroPhase::Focus
            {
                self.pomodoro.elapsed_secs += 1;
            } else {
                if self.pomodoro.remaining_secs > 0 {
                    self.pomodoro.remaining_secs -= 1;
                }
                if self.pomodoro.remaining_secs == 0 {
                    self.pomodoro.running = false;
                    match self.pomodoro.mode {
                        FocusMode::Countdown => {
                            self.pomodoro.completed_sessions += 1;
                            self.pomodoro.remaining_secs = self.pomodoro.countdown_mins * 60;
                            self.pomodoro.reminder = Some("Countdown finished!".to_string());
                        }
                        FocusMode::Flowtime => {
                            self.pomodoro.phase = PomodoroPhase::Focus;
                            self.pomodoro.elapsed_secs = 0;
                            self.pomodoro.reminder =
                                Some("Flowtime break finished! Ready to focus.".to_string());
                        }
                        FocusMode::Pomodoro => match self.pomodoro.phase {
                            PomodoroPhase::Focus => {
                                self.pomodoro.completed_sessions += 1;
                                let next_phase =
                                    if self.pomodoro.completed_sessions.is_multiple_of(4) {
                                        PomodoroPhase::LongBreak
                                    } else {
                                        PomodoroPhase::ShortBreak
                                    };
                                self.pomodoro.phase = next_phase;
                                self.pomodoro.remaining_secs = next_phase.duration_secs();
                                self.pomodoro.reminder = Some(format!(
                                    "Focus complete! Time for a {} ({}m).",
                                    next_phase.label(),
                                    next_phase.duration_secs() / 60
                                ));
                            }
                            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
                                self.pomodoro.phase = PomodoroPhase::Focus;
                                self.pomodoro.remaining_secs = PomodoroPhase::Focus.duration_secs();
                                self.pomodoro.reminder = Some(
                                    "Break finished! Ready for next Focus session.".to_string(),
                                );
                            }
                        },
                    }
                }
            }
        }
        if !self.health_check_in_flight
            && !self.sync_in_flight
            && self.last_health_check_at.elapsed() >= Duration::from_secs(2)
        {
            self.health_check_in_flight = true;
            self.last_health_check_at = Instant::now();
            let client = self.client.clone();
            let tx = self.api_tx.clone();
            tokio::spawn(async move {
                let is_up = matches!(
                    client.health().await,
                    Ok(h) if h.server == "up" && h.renderer_ready
                );
                let _ = tx.send(ApiEvent::HealthCheck(is_up));
            });
        }

        if self.connected
            && !self.sync_in_flight
            && self.active_modal == ActiveModal::None
            && self.rename_target.is_none()
            && self.last_sync_at.elapsed() >= Duration::from_secs(30)
        {
            self.sync_with_replay();
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
            ActiveModal::History => {
                self.handle_history_key(key);
                return;
            }
            ActiveModal::Counters => {
                self.handle_counters_key(key);
                return;
            }
            ActiveModal::Pomodoro => {
                self.handle_pomodoro_key(key);
                return;
            }
            ActiveModal::Reschedule => {
                self.handle_reschedule_key(key);
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
                        self.help_scroll = 0;
                        self.help_search_focused = false;
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
                    (':', 'w') => {
                        self.sequence.clear();
                        self.history_state.scroll = 0;
                        self.active_modal = ActiveModal::History;
                        return;
                    }
                    (':', 'n') => {
                        self.sequence.clear();
                        self.active_modal = ActiveModal::Counters;
                        return;
                    }
                    (':', 'p') => {
                        self.sequence.clear();
                        self.active_modal = ActiveModal::Pomodoro;
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
                'S' => {
                    self.status_message = "Syncing...".to_string();
                    self.sync_with_replay();
                }
                'H' => {
                    self.history_state.scroll = 0;
                    self.active_modal = ActiveModal::History;
                }
                'C' => {
                    self.active_modal = ActiveModal::Counters;
                }
                'P' => {
                    self.active_modal = ActiveModal::Pomodoro;
                }
                'R' => {
                    if let Some(t) = self.selected_task().cloned() {
                        self.open_reschedule(&t.id);
                    }
                }
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
                        NavItemKind::History => {
                            self.history_state.scroll = 0;
                            self.active_modal = ActiveModal::History;
                        }
                        NavItemKind::Counters => {
                            self.active_modal = ActiveModal::Counters;
                        }
                        NavItemKind::Help => {
                            self.help_search.clear();
                            self.help_scroll = 0;
                            self.help_search_focused = false;
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
                    if !self.connected || task.id.starts_with("offline-task-") {
                        self.enqueue_offline_op(QueuedOp::UpdateTask {
                            id: task.id,
                            patch: json!({ "isDone": next_done }),
                        });
                    } else {
                        let client = self.client.clone();
                        let tx = self.api_tx.clone();
                        tokio::spawn(async move {
                            match client.toggle_done(&task.id, next_done).await {
                                Ok(t) => {
                                    let _ = tx.send(ApiEvent::TaskUpdated(t));
                                }
                                Err(_) => {
                                    let _ = tx.send(ApiEvent::MutationOfflineFallback(
                                        QueuedOp::UpdateTask {
                                            id: task.id,
                                            patch: json!({ "isDone": next_done }),
                                        },
                                    ));
                                }
                            }
                        });
                    }
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
            if !self.connected || task.id.starts_with("offline-task-") {
                self.enqueue_offline_op(QueuedOp::DeleteTask { id: task.id });
            } else {
                let client = self.client.clone();
                let tx = self.api_tx.clone();
                tokio::spawn(async move {
                    match client.delete_task(&task.id).await {
                        Ok(()) => {
                            let _ = tx.send(ApiEvent::TaskDeleted(task.id));
                        }
                        Err(_) => {
                            let _ =
                                tx.send(ApiEvent::MutationOfflineFallback(QueuedOp::DeleteTask {
                                    id: task.id,
                                }));
                        }
                    }
                });
            }
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
        if !self.connected {
            self.enqueue_offline_op(QueuedOp::SetCurrentTask { task_id: next_id });
        } else {
            let client = self.client.clone();
            let tx = self.api_tx.clone();
            tokio::spawn(async move {
                match client.set_current_task(next_id.as_deref()).await {
                    Ok(()) => {
                        let _ = tx.send(ApiEvent::CurrentTaskUpdated(next_id));
                    }
                    Err(_) => {
                        let _ = tx.send(ApiEvent::MutationOfflineFallback(
                            QueuedOp::SetCurrentTask { task_id: next_id },
                        ));
                    }
                }
            });
        }
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
                    deadline_day: task.deadline_day.clone(),
                    time_estimate: task.time_estimate,
                    time_spent: task.time_spent,
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
                    deadline_day: task.deadline_day.clone(),
                    time_estimate: task.time_estimate,
                    time_spent: task.time_spent,
                    is_add_to_top: Some(*to_top),
                };
                self.spawn_create(payload, *to_top, false);
            }
        }
        self.undo_stack.push(op);
        self.status_message = "Redone".to_string();
        self.clamp_selections();
    }

    fn spawn_patch(&mut self, id: String, patch: serde_json::Value) {
        if !self.connected || id.starts_with("offline-task-") {
            self.enqueue_offline_op(QueuedOp::UpdateTask { id, patch });
            return;
        }
        let clean_patch = sanitize_patch_for_sp(
            patch.clone(),
            &self.offline_cache.project_id_map,
            &self.offline_cache.tag_id_map,
        );
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.update_task(&id, clean_patch).await {
                Ok(t) => {
                    let _ = tx.send(ApiEvent::TaskUpdated(t));
                }
                Err(_) => {
                    let _ = tx.send(ApiEvent::MutationOfflineFallback(QueuedOp::UpdateTask {
                        id,
                        patch,
                    }));
                }
            }
        });
    }

    fn spawn_create(&mut self, payload: CreateTaskPayload, to_top: bool, record_undo: bool) {
        if !self.connected {
            let temp_id = format!("offline-task-{}", Local::now().timestamp_millis());
            let task = Task {
                id: temp_id.clone(),
                title: payload.title.clone(),
                is_done: false,
                notes: payload.notes.clone(),
                project_id: payload.project_id.clone(),
                parent_id: payload.parent_id.clone(),
                tag_ids: payload.tag_ids.clone(),
                due_day: payload.due_day.clone(),
                deadline_day: payload.deadline_day.clone(),
                done_on: None,
                time_estimate: payload.time_estimate,
                time_spent: payload.time_spent,
                repeat_cfg_id: None,
            };
            if record_undo {
                self.push_undo(UndoOp::CreateTask {
                    task: task.clone(),
                    to_top,
                });
            } else if let Some(UndoOp::DeleteTask { task: dt, .. }) = self.redo_stack.last_mut() {
                dt.id = temp_id.clone();
            }
            if to_top {
                self.tasks.insert(0, task);
                self.task_selected = 0;
            } else {
                self.tasks.push(task);
            }
            self.clamp_selections();
            self.enqueue_offline_op(QueuedOp::CreateTask {
                temp_id,
                payload,
                to_top,
            });
            return;
        }
        let clean_payload = sanitize_create_payload_for_sp(
            payload.clone(),
            &HashMap::new(),
            &self.offline_cache.project_id_map,
            &self.offline_cache.tag_id_map,
        );
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.create_task(clean_payload).await {
                Ok(t) => {
                    let _ = tx.send(ApiEvent::TaskCreated(t, to_top, record_undo));
                }
                Err(_) => {
                    let temp_id = format!("offline-task-{}", Local::now().timestamp_millis());
                    let task = Task {
                        id: temp_id.clone(),
                        title: payload.title.clone(),
                        is_done: false,
                        notes: payload.notes.clone(),
                        project_id: payload.project_id.clone(),
                        parent_id: payload.parent_id.clone(),
                        tag_ids: payload.tag_ids.clone(),
                        due_day: payload.due_day.clone(),
                        deadline_day: payload.deadline_day.clone(),
                        done_on: None,
                        time_estimate: payload.time_estimate,
                        time_spent: payload.time_spent,
                        repeat_cfg_id: None,
                    };
                    let _ = tx.send(ApiEvent::TaskCreated(task, to_top, record_undo));
                    let _ = tx.send(ApiEvent::MutationOfflineFallback(QueuedOp::CreateTask {
                        temp_id,
                        payload,
                        to_top,
                    }));
                }
            }
        });
    }

    fn spawn_delete(&mut self, id: String) {
        if !self.connected || id.starts_with("offline-task-") {
            self.enqueue_offline_op(QueuedOp::DeleteTask { id });
            return;
        }
        let client = self.client.clone();
        let tx = self.api_tx.clone();
        tokio::spawn(async move {
            match client.delete_task(&id).await {
                Ok(()) => {
                    let _ = tx.send(ApiEvent::TaskDeleted(id));
                }
                Err(_) => {
                    let _ = tx.send(ApiEvent::MutationOfflineFallback(QueuedOp::DeleteTask {
                        id,
                    }));
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
                'c' => {
                    if let Some(p) = self.projects.iter().find(|p| p.id == pid) {
                        self.add_entity_state = AddEntityState::edit_project(p);
                        self.active_modal = ActiveModal::AddEntity;
                    }
                }
                'u' => {
                    if let Some(pos) = self.projects.iter().position(|p| p.id == pid)
                        && pos > 0
                    {
                        self.projects.swap(pos, pos - 1);
                        self.save_cache();
                        self.status_message = "Project moved up".to_string();
                    }
                    self.active_modal = ActiveModal::None;
                }
                'n' => {
                    if let Some(pos) = self.projects.iter().position(|p| p.id == pid)
                        && pos + 1 < self.projects.len()
                    {
                        self.projects.swap(pos, pos + 1);
                        self.save_cache();
                        self.status_message = "Project moved down".to_string();
                    }
                    self.active_modal = ActiveModal::None;
                }
                'd' => {
                    self.active_modal = ActiveModal::None;
                    self.projects.retain(|p| p.id != pid);
                    self.offline_cache.local_projects.retain(|p| p.id != pid);
                    self.save_cache();
                    if !pid.starts_with("local-proj-") {
                        self.bridge
                            .enqueue(BridgeAction::DeleteProject { id: pid.clone() });
                    }
                    let client = self.client.clone();
                    let tx = self.api_tx.clone();
                    let plugin_active = self.bridge.is_plugin_active();
                    tokio::spawn(async move {
                        match client.delete_project(&pid).await {
                            Ok(()) => {
                                let _ = tx.send(ApiEvent::ProjectDeleted(pid));
                            }
                            Err(_) if plugin_active => {
                                let _ = tx.send(ApiEvent::Error(
                                    "Project delete queued via SP bridge".to_string(),
                                ));
                            }
                            Err(_) => {
                                let _ = tx.send(ApiEvent::Error(
                                    "Deleted locally (install sp-tui-bridge plugin to sync)"
                                        .to_string(),
                                ));
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
                'c' => {
                    if let Some(t) = self.tags.iter().find(|t| t.id == tid) {
                        self.add_entity_state = AddEntityState::edit_tag(t);
                        self.active_modal = ActiveModal::AddEntity;
                    }
                }
                'u' => {
                    if let Some(pos) = self.tags.iter().position(|t| t.id == tid)
                        && pos > 0
                    {
                        self.tags.swap(pos, pos - 1);
                        self.save_cache();
                        self.status_message = "Tag moved up".to_string();
                    }
                    self.active_modal = ActiveModal::None;
                }
                'n' => {
                    if let Some(pos) = self.tags.iter().position(|t| t.id == tid)
                        && pos + 1 < self.tags.len()
                    {
                        self.tags.swap(pos, pos + 1);
                        self.save_cache();
                        self.status_message = "Tag moved down".to_string();
                    }
                    self.active_modal = ActiveModal::None;
                }
                'd' => {
                    self.active_modal = ActiveModal::None;
                    self.tags.retain(|t| t.id != tid);
                    self.offline_cache.local_tags.retain(|t| t.id != tid);
                    self.save_cache();
                    if !tid.starts_with("local-tag-") {
                        self.bridge
                            .enqueue(BridgeAction::DeleteTag { id: tid.clone() });
                    }
                    let client = self.client.clone();
                    let tx = self.api_tx.clone();
                    let plugin_active = self.bridge.is_plugin_active();
                    tokio::spawn(async move {
                        match client.delete_tag(&tid).await {
                            Ok(()) => {
                                let _ = tx.send(ApiEvent::TagDeleted(tid));
                            }
                            Err(_) if plugin_active => {
                                let _ = tx.send(ApiEvent::Error(
                                    "Tag delete queued via SP bridge".to_string(),
                                ));
                            }
                            Err(_) => {
                                let _ = tx.send(ApiEvent::Error(
                                    "Deleted locally (install sp-tui-bridge plugin to sync)"
                                        .to_string(),
                                ));
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
                        self.open_reschedule(&tid);
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
                                deadline_day: orig.deadline_day,
                                time_estimate: orig.time_estimate,
                                time_spent: orig.time_spent,
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
                if !self.connected || id.starts_with("offline-task-") {
                    self.enqueue_offline_op(QueuedOp::UpdateTask {
                        id,
                        patch: json!({ "title": new_title }),
                    });
                } else {
                    tokio::spawn(async move {
                        match client.rename_task(&id, &new_title).await {
                            Ok(t) => {
                                let _ = tx.send(ApiEvent::TaskUpdated(t));
                            }
                            Err(_) => {
                                let _ = tx.send(ApiEvent::MutationOfflineFallback(
                                    QueuedOp::UpdateTask {
                                        id,
                                        patch: json!({ "title": new_title }),
                                    },
                                ));
                            }
                        }
                    });
                }
            }
            RenameTarget::Project(id) => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
                    p.title = new_title.clone();
                }
                self.save_cache();
                self.bridge.enqueue(BridgeAction::UpdateProject {
                    id: id.clone(),
                    title: Some(new_title.clone()),
                    color: None,
                });
                let plugin_active = self.bridge.is_plugin_active();
                tokio::spawn(async move {
                    match client.rename_project(&id, &new_title).await {
                        Ok(p) => {
                            let _ = tx.send(ApiEvent::ProjectUpdated(p));
                        }
                        Err(_) if plugin_active => {
                            let _ = tx.send(ApiEvent::Error(
                                "Project rename queued via SP bridge".to_string(),
                            ));
                        }
                        Err(_) => {
                            let _ = tx.send(ApiEvent::Error(
                                "Renamed locally (install sp-tui-bridge plugin to sync)"
                                    .to_string(),
                            ));
                        }
                    }
                });
            }
            RenameTarget::Tag(id) => {
                if let Some(t) = self.tags.iter_mut().find(|t| t.id == id) {
                    t.title = new_title.clone();
                }
                self.save_cache();
                self.bridge.enqueue(BridgeAction::UpdateTag {
                    id: id.clone(),
                    title: Some(new_title.clone()),
                    color: None,
                });
                let plugin_active = self.bridge.is_plugin_active();
                tokio::spawn(async move {
                    match client.rename_tag(&id, &new_title).await {
                        Ok(t) => {
                            let _ = tx.send(ApiEvent::TagUpdated(t));
                        }
                        Err(_) if plugin_active => {
                            let _ = tx.send(ApiEvent::Error(
                                "Tag rename queued via SP bridge".to_string(),
                            ));
                        }
                        Err(_) => {
                            let _ = tx.send(ApiEvent::Error(
                                "Renamed locally (install sp-tui-bridge plugin to sync)"
                                    .to_string(),
                            ));
                        }
                    }
                });
            }
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        if self.help_search_focused {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.help_search_focused = false;
                }
                KeyCode::Backspace => {
                    self.help_search.pop();
                    self.help_scroll = 0;
                }
                KeyCode::Char(c) => {
                    self.help_search.push(c);
                    self.help_scroll = 0;
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.active_modal = ActiveModal::None,
            KeyCode::Char('/') | KeyCode::Char('i') => {
                self.help_search_focused = true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.help_scroll = self.help_scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Char('h') => {
                self.help_scroll = 0;
            }
            KeyCode::Char('G') | KeyCode::Char('l') => {
                self.help_scroll = usize::MAX;
            }
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
            self.set_completion(CompletionKind::Tag, items, last_word_start);
        } else if let Some(prefix) = token.strip_prefix('+') {
            let p_low = prefix.to_lowercase();
            let items: Vec<String> = self
                .projects
                .iter()
                .filter(|p| p_low.is_empty() || p.title.to_lowercase().contains(&p_low))
                .map(|p| p.title.clone())
                .collect();
            self.set_completion(CompletionKind::Project, items, last_word_start);
        } else if let Some(prefix) = token.strip_prefix('t')
            && (prefix.is_empty() || prefix.chars().next().is_some_and(|c| c.is_ascii_digit()))
        {
            let p_low = prefix.to_lowercase();
            let presets = [
                "15m", "30m", "45m", "1h", "1h30m", "1h5m3s", "2h", "1d", "30m/1h", "1h/2h",
            ];
            let items: Vec<String> = presets
                .iter()
                .filter(|s| p_low.is_empty() || s.starts_with(&p_low))
                .map(|s| (*s).to_string())
                .collect();
            self.set_completion(CompletionKind::Time, items, last_word_start);
        } else if let Some(prefix) = token.strip_prefix('@') {
            let p_low = prefix.to_lowercase();
            let presets = [
                "today",
                "tomorrow",
                "monday",
                "friday",
                "next_week",
                "30m",
                "2h",
                "1d",
                "4pm",
                "19:00",
            ];
            let items: Vec<String> = presets
                .iter()
                .filter(|s| p_low.is_empty() || s.starts_with(&p_low))
                .map(|s| (*s).to_string())
                .collect();
            self.set_completion(CompletionKind::Due, items, last_word_start);
        } else if let Some(prefix) = token.strip_prefix('!') {
            let p_low = prefix.to_lowercase();
            let presets = [
                "today",
                "tomorrow",
                "friday",
                "next_week",
                "next_month",
                "2d",
                "1h30m",
                "14:30",
            ];
            let items: Vec<String> = presets
                .iter()
                .filter(|s| p_low.is_empty() || s.starts_with(&p_low))
                .map(|s| (*s).to_string())
                .collect();
            self.set_completion(CompletionKind::Deadline, items, last_word_start);
        } else {
            self.add_task_state.completion = None;
        }
    }

    fn set_completion(&mut self, kind: CompletionKind, items: Vec<String>, trigger_start: usize) {
        if items.is_empty() {
            self.add_task_state.completion = None;
        } else {
            self.add_task_state.completion = Some(CompletionPopup {
                kind,
                items,
                selected: 0,
                trigger_start,
            });
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
            CompletionKind::Project => {
                if let Some(idx) = self.projects.iter().position(|p| p.title == choice) {
                    self.add_task_state.project_idx = idx + 1;
                }
                '+'
            }
            CompletionKind::Time => 't',
            CompletionKind::Due => '@',
            CompletionKind::Deadline => '!',
        };
        self.add_task_state.title.truncate(comp.trigger_start);
        self.add_task_state
            .title
            .push_str(&format!("{prefix_char}{choice} "));
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
                let initial_project_id = if self.add_task_state.project_idx == 0 {
                    None
                } else {
                    self.projects
                        .get(self.add_task_state.project_idx - 1)
                        .map(|p| p.id.clone())
                };
                let parsed = crate::short_syntax::parse_short_syntax(
                    &raw_title,
                    &self.projects,
                    &self.tags,
                    initial_project_id,
                    self.add_task_state.selected_tag_ids.clone(),
                );
                if parsed.title.is_empty() {
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
                let to_top = self.add_task_state.add_to_top;
                let payload = CreateTaskPayload {
                    title: parsed.title,
                    notes: if notes.is_empty() { None } else { Some(notes) },
                    project_id: parsed.project_id,
                    parent_id: self.add_task_state.parent_id.clone(),
                    tag_ids: parsed.tag_ids,
                    due_day: parsed.due_day,
                    deadline_day: parsed.deadline_day,
                    time_estimate: parsed.time_estimate,
                    time_spent: parsed.time_spent,
                    is_add_to_top: Some(to_top),
                };
                self.active_modal = ActiveModal::None;
                self.spawn_create(payload, to_top, true);
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

        if ed.picker_focus != ThemePickerFocus::FieldList {
            match key.code {
                KeyCode::Esc => {
                    ed.picker_focus = ThemePickerFocus::FieldList;
                }
                KeyCode::Tab => {
                    ed.picker_focus = ed.picker_focus.next_in_picker();
                }
                KeyCode::Enter => {
                    self.config.theme = ed.editing.clone();
                    let _ = self.config.save();
                    self.theme_editor = None;
                    self.active_modal = ActiveModal::None;
                    self.status_message = "Theme applied".to_string();
                }
                _ => match ed.picker_focus {
                    ThemePickerFocus::Map2D => {
                        match key.code {
                            KeyCode::Left | KeyCode::Char('h') => {
                                ed.apply_map_coords(ed.map_x.saturating_sub(1), ed.map_y);
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                ed.apply_map_coords(ed.map_x + 1, ed.map_y);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                ed.apply_map_coords(ed.map_x, ed.map_y.saturating_sub(1));
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                ed.apply_map_coords(ed.map_x, ed.map_y + 1);
                            }
                            _ => {}
                        }
                        self.config.theme = ed.editing.clone();
                    }
                    ThemePickerFocus::Red | ThemePickerFocus::Green | ThemePickerFocus::Blue => {
                        let ch_idx = match ed.picker_focus {
                            ThemePickerFocus::Red => 0,
                            ThemePickerFocus::Green => 1,
                            _ => 2,
                        };
                        let mut vals = [ed.rgb.0, ed.rgb.1, ed.rgb.2];
                        match key.code {
                            KeyCode::Left | KeyCode::Char('h') => {
                                vals[ch_idx] = vals[ch_idx].saturating_sub(5);
                                ed.apply_rgb(vals[0], vals[1], vals[2]);
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                vals[ch_idx] = vals[ch_idx].saturating_add(5);
                                ed.apply_rgb(vals[0], vals[1], vals[2]);
                            }
                            KeyCode::Backspace => {
                                ed.rgb_input[ch_idx].pop();
                                if let Ok(v) = ed.rgb_input[ch_idx].parse::<u8>() {
                                    vals[ch_idx] = v;
                                    ed.rgb = (vals[0], vals[1], vals[2]);
                                    let (mx, my) = rgb_to_map_coords(
                                        vals[0],
                                        vals[1],
                                        vals[2],
                                        ThemeEditorState::MAP_W,
                                        ThemeEditorState::MAP_H,
                                    );
                                    ed.map_x = mx;
                                    ed.map_y = my;
                                    *ed.field_value_mut() = format_hex(vals[0], vals[1], vals[2]);
                                }
                            }
                            KeyCode::Char(c) if c.is_ascii_digit() => {
                                if ed.rgb_input[ch_idx].len() < 3 {
                                    ed.rgb_input[ch_idx].push(c);
                                } else {
                                    ed.rgb_input[ch_idx] = c.to_string();
                                }
                                if let Ok(v) = ed.rgb_input[ch_idx].parse::<u8>() {
                                    vals[ch_idx] = v;
                                    ed.apply_rgb(vals[0], vals[1], vals[2]);
                                }
                            }
                            _ => {}
                        }
                        self.config.theme = ed.editing.clone();
                    }
                    ThemePickerFocus::Hex => {
                        match key.code {
                            KeyCode::Backspace => {
                                ed.field_value_mut().pop();
                                ed.sync_picker_from_field();
                            }
                            KeyCode::Char(c)
                                if (c.is_ascii_hexdigit() || c == '#')
                                    && ed.field_value(ed.field_idx).len() < 7 =>
                            {
                                ed.field_value_mut().push(c);
                                ed.sync_picker_from_field();
                            }
                            _ => {}
                        }
                        self.config.theme = ed.editing.clone();
                    }
                    ThemePickerFocus::FieldList => {}
                },
            }
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
                ed.sync_picker_from_field();
            }
            KeyCode::BackTab | KeyCode::Up => {
                ed.field_idx = if ed.field_idx == 0 {
                    ThemeEditorState::FIELD_LABELS.len() - 1
                } else {
                    ed.field_idx - 1
                };
                ed.sync_picker_from_field();
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
            KeyCode::Right | KeyCode::Char('p') if ed.field_idx > 0 => {
                ed.sync_picker_from_field();
                ed.picker_focus = ThemePickerFocus::Map2D;
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
                ed.sync_picker_from_field();
                self.config.theme = ed.editing.clone();
            }
            KeyCode::Char(c) => {
                ed.field_value_mut().push(c);
                ed.sync_picker_from_field();
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
                let editing_id = self.add_entity_state.editing_id.clone();
                self.active_modal = ActiveModal::None;

                if let Some(id) = editing_id {
                    match kind {
                        AddEntityKind::Project => {
                            if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
                                p.title = title.clone();
                                p.color = Some(color.clone());
                                p.theme = Some(crate::models::EntityTheme {
                                    primary: Some(color.clone()),
                                });
                            }
                            self.save_cache();
                            self.bridge.enqueue(BridgeAction::UpdateProject {
                                id,
                                title: Some(title),
                                color: Some(color),
                            });
                            self.status_message = if self.bridge.is_plugin_active() {
                                "Project update queued via SP bridge".to_string()
                            } else {
                                "Project updated locally".to_string()
                            };
                        }
                        AddEntityKind::Tag => {
                            if let Some(t) = self.tags.iter_mut().find(|t| t.id == id) {
                                t.title = title.clone();
                                t.color = Some(color.clone());
                            }
                            self.save_cache();
                            self.bridge.enqueue(BridgeAction::UpdateTag {
                                id,
                                title: Some(title),
                                color: Some(color),
                            });
                            self.status_message = if self.bridge.is_plugin_active() {
                                "Tag update queued via SP bridge".to_string()
                            } else {
                                "Tag updated locally".to_string()
                            };
                        }
                    }
                    return;
                }

                let local_id = match kind {
                    AddEntityKind::Project => {
                        format!("local-proj-{}", Local::now().timestamp_millis())
                    }
                    AddEntityKind::Tag => {
                        format!("local-tag-{}", Local::now().timestamp_millis())
                    }
                };
                match kind {
                    AddEntityKind::Project => {
                        let proj = Project {
                            id: local_id.clone(),
                            title: title.clone(),
                            theme: Some(crate::models::EntityTheme {
                                primary: Some(color.clone()),
                            }),
                            color: None,
                        };
                        self.projects.push(proj);
                        self.save_cache();
                        self.bridge.enqueue(BridgeAction::CreateProject {
                            local_id: local_id.clone(),
                            title: title.clone(),
                            color: color.clone(),
                        });
                    }
                    AddEntityKind::Tag => {
                        let tag = Tag {
                            id: local_id.clone(),
                            title: title.clone(),
                            color: Some(color.clone()),
                            theme: None,
                        };
                        self.tags.push(tag);
                        self.save_cache();
                        self.bridge.enqueue(BridgeAction::CreateTag {
                            local_id: local_id.clone(),
                            title: title.clone(),
                            color: color.clone(),
                        });
                    }
                }
                self.clamp_selections();
                self.status_message = if self.bridge.is_plugin_active() {
                    format!(
                        "{} queued via SP bridge",
                        if kind == AddEntityKind::Project {
                            "Project"
                        } else {
                            "Tag"
                        }
                    )
                } else {
                    format!(
                        "{} saved locally (install sp-tui-bridge plugin to sync to SP)",
                        if kind == AddEntityKind::Project {
                            "Project"
                        } else {
                            "Tag"
                        }
                    )
                };
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

    fn handle_history_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('H') => {
                self.active_modal = ActiveModal::None;
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.history_state.tab = self.history_state.tab.next();
                self.history_state.scroll = 0;
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.history_state.tab = self.history_state.tab.prev();
                self.history_state.scroll = 0;
            }
            KeyCode::Char('1') => {
                self.history_state.tab = HistoryTab::Completed;
                self.history_state.scroll = 0;
            }
            KeyCode::Char('2') => {
                self.history_state.tab = HistoryTab::Projects;
                self.history_state.scroll = 0;
            }
            KeyCode::Char('3') => {
                self.history_state.tab = HistoryTab::Tags;
                self.history_state.scroll = 0;
            }
            KeyCode::Char('f') | KeyCode::Char(' ') => {
                self.history_state.timeframe = self.history_state.timeframe.next();
                self.history_state.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.history_state.scroll = self.history_state.scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.history_state.scroll = self.history_state.scroll.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.history_state.scroll = 0;
            }
            _ => {}
        }
    }

    fn open_reschedule(&mut self, task_id: &str) {
        let existing_date = self
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .and_then(|t| t.due_day.as_deref())
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .unwrap_or_else(|| Local::now().date_naive());
        self.reschedule_state = Some(RescheduleState {
            task_id: task_id.to_string(),
            selected_date: existing_date,
            time_input: String::new(),
            focus_time: false,
        });
        self.active_modal = ActiveModal::Reschedule;
    }

    fn handle_counters_key(&mut self, key: KeyEvent) {
        if let Some(ref mut title) = self.counters_state.adding {
            match key.code {
                KeyCode::Esc => {
                    self.counters_state.adding = None;
                }
                KeyCode::Enter => {
                    let clean = title.trim().to_string();
                    self.counters_state.adding = None;
                    if !clean.is_empty() {
                        self.counters.push(SimpleCounter {
                            id: format!("local-cnt-{}", Local::now().timestamp_millis()),
                            title: clean,
                            count: 0,
                            is_on: false,
                        });
                        self.config.counters = self.counters.clone();
                        let _ = self.config.save();
                    }
                }
                KeyCode::Backspace => {
                    title.pop();
                }
                KeyCode::Char(c) => {
                    title.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('C') => {
                self.active_modal = ActiveModal::None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.counters_state.selected > 0 {
                    self.counters_state.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.counters_state.selected + 1 < self.counters.len() {
                    self.counters_state.selected += 1;
                }
            }
            KeyCode::Char('a') => {
                self.counters_state.adding = Some(String::new());
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right | KeyCode::Char('l') => {
                self.adjust_selected_counter(1);
            }
            KeyCode::Char('-') | KeyCode::Left | KeyCode::Char('h') => {
                self.adjust_selected_counter(-1);
            }
            KeyCode::Char('r') => {
                if let Some(c) = self.counters.get_mut(self.counters_state.selected) {
                    c.count = 0;
                    let id = c.id.clone();
                    self.config.counters = self.counters.clone();
                    let _ = self.config.save();
                    let client = self.client.clone();
                    let tx = self.api_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(updated) = client.update_counter(&id, 0).await {
                            let _ = tx.send(ApiEvent::CounterUpdated(updated));
                        }
                    });
                }
            }
            _ => {}
        }
    }

    fn adjust_selected_counter(&mut self, delta: i64) {
        if let Some(c) = self.counters.get_mut(self.counters_state.selected) {
            c.count = (c.count + delta).max(0);
            let id = c.id.clone();
            let count = c.count;
            self.config.counters = self.counters.clone();
            let _ = self.config.save();
            let client = self.client.clone();
            let tx = self.api_tx.clone();
            tokio::spawn(async move {
                if let Ok(updated) = client.update_counter(&id, count).await {
                    let _ = tx.send(ApiEvent::CounterUpdated(updated));
                }
            });
        }
    }

    fn handle_pomodoro_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('P') => {
                self.active_modal = ActiveModal::None;
            }
            KeyCode::Char(' ') => {
                self.pomodoro.reminder = None;
                self.pomodoro.running = !self.pomodoro.running;
                if self.pomodoro.running {
                    self.pomodoro.last_tick = Some(Instant::now());
                    if self.pomodoro.linked_task_id.is_none()
                        && let Some(t) = self.selected_task()
                    {
                        self.pomodoro.linked_task_id = Some(t.id.clone());
                    }
                    if self.pomodoro.phase == PomodoroPhase::Focus
                        && let Some(tid) = self.pomodoro.linked_task_id.clone()
                    {
                        self.current_task_id = Some(tid.clone());
                        self.timer_started_at = Some(Instant::now());
                        let client = self.client.clone();
                        tokio::spawn(async move {
                            let _ = client.set_current_task(Some(&tid)).await;
                        });
                    }
                } else {
                    self.pomodoro.last_tick = None;
                }
            }
            KeyCode::Char('m') | KeyCode::Tab => {
                self.pomodoro.running = false;
                self.pomodoro.last_tick = None;
                self.pomodoro.reminder = None;
                self.pomodoro.mode = self.pomodoro.mode.next();
                self.pomodoro.phase = PomodoroPhase::Focus;
                self.pomodoro.elapsed_secs = 0;
                self.pomodoro.remaining_secs = match self.pomodoro.mode {
                    FocusMode::Pomodoro => PomodoroPhase::Focus.duration_secs(),
                    FocusMode::Flowtime => 0,
                    FocusMode::Countdown => self.pomodoro.countdown_mins * 60,
                };
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right | KeyCode::Char('l')
                if !self.pomodoro.running && self.pomodoro.mode == FocusMode::Countdown =>
            {
                self.pomodoro.countdown_mins = (self.pomodoro.countdown_mins + 5).min(180);
                self.pomodoro.remaining_secs = self.pomodoro.countdown_mins * 60;
            }
            KeyCode::Char('-') | KeyCode::Left | KeyCode::Char('h')
                if !self.pomodoro.running && self.pomodoro.mode == FocusMode::Countdown =>
            {
                self.pomodoro.countdown_mins =
                    self.pomodoro.countdown_mins.saturating_sub(5).max(5);
                self.pomodoro.remaining_secs = self.pomodoro.countdown_mins * 60;
            }
            KeyCode::Char('r') => {
                self.pomodoro.running = false;
                self.pomodoro.last_tick = None;
                self.pomodoro.reminder = None;
                self.pomodoro.elapsed_secs = 0;
                self.pomodoro.remaining_secs = match self.pomodoro.mode {
                    FocusMode::Pomodoro => self.pomodoro.phase.duration_secs(),
                    FocusMode::Flowtime => 0,
                    FocusMode::Countdown => self.pomodoro.countdown_mins * 60,
                };
            }
            KeyCode::Char('n') => {
                self.pomodoro.running = false;
                self.pomodoro.last_tick = None;
                self.pomodoro.reminder = None;
                match self.pomodoro.mode {
                    FocusMode::Pomodoro => {
                        self.pomodoro.phase = match self.pomodoro.phase {
                            PomodoroPhase::Focus => PomodoroPhase::ShortBreak,
                            PomodoroPhase::ShortBreak => PomodoroPhase::LongBreak,
                            PomodoroPhase::LongBreak => PomodoroPhase::Focus,
                        };
                        self.pomodoro.remaining_secs = self.pomodoro.phase.duration_secs();
                    }
                    FocusMode::Flowtime => {
                        if self.pomodoro.phase == PomodoroPhase::Focus {
                            let earned_break = (self.pomodoro.elapsed_secs / 5).max(60);
                            self.pomodoro.completed_sessions += 1;
                            self.pomodoro.phase = PomodoroPhase::ShortBreak;
                            self.pomodoro.remaining_secs = earned_break;
                        } else {
                            self.pomodoro.phase = PomodoroPhase::Focus;
                            self.pomodoro.elapsed_secs = 0;
                        }
                    }
                    FocusMode::Countdown => {
                        self.pomodoro.remaining_secs = self.pomodoro.countdown_mins * 60;
                    }
                }
            }
            KeyCode::Char('l') => {
                if let Some(t) = self.selected_task() {
                    self.pomodoro.linked_task_id = Some(t.id.clone());
                }
            }
            _ => {}
        }
    }

    fn handle_reschedule_key(&mut self, key: KeyEvent) {
        let Some(ref mut rs) = self.reschedule_state else {
            self.active_modal = ActiveModal::None;
            return;
        };

        if key.code == KeyCode::Esc {
            self.reschedule_state = None;
            self.active_modal = ActiveModal::None;
            return;
        }

        if key.code == KeyCode::Tab {
            rs.focus_time = !rs.focus_time;
            return;
        }

        if key.code == KeyCode::Enter {
            let tid = rs.task_id.clone();
            let date_str = rs.selected_date.format("%Y-%m-%d").to_string();
            let time_str = rs.time_input.trim().to_string();
            let remind_at = if !time_str.is_empty()
                && let Ok(t) = chrono::NaiveTime::parse_from_str(&time_str, "%H:%M")
            {
                let dt = rs.selected_date.and_time(t);
                dt.and_local_timezone(Local)
                    .single()
                    .map(|ldt| ldt.timestamp_millis())
            } else {
                None
            };
            self.reschedule_state = None;
            self.active_modal = ActiveModal::None;
            if let Some(t) = self.tasks.iter_mut().find(|t| t.id == tid) {
                let prev_due = t.due_day.clone();
                t.due_day = Some(date_str.clone());
                self.push_undo(UndoOp::ScheduleTask {
                    id: tid.clone(),
                    prev_due,
                    new_due: Some(date_str.clone()),
                });
            }
            if let Some(ms) = remind_at {
                self.spawn_patch(tid, json!({ "dueDay": date_str, "remindAt": ms }));
            } else {
                self.spawn_patch(tid, json!({ "dueDay": date_str }));
            }
            return;
        }

        if rs.focus_time {
            match key.code {
                KeyCode::Backspace => {
                    rs.time_input.pop();
                }
                KeyCode::Char(c) if (c.is_ascii_digit() || c == ':') && rs.time_input.len() < 5 => {
                    rs.time_input.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                rs.selected_date -= ChronoDuration::days(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                rs.selected_date += ChronoDuration::days(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                rs.selected_date -= ChronoDuration::days(7);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                rs.selected_date += ChronoDuration::days(7);
            }
            KeyCode::Char('[') => {
                rs.selected_date -= ChronoDuration::days(30);
            }
            KeyCode::Char(']') => {
                rs.selected_date += ChronoDuration::days(30);
            }
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
            deadline_day: None,
            done_on: None,
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

    #[tokio::test]
    async fn live_health_transition_triggers_sync_and_preserves_cache_offline() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        app.tasks.push(Task {
            id: "t1".into(),
            title: "Cached task".into(),
            is_done: false,
            notes: None,
            project_id: None,
            parent_id: None,
            tag_ids: vec![],
            due_day: None,
            deadline_day: None,
            done_on: None,
            time_estimate: None,
            time_spent: None,
            repeat_cfg_id: None,
        });

        // Failed Loaded preserves cached tasks and sets Offline
        app.handle_api_event(ApiEvent::Loaded {
            tasks: vec![],
            projects: vec![],
            tags: vec![],
            counters: None,
            current_task_id: None,
            connected: false,
            message: String::new(),
        });
        assert_eq!(app.conn_state, ConnectionState::Offline);
        assert_eq!(app.tasks.len(), 1);

        // HealthCheck(true) transitions Offline -> Online -> Syncing
        app.handle_api_event(ApiEvent::HealthCheck(true));
        assert!(app.connected);
        assert_eq!(app.conn_state, ConnectionState::Syncing);
        assert!(app.sync_in_flight);

        // Successful Loaded transitions Syncing -> Online
        app.handle_api_event(ApiEvent::Loaded {
            tasks: app.tasks.clone(),
            projects: vec![],
            tags: vec![],
            counters: None,
            current_task_id: None,
            connected: true,
            message: String::new(),
        });
        assert_eq!(app.conn_state, ConnectionState::Online);
        assert!(!app.sync_in_flight);

        // HealthCheck(false) transitions Online -> Offline
        app.handle_api_event(ApiEvent::HealthCheck(false));
        assert!(!app.connected);
        assert_eq!(app.conn_state, ConnectionState::Offline);
    }

    #[tokio::test]
    async fn offline_mutations_queue_and_bridge_ack_maps_project_id() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        app.connected = false;

        // Create a local project
        app.add_entity_state = AddEntityState::new(AddEntityKind::Project);
        app.add_entity_state.title = "Side Quest".into();
        app.active_modal = ActiveModal::AddEntity;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.projects.len(), 1);
        let local_pid = app.projects[0].id.clone();
        assert!(local_pid.starts_with("local-proj-"));

        // Create a task offline assigned to that local project
        app.spawn_create(
            CreateTaskPayload {
                title: "Offline Task".into(),
                notes: None,
                project_id: Some(local_pid.clone()),
                parent_id: None,
                tag_ids: vec![],
                due_day: None,
                deadline_day: None,
                time_estimate: None,
                time_spent: None,
                is_add_to_top: Some(false),
            },
            false,
            true,
        );
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.offline_cache.pending_ops.len(), 1);

        // Bridge ack maps local-proj-* to real SP ID
        app.handle_api_event(ApiEvent::BridgeAcked {
            cmd: BridgeCommand {
                cmd_id: "cmd-1".into(),
                action: BridgeAction::CreateProject {
                    local_id: local_pid.clone(),
                    title: "Side Quest".into(),
                    color: "#89b4fa".into(),
                },
            },
            ack: BridgeAck {
                cmd_id: "cmd-1".into(),
                ok: true,
                entity_id: Some("real-sp-proj-123".into()),
                error: None,
            },
        });
        assert_eq!(app.projects[0].id, "real-sp-proj-123");
        assert_eq!(app.tasks[0].project_id.as_deref(), Some("real-sp-proj-123"));
    }
}
