use crate::api::SuperProductivityClient;
use crate::config::{AppConfig, KeybindMode};
use crate::keybinds::{JkEscapeTracker, JkResult, SequenceState};
use crate::models::{CreateTaskPayload, Project, Tag, Task};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddTaskField {
    Title,
    Note,
    Position,
    Project,
    Tags,
}

impl AddTaskField {
    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::Note,
            Self::Note => Self::Position,
            Self::Position => Self::Project,
            Self::Project => Self::Tags,
            Self::Tags => Self::Title,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Title => Self::Tags,
            Self::Note => Self::Title,
            Self::Position => Self::Note,
            Self::Project => Self::Position,
            Self::Tags => Self::Project,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddTaskState {
    pub title: String,
    pub note: String,
    pub add_to_top: bool,
    pub project_idx: usize,
    pub selected_tag_ids: Vec<String>,
    pub tag_cursor: usize,
    pub active_field: AddTaskField,
}

impl Default for AddTaskState {
    fn default() -> Self {
        Self {
            title: String::new(),
            note: String::new(),
            add_to_top: false,
            project_idx: 0,
            selected_tag_ids: Vec::new(),
            tag_cursor: 0,
            active_field: AddTaskField::Title,
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

#[derive(Debug)]
pub enum ApiEvent {
    Loaded {
        tasks: Vec<Task>,
        projects: Vec<Project>,
        tags: Vec<Tag>,
        connected: bool,
        message: String,
    },
    TaskUpdated(Task),
    TaskCreated(Task, bool),
    TaskDeleted(String),
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

    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub tags: Vec<Tag>,
    pub connected: bool,
    pub status_message: String,

    pub show_note: bool,
    pub renaming_task_id: Option<String>,
    pub rename_buffer: String,
    pub jk_tracker: JkEscapeTracker,
    pub sequence: SequenceState,

    pub active_modal: ActiveModal,
    pub help_search: String,
    pub settings_state: SettingsState,
    pub add_task_state: AddTaskState,

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
            tasks: Vec::new(),
            projects: Vec::new(),
            tags: Vec::new(),
            connected: false,
            status_message: "Connecting...".to_string(),
            show_note: false,
            renaming_task_id: None,
            rename_buffer: String::new(),
            jk_tracker: JkEscapeTracker::default(),
            sequence: SequenceState::default(),
            active_modal: ActiveModal::None,
            help_search: String::new(),
            settings_state,
            add_task_state: AddTaskState::default(),
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
                    match tasks {
                        Ok(tasks) => {
                            let _ = tx.send(ApiEvent::Loaded {
                                tasks,
                                projects,
                                tags,
                                connected: true,
                                message: "Connected to Super Productivity".to_string(),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(ApiEvent::Loaded {
                                tasks: Vec::new(),
                                projects,
                                tags,
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
                        connected: false,
                        message: format!("Server {} (ready={})", h.server, h.renderer_ready),
                    });
                }
                Err(e) => {
                    let _ = tx.send(ApiEvent::Loaded {
                        tasks: Vec::new(),
                        projects: Vec::new(),
                        tags: Vec::new(),
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
                connected,
                message,
            } => {
                self.tasks = tasks;
                self.projects = projects;
                self.tags = tags;
                self.connected = connected;
                self.status_message = message;
                self.clamp_selections();
            }
            ApiEvent::TaskUpdated(updated) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == updated.id) {
                    *t = updated;
                }
                self.status_message = "Task updated".to_string();
            }
            ApiEvent::TaskCreated(created, to_top) => {
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
        self.tasks
            .iter()
            .filter(|t| Self::matches_filter(t, &self.active_filter))
            .collect()
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.filtered_tasks().get(self.task_selected).copied()
    }

    fn clamp_selections(&mut self) {
        let nav_len = self.all_nav_rows().len();
        if nav_len == 0 {
            self.nav_selected = 0;
        } else if self.nav_selected >= nav_len {
            self.nav_selected = nav_len - 1;
        }

        let task_len = self.filtered_tasks().len();
        if task_len == 0 {
            self.task_selected = 0;
        } else if self.task_selected >= task_len {
            self.task_selected = task_len - 1;
        }
    }

    pub fn tick(&mut self) {
        if self.renaming_task_id.is_some() && self.jk_tracker.poll_expired() {
            self.rename_buffer.push('j');
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if self.renaming_task_id.is_some() {
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
            ActiveModal::None => {}
        }

        let mode = self.config.keybind_mode;

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
                'j' if mode.is_nvim_active() => self.move_down(),
                'k' if mode.is_nvim_active() => self.move_up(),
                'o' if mode.is_nvim_active() => self.activate_selected(),
                'p' if mode.is_normal_active() => self.open_properties(),
                'O' if mode.is_nvim_active() => self.open_properties(),
                'i' if mode.is_nvim_active() => self.start_rename(),
                'a' => {
                    self.add_task_state = AddTaskState::default();
                    self.active_modal = ActiveModal::AddTask;
                }
                'd' => self.delete_selected_task(),
                'n' if mode.is_normal_active() => self.show_note = !self.show_note,
                'K' if mode.is_nvim_active() => self.show_note = !self.show_note,
                _ => {}
            }
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.active_modal != ActiveModal::None || self.renaming_task_id.is_some() {
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
                let len = self.filtered_tasks().len();
                if self.task_selected + 1 < len {
                    self.task_selected += 1;
                }
            }
        }
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
        if self.focus == FocusSection::Main && self.selected_task().is_some() {
            self.active_modal = ActiveModal::Properties;
        }
    }

    fn start_rename(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.active_modal = ActiveModal::None;
            self.renaming_task_id = Some(task.id);
            self.rename_buffer = task.title;
            self.jk_tracker = JkEscapeTracker::default();
        }
    }

    fn delete_selected_task(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
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

    fn handle_properties_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.active_modal = ActiveModal::None,
            KeyCode::Char('r') => self.start_rename(),
            _ => {}
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
                self.renaming_task_id = None;
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
        if let Some(id) = self.renaming_task_id.take() {
            let new_title = self.rename_buffer.trim().to_string();
            self.rename_buffer.clear();
            if new_title.is_empty() {
                return;
            }
            let client = self.client.clone();
            let tx = self.api_tx.clone();
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

    fn handle_add_task_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.active_modal = ActiveModal::None,
            KeyCode::Tab | KeyCode::Down => {
                self.add_task_state.active_field = self.add_task_state.active_field.next();
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.add_task_state.active_field = self.add_task_state.active_field.prev();
            }
            KeyCode::Enter => {
                let title = self.add_task_state.title.trim().to_string();
                if title.is_empty() {
                    return;
                }
                let notes = if self.add_task_state.note.trim().is_empty() {
                    None
                } else {
                    Some(self.add_task_state.note.clone())
                };
                let project_id = if self.add_task_state.project_idx == 0 {
                    None
                } else {
                    self.projects
                        .get(self.add_task_state.project_idx - 1)
                        .map(|p| p.id.clone())
                };
                let to_top = self.add_task_state.add_to_top;
                let payload = CreateTaskPayload {
                    title,
                    notes,
                    project_id,
                    tag_ids: self.add_task_state.selected_tag_ids.clone(),
                    is_add_to_top: Some(to_top),
                };
                let client = self.client.clone();
                let tx = self.api_tx.clone();
                self.active_modal = ActiveModal::None;
                tokio::spawn(async move {
                    match client.create_task(payload).await {
                        Ok(t) => {
                            let _ = tx.send(ApiEvent::TaskCreated(t, to_top));
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
                AddTaskField::Title => self.add_task_state.title.push(' '),
                AddTaskField::Note => self.add_task_state.note.push(' '),
                AddTaskField::Project => {}
            },
            KeyCode::Backspace => match self.add_task_state.active_field {
                AddTaskField::Title => {
                    self.add_task_state.title.pop();
                }
                AddTaskField::Note => {
                    self.add_task_state.note.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match self.add_task_state.active_field {
                AddTaskField::Title => self.add_task_state.title.push(c),
                AddTaskField::Note => self.add_task_state.note.push(c),
                _ => {}
            },
            _ => {}
        }
    }
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
}
