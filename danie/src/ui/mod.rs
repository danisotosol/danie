mod views;

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail};
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use danie_core::{
    slugify, DanieStore, KnowledgeMap, LearnerProfile, PlanGraph, PlanNode, QuizLogEntry,
    QuizOutcome, SessionSummary, SrsCard, SrsQueue, StrandStatus,
};
use danie_llm::{Config, LlmProvider};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::runtime::Runtime;

use danie_engine::protocol::{PrereqDto, ProbeQuestionDto, QuizDto, TeachLessonDto};
use danie_engine::{self as engine, quality_value, QUALITY_LABELS};

pub const WRONG_MENU_OPTIONS: [&str; 4] = [
    "Retry quiz",
    "Insert prerequisite",
    "Mark known anyway",
    "End session",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Teach,
    ProbeOnly,
    Review,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Teach => "teach",
            Mode::ProbeOnly => "probe",
            Mode::Review => "review",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    TopicInput,
    ResumeModal,
    Dashboard,
    ProbeQuestion,
    ProbeDone,
    PlanView,
    Lesson,
    QuizPick,
    Reveal,
    QualityMenu,
    WrongMenu,
    Done,
    ReviewEmpty,
    ReviewQuestion,
    ReviewReveal,
    ReviewQuality,
    ReviewSummary,
}

#[derive(Debug)]
enum Action {
    GenerateProbe,
    GeneratePlan,
    Lesson(String),
    Prereq(String),
    ReviewQuestion(String),
}

impl Action {
    fn thinking_label(&self) -> &'static str {
        match self {
            Action::GenerateProbe => "Designing the diagnostic probe...",
            Action::GeneratePlan => "Planning your route...",
            Action::Lesson(_) => "Writing the lesson...",
            Action::Prereq(_) => "Finding a missing prerequisite...",
            Action::ReviewQuestion(_) => "Preparing the review question...",
        }
    }
}

enum StepResult {
    Probe(Vec<ProbeQuestionDto>),
    Plan(engine::PlanBundle),
    Lesson(TeachLessonDto, String),
    Prereq(PrereqDto, String),
    ReviewQ(QuizDto),
}

struct RevealInfo {
    chosen: usize,
    correct: bool,
}

struct PlanRow {
    depth: usize,
    title: String,
    arrow: String,
}

pub struct App {
    mode: Mode,
    store: DanieStore,
    store_dir: PathBuf,
    screen: Screen,

    input: String,
    resume_choice: usize,
    confirm_choice: usize,
    selected: usize,
    scroll: u16,

    goal: String,
    slug: String,
    profile: LearnerProfile,
    profile_default: bool,
    map: Option<KnowledgeMap>,
    queue: SrsQueue,

    graph: Option<PlanGraph>,
    nodes: BTreeMap<String, PlanNode>,
    edges: Vec<(String, String)>,
    plan_rows: Vec<PlanRow>,

    known: HashSet<String>,
    current_node: Option<String>,
    lesson: Option<TeachLessonDto>,
    reveal: Option<RevealInfo>,
    pending_quality_node: Option<String>,

    probe_questions: Vec<ProbeQuestionDto>,
    probe_index: usize,

    review_due: Vec<SrsCard>,
    review_index: usize,
    review_quiz: Option<QuizDto>,
    review_good: usize,
    review_again: usize,

    session_locked: Vec<String>,
    session_edge: Vec<String>,
    saved_paths: Vec<String>,
    summary_saved: bool,
    progress_made: bool,

    action: Option<Action>,
    thinking: Option<&'static str>,
    error: Option<String>,
    confirm_quit: bool,
    quit: bool,
}

fn is_effectively_default(profile: &LearnerProfile) -> bool {
    profile.solid_ground.is_empty()
        && profile.goals.is_empty()
        && profile.pace_notes.is_none()
        && profile.struggle_prefs.is_none()
        && profile.voice_prefs.is_none()
}

impl App {
    fn new(mode: Mode, topic: Option<String>, store_dir: &Path) -> anyhow::Result<Self> {
        let store = DanieStore::open(store_dir)?;
        let (profile, profile_warning) = match store.load_profile() {
            Ok(profile) => (profile, None),
            Err(error) => (
                LearnerProfile::default(),
                Some(format!("Could not parse {store_dir:?}/profile.md ({error}); using defaults. It will be rewritten in canonical format on first save.")),
            ),
        };
        if let Some(warning) = &profile_warning {
            eprintln!("warning: {warning}");
        }
        let profile_default = is_effectively_default(&profile);
        let (queue, queue_warning) = match store.load_queue() {
            Ok(queue) => (queue, None),
            Err(error) => (
                SrsQueue::default(),
                Some(format!(
                    "Could not parse srs.json ({error}); starting an empty review schedule."
                )),
            ),
        };
        if let Some(warning) = &queue_warning {
            eprintln!("warning: {warning}");
        }
        let mut app = Self {
            mode,
            store,
            store_dir: store_dir.to_path_buf(),
            screen: Screen::TopicInput,
            input: String::new(),
            resume_choice: 0,
            confirm_choice: 0,
            selected: 0,
            scroll: 0,
            goal: String::new(),
            slug: String::new(),
            profile,
            profile_default,
            map: None,
            queue,
            graph: None,
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            plan_rows: Vec::new(),
            known: HashSet::new(),
            current_node: None,
            lesson: None,
            reveal: None,
            pending_quality_node: None,
            probe_questions: Vec::new(),
            probe_index: 0,
            review_due: Vec::new(),
            review_index: 0,
            review_quiz: None,
            review_good: 0,
            review_again: 0,
            session_locked: Vec::new(),
            session_edge: Vec::new(),
            saved_paths: Vec::new(),
            summary_saved: false,
            progress_made: false,
            action: None,
            thinking: None,
            error: None,
            confirm_quit: false,
            quit: false,
        };
        match mode {
            Mode::Review => app.init_review(),
            Mode::Teach | Mode::ProbeOnly => {
                if let Some(topic) = topic {
                    app.set_topic(topic);
                }
            }
        }
        Ok(app)
    }

    fn set_topic(&mut self, topic: String) {
        let goal = topic.trim().to_string();
        if goal.is_empty() {
            self.error = Some("Please type a topic before continuing.".into());
            return;
        }
        let slug = slugify(&goal);
        if slug.is_empty() {
            self.error =
                Some("That topic cannot be turned into a valid name. Try another wording.".into());
            return;
        }
        self.goal = goal;
        self.slug = slug;
        if self.store.list_maps().contains(&self.slug) {
            self.resume_choice = 0;
            self.screen = Screen::ResumeModal;
        } else {
            self.map = Some(KnowledgeMap::new(self.goal.clone()));
            self.screen = Screen::Dashboard;
        }
    }

    fn apply_resume(&mut self, fresh: bool) {
        if fresh {
            self.map = Some(KnowledgeMap::new(self.goal.clone()));
            self.known.clear();
            self.screen = Screen::Dashboard;
            return;
        }
        match self.store.load_map(&self.slug) {
            Ok(map) => {
                self.known = map
                    .strands_with(StrandStatus::Known)
                    .iter()
                    .map(|strand| engine::normalize_id(&strand.name))
                    .collect();
                self.map = Some(map);
                match self.store.load_plan(&self.slug) {
                    Ok(Some(plan)) => {
                        if plan.node_count() > 0 {
                            self.install_plan(plan);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.error = Some(format!("Could not load the stored plan: {error}"));
                    }
                }
                self.screen = Screen::Dashboard;
            }
            Err(error) => {
                self.error = Some(format!("Could not load the stored map: {error}"));
                self.screen = Screen::TopicInput;
            }
        }
    }

    fn init_review(&mut self) {
        let due: Vec<SrsCard> = self
            .queue
            .due_cards(Utc::now())
            .into_iter()
            .cloned()
            .collect();
        if due.is_empty() {
            self.screen = Screen::ReviewEmpty;
        } else {
            let first = due[0].node.clone();
            self.review_due = due;
            self.review_index = 0;
            self.screen = Screen::ReviewQuestion;
            self.action = Some(Action::ReviewQuestion(first));
        }
    }

    fn strand_context(&self, node_id: &str) -> Option<String> {
        for slug in self.store.list_maps() {
            if let Ok(map) = self.store.load_map(&slug) {
                if let Some(strand) = map
                    .strands
                    .iter()
                    .find(|candidate| candidate.name.eq_ignore_ascii_case(node_id))
                {
                    return Some(format!(
                        "status {}; evidence {}",
                        strand.status, strand.evidence
                    ));
                }
            }
        }
        None
    }

    fn prereq_titles_of(&self, id: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|(_, after)| after == id)
            .filter_map(|(before, _)| self.nodes.get(before).map(|n| n.title.clone()))
            .collect()
    }

    fn next_target(&self) -> Option<String> {
        self.graph
            .as_ref()
            .and_then(|graph| graph.next_unlocked(&self.known))
            .map(|node| node.id.clone())
    }

    fn note_path(&mut self, path: PathBuf) {
        let text = path.display().to_string();
        if !self.saved_paths.contains(&text) {
            self.saved_paths.push(text);
        }
    }

    fn persist_map(&mut self) {
        let Some(map) = &self.map else {
            return;
        };
        match self.store.save_map(map) {
            Ok(path) => self.note_path(path),
            Err(error) => self.error = Some(format!("Failed to save the knowledge map: {error}")),
        }
    }

    fn persist_queue(&mut self) {
        match self.store.save_queue(&self.queue) {
            Ok(()) => self.note_path(self.store_dir.join("srs.json")),
            Err(error) => self.error = Some(format!("Failed to save the review schedule: {error}")),
        }
    }

    fn persist_plan(&mut self) {
        let Some(graph) = &self.graph else {
            return;
        };
        match self.store.save_plan(&self.slug, graph) {
            Ok(path) => self.note_path(path),
            Err(error) => self.error = Some(format!("Failed to save the learning plan: {error}")),
        }
    }

    fn install_plan(&mut self, plan: PlanGraph) {
        let mut nodes: std::collections::BTreeMap<String, PlanNode> =
            std::collections::BTreeMap::new();
        for node in plan.nodes() {
            nodes.insert(node.id.clone(), node.clone());
        }
        self.edges = plan.edges();
        self.graph = Some(plan);
        self.nodes = nodes;
        self.rebuild_plan_rows();
        self.current_node = self.next_target();
    }

    fn finish_session(&mut self, notes: &str) {
        let next_node = self
            .graph
            .as_ref()
            .and_then(|graph| graph.next_unlocked(&self.known))
            .map(|node| node.title.clone());
        let summary = SessionSummary {
            date: Utc::now(),
            topic: self.goal.clone(),
            locked: self.session_locked.clone(),
            edge: self.session_edge.clone(),
            next_node,
            notes: notes.to_string(),
        };
        match self.store.save_session(&summary) {
            Ok(path) => self.note_path(path),
            Err(error) => self.error = Some(format!("Failed to save the session summary: {error}")),
        }
        self.summary_saved = true;
        self.screen = Screen::Done;
    }

    fn push_unique(list: &mut Vec<String>, value: String) {
        if !list.contains(&value) {
            list.push(value);
        }
    }

    fn rebuild_plan_rows(&mut self) {
        let Some(graph) = &self.graph else {
            return;
        };
        let order: Vec<String> = match graph.topo_order() {
            Ok(nodes) => nodes.into_iter().map(|n| n.id.clone()).collect(),
            Err(_) => self.nodes.keys().cloned().collect(),
        };
        let mut depths: BTreeMap<String, usize> = BTreeMap::new();
        let mut rows = Vec::new();
        for id in &order {
            let prereqs: Vec<String> = self
                .edges
                .iter()
                .filter(|(_, after)| after == id)
                .map(|(before, _)| before.clone())
                .collect();
            let depth = prereqs
                .iter()
                .filter_map(|prereq| depths.get(prereq))
                .copied()
                .max()
                .map_or(0, |d| d + 1);
            depths.insert(id.clone(), depth);
            let title = self
                .nodes
                .get(id)
                .map(|node| node.title.clone())
                .unwrap_or_else(|| id.clone());
            let arrow = if prereqs.is_empty() {
                String::new()
            } else {
                format!("{} -> {}", prereqs.join(", "), id)
            };
            rows.push(PlanRow {
                depth,
                title,
                arrow,
            });
        }
        self.plan_rows = rows;
    }

    fn move_selection(&mut self, delta: i32, len: usize) {
        if len == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).clamp(0, len as i32 - 1);
        self.selected = next as usize;
    }

    fn scroll_by(&mut self, delta: i16) {
        if delta < 0 {
            self.scroll = self.scroll.saturating_sub(delta.unsigned_abs());
        } else {
            self.scroll = self.scroll.saturating_add(delta.unsigned_abs());
        }
    }

    fn digit_choice(key: &KeyEvent, max: usize) -> Option<usize> {
        if let KeyCode::Char(c) = key.code {
            if let Some(digit) = c.to_digit(10) {
                let digit = digit as usize;
                if (1..=max).contains(&digit) {
                    return Some(digit - 1);
                }
            }
        }
        None
    }

    fn request_quit(&mut self) {
        if self.mode == Mode::Teach {
            self.confirm_quit = true;
            self.confirm_choice = 0;
        } else {
            self.quit = true;
        }
    }

    fn quit_and_save(&mut self) {
        if self.mode == Mode::Teach && self.progress_made && !self.summary_saved {
            self.finish_session("Ended early.");
        }
        self.quit = true;
    }

    fn on_key_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirm_quit = false;
                self.quit_and_save();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm_quit = false;
            }
            KeyCode::Left | KeyCode::Char('h') => self.confirm_choice = 0,
            KeyCode::Right | KeyCode::Char('l') => self.confirm_choice = 1,
            KeyCode::Up | KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('k') => {
                self.confirm_choice ^= 1;
            }
            KeyCode::Enter => {
                let choice = self.confirm_choice;
                self.confirm_quit = false;
                if choice == 0 {
                    self.quit_and_save();
                }
            }
            _ => {}
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.request_quit();
            return;
        }
        if self.error.is_some() {
            self.error = None;
            return;
        }
        if self.confirm_quit {
            self.on_key_confirm(key);
            return;
        }
        match self.screen {
            Screen::TopicInput => self.on_key_topic(key),
            Screen::ResumeModal => self.on_key_resume(key),
            Screen::Dashboard => self.on_key_dashboard(key),
            Screen::ProbeQuestion => self.on_key_probe_question(key),
            Screen::ProbeDone => self.on_key_any_exit(key),
            Screen::PlanView => self.on_key_plan(key),
            Screen::Lesson => self.on_key_lesson(key),
            Screen::QuizPick => self.on_key_quiz_pick(key),
            Screen::Reveal => self.on_key_reveal(key),
            Screen::QualityMenu => self.on_key_quality(key),
            Screen::WrongMenu => self.on_key_wrong_menu(key),
            Screen::Done => self.on_key_any_exit(key),
            Screen::ReviewEmpty => self.on_key_any_exit(key),
            Screen::ReviewQuestion => self.on_key_review_question(key),
            Screen::ReviewReveal => self.on_key_review_reveal(key),
            Screen::ReviewQuality => self.on_key_review_quality(key),
            Screen::ReviewSummary => self.on_key_any_exit(key),
        }
    }

    fn on_key_any_exit(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) {
            self.quit = true;
        }
    }

    fn on_key_topic(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => {
                let topic = std::mem::take(&mut self.input);
                self.set_topic(topic);
            }
            KeyCode::Esc => self.request_quit(),
            _ => {}
        }
    }

    fn on_key_resume(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Tab
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Char('h')
            | KeyCode::Char('l')
            | KeyCode::Char('j')
            | KeyCode::Char('k') => self.resume_choice ^= 1,
            KeyCode::Char('1') => self.resume_choice = 0,
            KeyCode::Char('2') => self.resume_choice = 1,
            KeyCode::Enter => {
                let fresh = self.resume_choice == 1;
                self.apply_resume(fresh);
            }
            KeyCode::Esc => self.apply_resume(false),
            _ => {}
        }
    }

    fn on_key_dashboard(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let has_strands = self.map.as_ref().is_some_and(|m| !m.strands.is_empty());
                match self.mode {
                    Mode::Teach => {
                        if has_strands {
                            self.action = Some(Action::GeneratePlan);
                        } else {
                            self.action = Some(Action::GenerateProbe);
                        }
                    }
                    Mode::ProbeOnly => {
                        if has_strands {
                            self.screen = Screen::ProbeDone;
                        } else {
                            self.action = Some(Action::GenerateProbe);
                        }
                    }
                    Mode::Review => {}
                }
            }
            KeyCode::Esc => self.request_quit(),
            _ => {}
        }
    }

    fn on_key_probe_question(&mut self, key: KeyEvent) {
        let count = self
            .probe_questions
            .get(self.probe_index)
            .map(|question| question.options.len())
            .unwrap_or(0);
        if let Some(choice) = Self::digit_choice(&key, count) {
            self.submit_probe_answer(choice);
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, count),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, count),
            KeyCode::Enter => self.submit_probe_answer(self.selected),
            KeyCode::Esc => self.request_quit(),
            _ => {}
        }
    }

    fn submit_probe_answer(&mut self, chosen: usize) {
        let Some(question) = self.probe_questions.get(self.probe_index).cloned() else {
            return;
        };
        let Some(map) = self.map.as_mut() else {
            return;
        };
        engine::score_probe(map, &question, Some(chosen));
        self.progress_made = true;
        self.persist_map();
        self.probe_index += 1;
        self.selected = 0;
        if self.probe_index >= self.probe_questions.len() {
            match self.mode {
                Mode::Teach => {
                    self.action = Some(Action::GeneratePlan);
                    self.screen = Screen::PlanView;
                }
                Mode::ProbeOnly => self.screen = Screen::ProbeDone,
                Mode::Review => {}
            }
        }
    }

    fn on_key_plan(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::PageUp => self.scroll_by(-3),
            KeyCode::Down | KeyCode::PageDown => self.scroll_by(3),
            KeyCode::Char('k') => self.scroll_by(-1),
            KeyCode::Char('j') => self.scroll_by(1),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Enter => {
                let target = self.current_node.clone().or_else(|| self.next_target());
                match target {
                    Some(id) => {
                        self.current_node = Some(id.clone());
                        self.action = Some(Action::Lesson(id));
                        self.selected = 0;
                    }
                    None => self.finish_session("Session complete."),
                }
            }
            KeyCode::Esc => self.request_quit(),
            _ => {}
        }
    }

    fn on_key_lesson(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.scroll_by(-3),
            KeyCode::Down => self.scroll_by(3),
            KeyCode::Char('k') => self.scroll_by(-1),
            KeyCode::Char('j') => self.scroll_by(1),
            KeyCode::Enter => {
                self.selected = 0;
                self.screen = Screen::QuizPick;
            }
            KeyCode::Esc => self.screen = Screen::PlanView,
            _ => {}
        }
    }

    fn on_key_quiz_pick(&mut self, key: KeyEvent) {
        let count = self
            .lesson
            .as_ref()
            .map(|lesson| lesson.quiz.options.len())
            .unwrap_or(0);
        if let Some(choice) = Self::digit_choice(&key, count) {
            self.submit_teach_answer(choice);
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, count),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, count),
            KeyCode::Enter => self.submit_teach_answer(self.selected),
            KeyCode::Esc => self.screen = Screen::PlanView,
            _ => {}
        }
    }

    fn submit_teach_answer(&mut self, chosen: usize) {
        let Some(lesson) = self.lesson.clone() else {
            return;
        };
        let Some(node_id) = self.current_node.clone() else {
            return;
        };
        let correct = chosen == lesson.quiz.correct_index;
        let answer_text = lesson
            .quiz
            .options
            .get(chosen)
            .cloned()
            .unwrap_or_else(|| "(no answer)".into());
        let outcome = if correct {
            QuizOutcome::Correct
        } else {
            QuizOutcome::Wrong
        };
        let status = if correct {
            StrandStatus::Known
        } else {
            StrandStatus::Edge
        };
        let evidence = if correct {
            format!("locked in: {}", lesson.quiz.prompt)
        } else {
            format!(
                "failed lock-in quiz: chose {:?}, expected {:?}",
                answer_text,
                lesson
                    .quiz
                    .options
                    .get(lesson.quiz.correct_index)
                    .cloned()
                    .unwrap_or_default()
            )
        };
        let node_title = self
            .nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.clone());
        if let Some(map) = self.map.as_mut() {
            map.upsert_strand(node_id.clone(), status, evidence);
            map.log_quiz(QuizLogEntry {
                strand: node_id.clone(),
                answer: answer_text,
                outcome,
            });
        }
        self.progress_made = true;
        if correct {
            Self::push_unique(&mut self.session_locked, node_title);
        } else {
            Self::push_unique(&mut self.session_edge, node_title);
        }
        self.persist_map();
        self.reveal = Some(RevealInfo { chosen, correct });
        self.selected = chosen;
        self.screen = Screen::Reveal;
    }

    fn on_key_reveal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let correct = self.reveal.as_ref().is_some_and(|info| info.correct);
                if correct {
                    self.pending_quality_node = self.current_node.clone();
                    self.selected = 2;
                    self.screen = Screen::QualityMenu;
                } else {
                    self.selected = 0;
                    self.screen = Screen::WrongMenu;
                }
            }
            KeyCode::Esc => self.screen = Screen::PlanView,
            _ => {}
        }
    }

    fn on_key_quality(&mut self, key: KeyEvent) {
        if let Some(choice) = Self::digit_choice(&key, QUALITY_LABELS.len()) {
            self.pick_quality(choice);
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, QUALITY_LABELS.len()),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, QUALITY_LABELS.len()),
            KeyCode::Enter => self.pick_quality(self.selected),
            KeyCode::Esc => self.screen = Screen::Reveal,
            _ => {}
        }
    }

    fn pick_quality(&mut self, choice: usize) {
        let Some(node_id) = self.pending_quality_node.clone() else {
            return;
        };
        let quality = quality_value(choice);
        self.queue.upsert_card(node_id.clone());
        if let Some(card) = self
            .queue
            .cards
            .iter_mut()
            .find(|card| card.node == node_id)
        {
            if let Err(error) = card.review(quality) {
                self.error = Some(format!("Could not schedule the review card: {error}"));
                return;
            }
        }
        self.persist_queue();
        self.advance_teach_loop();
    }

    fn on_key_wrong_menu(&mut self, key: KeyEvent) {
        if let Some(choice) = Self::digit_choice(&key, WRONG_MENU_OPTIONS.len()) {
            self.choose_wrong_action(choice);
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, WRONG_MENU_OPTIONS.len()),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, WRONG_MENU_OPTIONS.len()),
            KeyCode::Enter => self.choose_wrong_action(self.selected),
            KeyCode::Esc => self.screen = Screen::Reveal,
            _ => {}
        }
    }

    fn choose_wrong_action(&mut self, choice: usize) {
        match choice {
            0 => {
                self.reveal = None;
                self.selected = 0;
                self.screen = Screen::QuizPick;
            }
            1 => {
                let Some(current) = self.current_node.clone() else {
                    return;
                };
                self.action = Some(Action::Prereq(current));
            }
            2 => {
                let Some(node_id) = self.current_node.clone() else {
                    return;
                };
                let node_title = self
                    .nodes
                    .get(&node_id)
                    .map(|node| node.title.clone())
                    .unwrap_or_else(|| node_id.clone());
                if let Some(map) = self.map.as_mut() {
                    map.upsert_strand(
                        node_id.clone(),
                        StrandStatus::Known,
                        "marked known despite failed lock-in quiz".to_string(),
                    );
                }
                Self::push_unique(&mut self.session_locked, node_title);
                self.progress_made = true;
                self.persist_map();
                self.known.insert(node_id.clone());
                self.pending_quality_node = Some(node_id);
                self.selected = 1;
                self.screen = Screen::QualityMenu;
            }
            3 => self.finish_session("Ended early."),
            _ => {}
        }
    }

    fn advance_teach_loop(&mut self) {
        if let Some(done) = self.pending_quality_node.take() {
            self.known.insert(done);
        }
        self.proceed_after_node();
    }

    fn proceed_after_node(&mut self) {
        self.lesson = None;
        self.reveal = None;
        self.selected = 0;
        match self.next_target() {
            Some(id) => {
                self.current_node = Some(id.clone());
                self.action = Some(Action::Lesson(id));
                self.screen = Screen::PlanView;
            }
            None => self.finish_session("Session complete."),
        }
    }

    fn on_key_review_question(&mut self, key: KeyEvent) {
        let count = self
            .review_quiz
            .as_ref()
            .map(|quiz| quiz.options.len())
            .unwrap_or(0);
        if let Some(choice) = Self::digit_choice(&key, count) {
            self.submit_review_answer(choice);
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, count),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, count),
            KeyCode::Enter => self.submit_review_answer(self.selected),
            KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn submit_review_answer(&mut self, chosen: usize) {
        let Some(quiz) = self.review_quiz.clone() else {
            return;
        };
        self.selected = chosen;
        self.reveal = Some(RevealInfo {
            chosen,
            correct: chosen == quiz.correct_index,
        });
        self.screen = Screen::ReviewReveal;
    }

    fn on_key_review_reveal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.selected = 2;
                self.screen = Screen::ReviewQuality;
            }
            KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn on_key_review_quality(&mut self, key: KeyEvent) {
        if let Some(choice) = Self::digit_choice(&key, QUALITY_LABELS.len()) {
            self.apply_review_quality(choice);
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, QUALITY_LABELS.len()),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, QUALITY_LABELS.len()),
            KeyCode::Enter => self.apply_review_quality(self.selected),
            KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn apply_review_quality(&mut self, choice: usize) {
        let quality = quality_value(choice);
        let Some(node) = self
            .review_due
            .get(self.review_index)
            .map(|c| c.node.clone())
        else {
            return;
        };
        if let Some(card) = self.queue.cards.iter_mut().find(|card| card.node == node) {
            if let Err(error) = card.review(quality) {
                self.error = Some(format!("Could not schedule the review card: {error}"));
                return;
            }
        }
        self.persist_queue();
        if quality >= 3 {
            self.review_good += 1;
        } else {
            self.review_again += 1;
        }
        self.reveal = None;
        self.review_index += 1;
        match self.review_due.get(self.review_index) {
            Some(next) => {
                self.action = Some(Action::ReviewQuestion(next.node.clone()));
                self.screen = Screen::ReviewQuestion;
            }
            None => self.screen = Screen::ReviewSummary,
        }
    }

    fn apply(&mut self, result: StepResult) {
        match result {
            StepResult::Probe(questions) => {
                self.probe_questions = questions;
                self.probe_index = 0;
                self.selected = 0;
                self.screen = Screen::ProbeQuestion;
            }
            StepResult::Plan(bundle) => {
                self.graph = Some(bundle.graph);
                self.nodes = bundle.nodes;
                self.edges = bundle.edges;
                self.rebuild_plan_rows();
                self.current_node = self.next_target();
                self.scroll = 0;
                self.screen = Screen::PlanView;
                self.persist_plan();
            }
            StepResult::Lesson(lesson, id) => {
                self.lesson = Some(lesson);
                self.current_node = Some(id);
                self.selected = 0;
                self.scroll = 0;
                self.screen = Screen::Lesson;
            }
            StepResult::Prereq(dto, current_id) => self.apply_prereq(dto, &current_id),
            StepResult::ReviewQ(quiz) => {
                self.review_quiz = Some(quiz);
                self.selected = 0;
                self.screen = Screen::ReviewQuestion;
            }
        }
    }

    fn apply_prereq(&mut self, dto: PrereqDto, current_id: &str) {
        let id = engine::normalize_id(&dto.id);
        if id.is_empty() {
            self.error = Some("The model returned an unusable prerequisite id. Try again.".into());
            return;
        }
        if self.nodes.contains_key(&id) {
            self.error = Some(format!(
                "The model proposed an existing node id ({id}). Try inserting again."
            ));
            return;
        }
        let plan_node = PlanNode {
            id: id.clone(),
            title: dto.title.trim().to_string(),
            summary: dto.summary.trim().to_string(),
        };
        {
            let Some(graph) = self.graph.as_mut() else {
                self.error = Some("No active learning plan to extend.".into());
                return;
            };
            if let Err(error) = graph.add_node(plan_node.clone()) {
                self.error = Some(format!("Could not add the prerequisite node: {error}"));
                return;
            }
            if let Err(error) = graph.add_prereq(&id, current_id) {
                self.error = Some(format!("Could not link the prerequisite: {error}"));
                return;
            }
        }
        self.nodes.insert(id.clone(), plan_node);
        self.edges.push((id.clone(), current_id.to_string()));
        self.rebuild_plan_rows();
        self.persist_plan();
        self.current_node = Some(id.clone());
        self.action = Some(Action::Lesson(id));
        self.screen = Screen::PlanView;
    }
}

async fn run_action(
    app: &App,
    provider: &dyn LlmProvider,
    action: Action,
) -> anyhow::Result<StepResult> {
    match action {
        Action::GenerateProbe => Ok(StepResult::Probe(
            engine::generate_probe(provider, &app.goal, &app.profile).await?,
        )),
        Action::GeneratePlan => {
            let map = app
                .map
                .as_ref()
                .ok_or_else(|| anyhow!("no knowledge map loaded"))?;
            Ok(StepResult::Plan(
                engine::generate_plan(provider, &app.goal, map, &app.profile).await?,
            ))
        }
        Action::Lesson(id) => {
            let node = app
                .nodes
                .get(&id)
                .ok_or_else(|| anyhow!("unknown node {id}"))?
                .clone();
            let prereqs = app.prereq_titles_of(&id);
            let map = app
                .map
                .as_ref()
                .ok_or_else(|| anyhow!("no knowledge map loaded"))?;
            Ok(StepResult::Lesson(
                engine::generate_lesson(provider, &app.goal, &node, &prereqs, map, &app.profile)
                    .await?,
                id,
            ))
        }
        Action::Prereq(id) => {
            let node = app
                .nodes
                .get(&id)
                .ok_or_else(|| anyhow!("unknown node {id}"))?
                .clone();
            let existing_ids: Vec<String> = app.nodes.keys().cloned().collect();
            Ok(StepResult::Prereq(
                engine::propose_prerequisite(
                    provider,
                    &app.goal,
                    &node,
                    &existing_ids,
                    &app.profile,
                )
                .await?,
                id,
            ))
        }
        Action::ReviewQuestion(node) => {
            let context = app.strand_context(&node);
            Ok(StepResult::ReviewQ(
                engine::generate_review_question(provider, &node, context.as_deref(), &app.profile)
                    .await?,
            ))
        }
    }
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

fn event_loop(
    mode: Mode,
    topic: Option<String>,
    store_dir: &Path,
    rt: &Runtime,
    provider: &dyn LlmProvider,
    terminal: &mut Tui,
) -> anyhow::Result<()> {
    let mut app = App::new(mode, topic, store_dir)?;
    loop {
        terminal.draw(|frame| views::draw(frame, &app))?;
        if let Some(action) = app.action.take() {
            app.thinking = Some(action.thinking_label());
            terminal.draw(|frame| views::draw(frame, &app))?;
            let step = rt.block_on(run_action(&app, provider, action));
            app.thinking = None;
            match step {
                Ok(result) => app.apply(result),
                Err(error) => {
                    tracing::warn!(%error, "engine step failed");
                    app.error = Some(format!("{error:#}"));
                }
            }
            continue;
        }
        if app.quit {
            break;
        }
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
    }
    if mode == Mode::Teach && app.progress_made && !app.summary_saved {
        app.finish_session("Ended early.");
    }
    Ok(())
}

pub fn run_app(
    mode: Mode,
    topic: Option<String>,
    store_dir: &Path,
    rt: &Runtime,
) -> anyhow::Result<()> {
    crate::logging::init(store_dir)?;
    let config = Config::load()?;
    let provider = match config.create_provider() {
        Ok(provider) => provider,
        Err(error) => {
            crate::doctor::print_setup_guidance(&error);
            bail!("provider unavailable: {error}");
        }
    };
    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let outcome = event_loop(mode, topic, store_dir, rt, provider.as_ref(), &mut terminal);
    restore_terminal();
    if let Err(error) = &outcome {
        tracing::warn!(%error, "tui exited with an error");
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_app(mode: Mode) -> (App, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "danie-ui-{}-{}-{nanos}",
            mode.label(),
            std::process::id()
        ));
        (App::new(mode, None, &dir).unwrap(), dir)
    }

    fn two_node_plan(app: &mut App) {
        let mut graph = PlanGraph::new();
        for id in ["a", "b"] {
            graph
                .add_node(PlanNode {
                    id: id.into(),
                    title: id.to_uppercase(),
                    summary: String::new(),
                })
                .unwrap();
        }
        graph.add_prereq("a", "b").unwrap();
        app.graph = Some(graph);
        app.nodes.insert(
            "a".into(),
            PlanNode {
                id: "a".into(),
                title: "A".into(),
                summary: String::new(),
            },
        );
        app.nodes.insert(
            "b".into(),
            PlanNode {
                id: "b".into(),
                title: "B".into(),
                summary: String::new(),
            },
        );
        app.edges = vec![("a".into(), "b".into())];
        app.map = Some(KnowledgeMap::new("t"));
    }

    #[test]
    fn fresh_topic_without_stored_map_lands_on_dashboard() {
        let (mut app, dir) = temp_app(Mode::Teach);
        app.set_topic("Recursion Basics!".into());
        assert_eq!(app.slug, "recursion-basics");
        assert_eq!(app.screen, Screen::Dashboard);
        assert!(app.map.is_some());
        assert!(app.error.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn marking_a_node_known_anyway_advances_past_it() {
        let (mut app, dir) = temp_app(Mode::Teach);
        app.set_topic("t".into());
        two_node_plan(&mut app);
        app.current_node = Some("a".into());

        app.choose_wrong_action(2);

        assert!(app.known.contains("a"));
        assert_eq!(app.session_locked, vec!["A"]);
        assert_eq!(app.screen, Screen::QualityMenu);
        assert_eq!(app.pending_quality_node.as_deref(), Some("a"));

        app.pick_quality(2);

        let card = app.queue.cards.iter().find(|c| c.node == "a").unwrap();
        assert_eq!(card.reps, 1);
        assert_eq!(card.interval_days, 1);
        match &app.action {
            Some(Action::Lesson(id)) => assert_eq!(id, "b"),
            other => panic!("expected a lesson action for node b, got {other:?}"),
        }
        assert_eq!(app.screen, Screen::PlanView);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn completing_the_last_node_finishes_the_session() {
        let (mut app, dir) = temp_app(Mode::Teach);
        app.set_topic("t".into());
        two_node_plan(&mut app);
        app.known.insert("a".into());
        app.pending_quality_node = Some("b".into());

        app.advance_teach_loop();

        assert!(app.summary_saved);
        assert_eq!(app.screen, Screen::Done);
        assert!(app.saved_paths.iter().any(|p| p.ends_with(".md")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
