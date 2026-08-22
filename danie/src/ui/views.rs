use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use ratatui::Frame;

use super::{App, Screen, WRONG_MENU_OPTIONS};
use crate::engine::QUALITY_LABELS;
use crate::textutil::{strip_inline_markdown, wrap_text};

const THINKING_TEXT: &str = " Thinking... calling the model ";

fn bold(text: impl Into<String>) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
}

fn dim(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(Color::DarkGray))
}

fn accent(text: impl Into<String>) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn status_color(status: danie_core::StrandStatus) -> Color {
    match status {
        danie_core::StrandStatus::Known => Color::Green,
        danie_core::StrandStatus::Edge => Color::Yellow,
        danie_core::StrandStatus::Unknown => Color::DarkGray,
        danie_core::StrandStatus::Blocked => Color::Red,
    }
}

fn panel(title: impl AsRef<str>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" {} ", title.as_ref()))
        .title_style(
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
}

fn help_text(screen: Screen) -> &'static str {
    match screen {
        Screen::TopicInput => " type the topic | Enter confirm | Esc quit ",
        Screen::ResumeModal => " <-/-> switch | Enter confirm | Esc resumes the existing map ",
        Screen::Dashboard => " Enter continue | Esc quit ",
        Screen::ProbeQuestion => " up/down or j/k move | 1-9 pick | Esc quit ",
        Screen::ProbeDone => " Enter/Esc exit ",
        Screen::PlanView => " up/down scroll | Enter start teaching | Esc quit ",
        Screen::Lesson => " up/down scroll | Enter take the quiz | Esc back to the plan ",
        Screen::QuizPick => " up/down or j/k move | 1-9 pick | Esc back to the plan ",
        Screen::Reveal => " Enter continue | Esc back to the plan ",
        Screen::QualityMenu => " 1-5 rate | up/down move | Enter confirm | Esc back ",
        Screen::WrongMenu => " 1-4 choose | up/down move | Enter confirm | Esc back ",
        Screen::Done => " Enter/Esc exit ",
        Screen::ReviewEmpty => " nothing due right now | Enter/Esc exit ",
        Screen::ReviewQuestion => " up/down or j/k move | 1-9 pick | Esc save & exit ",
        Screen::ReviewReveal => " Enter rate recall | Esc save & exit ",
        Screen::ReviewQuality => " 1-5 rate | up/down move | Enter confirm | Esc save & exit ",
        Screen::ReviewSummary => " all due cards reviewed | Enter/Esc exit ",
    }
}

fn option_lines(options: &[String], selected: usize) -> Vec<Line<'static>> {
    options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let chosen = index == selected;
            let marker = if chosen { "> " } else { "  " };
            let span = if chosen {
                Span::styled(
                    format!("{marker}[{}] {}", index + 1, strip_inline_markdown(option)),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(format!(
                    "{marker}[{}] {}",
                    index + 1,
                    strip_inline_markdown(option)
                ))
            };
            Line::from(span)
        })
        .collect()
}

fn numbered_menu(items: &[&str], selected: usize) -> Vec<Line<'static>> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let chosen = index == selected;
            let marker = if chosen { "> " } else { "  " };
            let span = if chosen {
                Span::styled(
                    format!("{marker}[{}] {item}", index + 1),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(format!("{marker}[{}] {item}", index + 1))
            };
            Line::from(span)
        })
        .collect()
}

fn markdown_body(md: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for raw in md.lines() {
        let trimmed_start = raw.trim_start();
        if trimmed_start.starts_with('#') {
            let text = strip_inline_markdown(trimmed_start.trim_start_matches('#').trim());
            if !text.is_empty() {
                out.push(Line::from(""));
                out.push(Line::from(bold(text)));
                continue;
            }
        }
        let stripped = strip_inline_markdown(raw.trim_end());
        if stripped.trim().is_empty() {
            out.push(Line::from(""));
            continue;
        }
        for piece in wrap_text(&stripped, width.max(10)) {
            out.push(Line::from(piece));
        }
    }
    out
}

fn wrapped_paragraph(lines: Vec<Line<'static>>, area: Rect, scroll: u16) -> Paragraph<'static> {
    let max_scroll = lines.len().saturating_sub(area.height as usize) as u16;
    Paragraph::new(Text::from(lines)).scroll((scroll.min(max_scroll), 0))
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn body_width(area: Rect) -> usize {
    area.width.saturating_sub(4) as usize
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(if app.thinking.is_some() { 2 } else { 1 }),
        ])
        .split(area);

    let title_spans = if app.goal.is_empty() {
        vec![
            Span::styled(
                " danie ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {} ", app.mode.label())),
        ]
    } else {
        let goal: String = app.goal.chars().take(40).collect();
        vec![
            Span::styled(
                " danie ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {} ", app.mode.label())),
            Span::raw(" - "),
            dim(goal),
        ]
    };
    f.render_widget(Paragraph::new(Line::from(title_spans)), rows[0]);

    render_main(f, app, rows[1]);

    if app.thinking.is_some() {
        let bottom = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(rows[2]);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                THINKING_TEXT,
                Style::default().fg(Color::Black).bg(Color::Yellow),
            ))),
            bottom[0],
        );
        f.render_widget(Paragraph::new(dim(help_text(app.screen))), bottom[1]);
    } else {
        f.render_widget(Paragraph::new(dim(help_text(app.screen))), rows[2]);
    }

    if app.confirm_quit {
        render_confirm_quit(f, app, area);
    }
    if let Some(message) = &app.error {
        render_error(f, area, message);
    }
}

fn render_main(f: &mut Frame, app: &App, area: Rect) {
    match app.screen {
        Screen::TopicInput => render_topic_input(f, app, area),
        Screen::ResumeModal => render_resume_modal(f, app, area),
        Screen::Dashboard => render_dashboard(f, app, area),
        Screen::ProbeQuestion => render_probe_question(f, app, area),
        Screen::ProbeDone => render_probe_done(f, app, area),
        Screen::PlanView => render_plan_view(f, app, area),
        Screen::Lesson => render_lesson(f, app, area),
        Screen::QuizPick => render_teach_quiz(f, app, area),
        Screen::Reveal => render_teach_reveal(f, app, area),
        Screen::QualityMenu => render_quality_menu(f, app, area, false),
        Screen::WrongMenu => render_wrong_menu(f, app, area),
        Screen::Done => render_done(f, app, area),
        Screen::ReviewEmpty => render_review_empty(f, area),
        Screen::ReviewQuestion => render_review_question(f, app, area),
        Screen::ReviewReveal => render_review_reveal(f, app, area),
        Screen::ReviewQuality => render_quality_menu(f, app, area, true),
        Screen::ReviewSummary => render_review_summary(f, app, area),
    }
}

fn render_topic_input(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" New session ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(bold("What do you want to learn today?")),
        Line::from(""),
        Line::from(accent(format!("  > {}", app.input))),
        Line::from(""),
        Line::from(dim(
            "danie will probe what you already know, plan a prerequisite route,",
        )),
        Line::from(dim("then teach one node at a time with lock-in quizzes.")),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_resume_modal(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Existing map ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        Line::from(bold(format!(
            "A stored map for \"{}\" already exists.",
            app.goal
        ))),
        Line::from(""),
    ];
    for (index, label) in ["Resume existing map", "Fresh restart"].iter().enumerate() {
        let chosen = index == app.resume_choice;
        let marker = if chosen { "> " } else { "  " };
        let span = if chosen {
            Span::styled(
                format!("{marker}[{}] {label}", index + 1),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw(format!("{marker}[{}] {label}", index + 1))
        };
        lines.push(Line::from(span));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(dim(
        "Resuming keeps your mastery statuses; a fresh restart clears them.",
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(format!(" Goal: {} ", app.goal));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    if app.profile_default {
        lines.push(Line::from(dim(
            "Using a default learner profile. Edit profile.md inside the store directory",
        )));
        lines.push(Line::from(dim(
            "to record your background, goals and preferred pace.",
        )));
    } else {
        lines.push(Line::from(vec![
            Span::raw("Learner language: "),
            bold(app.profile.language.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::raw("Solid ground: "),
            Span::raw(if app.profile.solid_ground.is_empty() {
                "(none recorded)".to_string()
            } else {
                app.profile.solid_ground.join(", ")
            }),
        ]));
    }
    lines.push(Line::from(""));

    if let Some(map) = &app.map {
        lines.push(Line::from(bold("Current mastery")));
        if map.strands.is_empty() {
            lines.push(Line::from(dim("  nothing probed yet")));
        } else {
            for status in [
                danie_core::StrandStatus::Known,
                danie_core::StrandStatus::Edge,
                danie_core::StrandStatus::Unknown,
                danie_core::StrandStatus::Blocked,
            ] {
                let strands = map.strands_with(status);
                if strands.is_empty() {
                    continue;
                }
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:8}", status.to_string()),
                        Style::default().fg(status_color(status)),
                    ),
                    Span::raw(strands.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join(", ")),
                ]));
            }
        }
    }
    lines.push(Line::from(""));
    match app.mode {
        super::Mode::Teach => lines.push(Line::from(accent(
            "Press Enter to continue to the next step of the Alvar loop.",
        ))),
        _ => lines.push(Line::from(accent(
            "Press Enter to run the diagnostic probe.",
        ))),
    }
    f.render_widget(wrapped_paragraph(lines, inner, 0), inner);
}

fn render_probe_question(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Diagnostic probe ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = body_width(area);
    let mut lines = Vec::new();
    if let Some(question) = app.probe_questions.get(app.probe_index) {
        lines.push(Line::from(vec![
            bold(format!(
                "Question {} of {}",
                app.probe_index + 1,
                app.probe_questions.len()
            )),
            dim(format!("   strand: {}", question.strand)),
        ]));
        lines.push(Line::from(""));
        for piece in wrap_text(&strip_inline_markdown(&question.question), width) {
            lines.push(Line::from(piece));
        }
        lines.push(Line::from(""));
        lines.extend(option_lines(&question.options, app.selected));
        lines.push(Line::from(""));
        lines.push(Line::from(dim(
            "Answer honestly - \"I don't know\" is always allowed.",
        )));
    }
    f.render_widget(wrapped_paragraph(lines, inner, 0), inner);
}

fn render_probe_done(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(format!(" Probe complete: {} ", app.goal));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![Line::from(bold("Where you stand")), Line::from("")];
    if let Some(map) = &app.map {
        for strand in &map.strands {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:8}  ", strand.status.to_string()),
                    Style::default().fg(status_color(strand.status)),
                ),
                Span::raw(strand.name.clone()),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(bold("Saved to")));
    for path in &app.saved_paths {
        lines.push(Line::from(vec![Span::raw("  "), dim(path.clone())]));
    }
    f.render_widget(wrapped_paragraph(lines, inner, app.scroll), inner);
}

fn render_plan_view(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(format!(" Learning plan: {} ", app.goal));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = Vec::new();
    if let Some(current) = &app.current_node {
        if let Some(node) = app.nodes.get(current) {
            lines.push(Line::from(vec![Span::raw("Up next: "), accent(node.title.clone())]));
            lines.push(Line::from(""));
        }
    }
    for row in &app.plan_rows {
        let indent = "  ".repeat(row.depth.min(8));
        let mut spans = vec![Span::raw(indent.clone())];
        if row.depth == 0 {
            spans.push(bold(format!("- {}", row.title)));
        } else {
            spans.push(Span::raw(format!("- {}", row.title)));
        }
        lines.push(Line::from(spans));
        if !row.arrow.is_empty() {
            lines.push(Line::from(vec![
                Span::raw(format!("{indent}      ")),
                dim(row.arrow.clone()),
            ]));
        }
    }
    if app.plan_rows.is_empty() {
        lines.push(Line::from(dim("The plan is empty.")));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(accent(
        "Press Enter to teach the next unlocked node.",
    )));
    f.render_widget(wrapped_paragraph(lines, inner, app.scroll), inner);
}

fn current_quiz(app: &App) -> Option<&crate::protocol::QuizDto> {
    if app.mode == super::Mode::Review {
        return app.review_quiz.as_ref();
    }
    app.lesson.as_ref().map(|lesson| &lesson.quiz)
}

fn render_lesson(f: &mut Frame, app: &App, area: Rect) {
    let title = app
        .lesson
        .as_ref()
        .map(|lesson| lesson.title.clone())
        .unwrap_or_default();
    let block = panel(format!(" Lesson: {title} "));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = body_width(area);
    let mut lines = Vec::new();
    if let Some(lesson) = &app.lesson {
        lines.push(Line::from(bold(lesson.title.clone())));
        lines.push(Line::from(""));
        lines.extend(markdown_body(&lesson.body_md, width));
        lines.push(Line::from(""));
        lines.push(Line::from(accent(
            "Press Enter to lock this in with a quick quiz.",
        )));
    }
    f.render_widget(wrapped_paragraph(lines, inner, app.scroll), inner);
}

fn render_teach_quiz(f: &mut Frame, app: &App, area: Rect) {
    let node_title = app
        .current_node
        .as_deref()
        .and_then(|id| app.nodes.get(id))
        .map(|node| node.title.clone())
        .unwrap_or_default();
    let block = panel(format!(" Lock-in quiz: {node_title} "));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = body_width(area);
    let mut lines = Vec::new();
    if let Some(quiz) = current_quiz(app) {
        for piece in wrap_text(&strip_inline_markdown(&quiz.prompt), width) {
            lines.push(Line::from(piece));
        }
        lines.push(Line::from(""));
        lines.extend(option_lines(&quiz.options, app.selected));
    }
    f.render_widget(wrapped_paragraph(lines, inner, 0), inner);
}

fn render_reveal_content(quiz: &crate::protocol::QuizDto, reveal: &super::RevealInfo, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if reveal.correct {
        lines.push(Line::from(Span::styled(
            "Correct!",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Not quite.",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));
    let chosen_text = quiz
        .options
        .get(reveal.chosen)
        .cloned()
        .unwrap_or_default();
    lines.push(Line::from(vec![
        Span::raw("Your answer: "),
        Span::styled(chosen_text, Style::default().fg(if reveal.correct { Color::Green } else { Color::Red })),
    ]));
    let correct_text = quiz
        .options
        .get(quiz.correct_index)
        .cloned()
        .unwrap_or_default();
    lines.push(Line::from(vec![
        Span::raw("Correct answer: "),
        Span::styled(correct_text, Style::default().fg(Color::Green)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(bold("Why")));
    for piece in wrap_text(&strip_inline_markdown(&quiz.explanation), width) {
        lines.push(Line::from(piece));
    }
    lines
}

fn render_teach_reveal(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Quiz result ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = body_width(area);
    let mut lines = Vec::new();
    if let (Some(quiz), Some(reveal)) = (current_quiz(app), app.reveal.as_ref()) {
        lines.extend(render_reveal_content(quiz, reveal, width));
    }
    f.render_widget(wrapped_paragraph(lines, inner, 0), inner);
}

fn render_quality_menu(f: &mut Frame, app: &App, area: Rect, review: bool) {
    let title = if review {
        " Rate your recall "
    } else {
        " Schedule the card "
    };
    let block = panel(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        Line::from(bold("How well did you know it?")),
        Line::from(""),
    ];
    lines.extend(numbered_menu(&QUALITY_LABELS, app.selected));
    lines.push(Line::from(""));
    lines.push(Line::from(dim(
        "Again and Hard reschedule sooner; Easy and Perfect push the card further out.",
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_wrong_menu(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Adjust the loop ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        Line::from(bold("The answer did not stick. What now?")),
        Line::from(""),
    ];
    lines.extend(numbered_menu(&WRONG_MENU_OPTIONS, app.selected));
    lines.push(Line::from(""));
    lines.push(Line::from(dim(
        "Inserting a prerequisite teaches a missing foundation first.",
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_done(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Session saved ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        Line::from(bold(format!("Topic: {}", app.goal))),
        Line::from(""),
        Line::from(bold("Locked in")),
    ];
    if app.session_locked.is_empty() {
        lines.push(Line::from(dim("  (nothing locked this session)")));
    }
    for item in &app.session_locked {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(item.clone(), Style::default().fg(Color::Green)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(bold("On the edge")));
    if app.session_edge.is_empty() {
        lines.push(Line::from(dim("  (nothing left shaky)")));
    }
    for item in &app.session_edge {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(item.clone(), Style::default().fg(Color::Yellow)),
        ]));
    }
    lines.push(Line::from(""));
    let next = app
        .graph
        .as_ref()
        .and_then(|graph| graph.next_unlocked(&app.known))
        .map(|node| node.title.clone());
    lines.push(Line::from(vec![
        bold("Next node: "),
        match next {
            Some(title) => Span::raw(title),
            None => dim("(plan complete)"),
        },
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(bold("Files written")));
    for path in &app.saved_paths {
        lines.push(Line::from(vec![Span::raw("  "), dim(path.clone())]));
    }
    f.render_widget(wrapped_paragraph(lines, inner, app.scroll), inner);
}

fn render_review_empty(f: &mut Frame, area: Rect) {
    let block = panel(" Review queue ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(bold("Nothing is due right now.")),
        Line::from(""),
        Line::from(dim(
            "Cards become due over time as intervals grow. Finish more lessons",
        )),
        Line::from(dim("with `danie teach`, then come back for another round.")),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_review_question(f: &mut Frame, app: &App, area: Rect) {
    let node = app
        .review_due
        .get(app.review_index)
        .map(|card| card.node.clone())
        .unwrap_or_default();
    let block = panel(format!(
        " Review {}/{}: {} ",
        (app.review_index + 1).min(app.review_due.len().max(1)),
        app.review_due.len(),
        node
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = body_width(area);
    let mut lines = Vec::new();
    if let Some(quiz) = &app.review_quiz {
        for piece in wrap_text(&strip_inline_markdown(&quiz.prompt), width) {
            lines.push(Line::from(piece));
        }
        lines.push(Line::from(""));
        lines.extend(option_lines(&quiz.options, app.selected));
    }
    f.render_widget(wrapped_paragraph(lines, inner, 0), inner);
}

fn render_review_reveal(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Review result ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = body_width(area);
    let mut lines = Vec::new();
    if let (Some(quiz), Some(reveal)) = (app.review_quiz.as_ref(), app.reveal.as_ref()) {
        lines.extend(render_reveal_content(quiz, reveal, width));
    }
    f.render_widget(wrapped_paragraph(lines, inner, 0), inner);
}

fn render_review_summary(f: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Review complete ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(bold("Round finished.")),
        Line::from(""),
        Line::from(vec![
            Span::raw("Recalled well: "),
            Span::styled(
                app.review_good.to_string(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::raw("Needs work soon: "),
            Span::styled(
                app.review_again.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::raw("Schedule updated at "), dim("srs.json")]),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_quit(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(60, 30, area);
    f.render_widget(Clear, popup);
    let block = panel(" Quit danie? ").border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let mut lines = vec![
        Line::from("Your progress is saved after every step."),
        Line::from(""),
    ];
    for (index, label) in ["Save progress and quit", "Keep going"].iter().enumerate() {
        let chosen = index == app.confirm_choice;
        let marker = if chosen { "> " } else { "  " };
        let span = if chosen {
            Span::styled(
                format!("{marker}[{}] {label}", index + 1),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw(format!("{marker}[{}] {label}", index + 1))
        };
        lines.push(Line::from(span));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(dim("y quit | n stay | arrows + Enter choose")));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_error(f: &mut Frame, area: Rect, message: &str) {
    let popup = centered_rect(70, 45, area);
    f.render_widget(Clear, popup);
    let block = panel(" Something went wrong ").border_style(Style::default().fg(Color::Red));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let width = inner.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line<'static>> =
        wrap_text(message, width.max(10)).into_iter().map(Line::from).collect();
    lines.push(Line::from(""));
    lines.push(Line::from(dim("press any key to continue")));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
