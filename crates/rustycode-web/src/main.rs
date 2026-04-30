use std::{cell::RefCell, io, rc::Rc};

use gloo_net::http::Request;
use js_sys::Date;
use ratzilla::ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::Color,
    style::Stylize,
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Terminal,
};
use rustycode_protocol::{ToolCall, ToolResult};
use rustycode_ui_core::{
    FrontendMessageKind, FrontendSession, MarkdownRenderer, MessageTheme, RunController,
    SessionRunController, SubmittedInput,
};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use ratzilla::{event::KeyCode, DomBackend, WebRenderer};

mod slash_commands;
mod skills;
use slash_commands::execute_command;

struct WebAppState {
    session: FrontendSession,
    selected_panel: usize,
    #[allow(dead_code)]
    selected_right_panel: usize,
    last_key: String,
    quit_requested: bool,
    theme: MessageTheme,
    right_panel_content: String,
    skill_manager: skills::WebSkillManager,
}

impl Default for WebAppState {
    fn default() -> Self {
        let mut session = FrontendSession::default();
        session.add_message(
            "╶─ RustyCode Web ─╴\n\nautonomous development framework\n\ntype a message and press Enter to start",
            FrontendMessageKind::System,
        );

        Self {
            session,
            selected_panel: 0,
            selected_right_panel: 0,
            last_key: String::new(),
            quit_requested: false,
            theme: MessageTheme::default(),
            right_panel_content: "Welcome to RustyCode Web\n\nUse arrow keys to switch panels\nPress ? for commands".to_string(),
            skill_manager: skills::WebSkillManager::new(),
        }
    }
}

impl WebAppState {
    fn next_panel(&mut self) {
        self.selected_panel = (self.selected_panel + 1) % 2;
    }

    fn previous_panel(&mut self) {
        self.selected_panel = (self.selected_panel + 2 - 1) % 2;
    }

    #[allow(dead_code)]
    fn current_panel_name(&self) -> &'static str {
        match self.selected_panel {
            0 => "Conversation",
            _ => "Task / Info",
        }
    }
}

const TOOL_SERVER_ENDPOINT: &str = "http://127.0.0.1:3000/call";

fn describe_key(code: &KeyCode) -> String {
    match code {
        KeyCode::Char(ch) => format!("char({ch})"),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "page_up".to_string(),
        KeyCode::PageDown => "page_down".to_string(),
        KeyCode::F(num) => format!("f{num}"),
        KeyCode::Unidentified => "unidentified".to_string(),
    }
}

fn call_tool_server(state: Rc<RefCell<WebAppState>>, command: String) {
    let call = ToolCall {
        call_id: format!("web-{:.0}", Date::now()),
        name: "bash".to_string(),
        arguments: json!({ "command": command }),
    };

    spawn_local(async move {
        let response = Request::post(TOOL_SERVER_ENDPOINT)
            .header("Content-Type", "application/json")
            .json(&call)
            .expect("failed to serialize tool call")
            .send()
            .await;

        match response {
            Ok(resp) => match resp.json::<ToolResult>().await {
                Ok(result) => {
                    let mut state = state.borrow_mut();
                    let mut controller = SessionRunController::new(&mut state.session);
                    if result.success {
                        controller.finish_success(result.output);
                    } else {
                        controller.finish_error(
                            result
                                .error
                                .unwrap_or_else(|| "Tool call failed".to_string()),
                        );
                    }
                }
                Err(err) => {
                    let mut state = state.borrow_mut();
                    state
                        .session
                        .finish_error(format!("response parse failed: {}", err));
                }
            },
            Err(err) => {
                let mut state = state.borrow_mut();
                SessionRunController::new(&mut state.session)
                    .finish_error(format!("request failed: {}", err));
            }
        }
    });
}

fn main() -> io::Result<()> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let backend = DomBackend::new()?;
    let terminal = Terminal::new(backend)?;
    let state = Rc::new(RefCell::new(WebAppState::default()));

    terminal.on_key_event({
        let state = state.clone();
        move |key_event| {
            let mut app = state.borrow_mut();
            app.last_key = describe_key(&key_event.code);

            match key_event.code {
                KeyCode::Left => app.previous_panel(),
                KeyCode::Right | KeyCode::Tab => app.next_panel(),
                KeyCode::Backspace => {
                    if app.selected_panel == 0 {
                        app.session.input.pop();
                    }
                }
                KeyCode::Enter => {
                    if app.selected_panel == 0 {
                        match app.session.submit_input() {
                            SubmittedInput::ChatMessage(message) => {
                                SessionRunController::new(&mut app.session).start_request();
                                drop(app);
                                call_tool_server(state.clone(), message);
                            }
                            SubmittedInput::SlashCommand(command) => {
                                let result = execute_command(&command, &mut app.skill_manager);
                                if result.success {
                                    app.session.add_message(&result.message, FrontendMessageKind::System);
                                } else {
                                    app.session.add_message(&result.message, FrontendMessageKind::Error);
                                }
                                if let Some(panel) = result.panel_update {
                                    app.right_panel_content = panel.content;
                                }
                            }
                            SubmittedInput::BangCommand(command) => {
                                app.session.add_message(
                                    format!("Bang commands are parsed by shared code: {command}"),
                                    FrontendMessageKind::System,
                                );
                            }
                            SubmittedInput::Empty => {}
                            _ => {
                                app.session.add_message(
                                    "Unhandled input type",
                                    FrontendMessageKind::System,
                                );
                            }
                        }
                    }
                }
                KeyCode::Char('q') => {
                    app.quit_requested = true;
                    log::info!("Quit requested from the web UI");
                }
                KeyCode::Char(ch) => {
                    if app.selected_panel == 0 {
                        app.session.input.push(ch);
                    }
                }
                _ => {}
            }
        }
    });

    terminal.draw_web(move |f| {
        let state = state.borrow();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.area());

        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(chunks[1]);

        let header = Paragraph::new("╺─ RustyCode Web v0.1 ────── ins")
            .alignment(Alignment::Left)
            .block(
                Block::bordered()
                    .border_style(Color::Rgb(94, 129, 172)), // frost blue
            )
            .fg(Color::Rgb(236, 239, 244)); // snow

        // 60% Left side - Conversation
        let mut conversation_lines = Vec::new();
        for message in &state.session.messages {
            let (prefix, color) = match message.kind {
                FrontendMessageKind::User => ("▐ you ", state.theme.user_color),
                FrontendMessageKind::Assistant => ("▐ ai ", state.theme.ai_color),
                FrontendMessageKind::System => ("▐ sys ", state.theme.system_color),
                FrontendMessageKind::Tool => ("  ╶─ tool ─╴ ", state.theme.tool_summary_color),
                FrontendMessageKind::Error => ("✗ ", Color::Rgb(191, 97, 106)),
                _ => ("▐ ", state.theme.system_color),
            };

            let prefix_span = Span::styled(prefix, color);
            let mut content_lines = MarkdownRenderer::render_content(&message.content, &state.theme, None);

            if !content_lines.is_empty() {
                let mut spans = vec![prefix_span];
                spans.extend(content_lines[0].spans.clone().into_iter());
                content_lines[0] = Line::from(spans);
                conversation_lines.extend(content_lines);
            }
            conversation_lines.push(Line::from("")); // Break between messages
        }

        conversation_lines.push(Line::from(""));
        conversation_lines.push(Line::from(vec![
            Span::styled("▐▸ ", Color::Rgb(94, 129, 172)),
            Span::raw(&state.session.input),
            Span::styled("█", Color::Rgb(76, 86, 106)),
        ]));

        let conv_border_color = if state.selected_panel == 0 {
            Color::Rgb(94, 129, 172) // frost blue
        } else {
            Color::Rgb(76, 86, 106) // muted
        };
        let conversation_paragraph = Paragraph::new(conversation_lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .title("messages")
                    .title_alignment(Alignment::Left)
                    .border_style(conv_border_color),
            );

        // 40% Right side - Task/Info Panel
        let task_border_color = if state.selected_panel == 1 { Color::Green } else { Color::DarkGray };
        let task_paragraph = Paragraph::new(state.right_panel_content.as_str())
            .wrap(Wrap { trim: true })
            .block(
                Block::bordered()
                    .title("Task View")
                    .title_alignment(Alignment::Left)
                    .border_style(task_border_color),
            )
            .white();

        let footer = Paragraph::new(format!(
            "Last key: {} | Quit requested: {} | Pending request: {}",
            if state.last_key.is_empty() {
                "(none)"
            } else {
                &state.last_key
            },
            if state.quit_requested { "yes" } else { "no" },
            if state.session.pending_request {
                "yes"
            } else {
                "no"
            }
        ))
        .alignment(Alignment::Left)
        .block(
            Block::bordered()
                .title("Status")
                .title_alignment(Alignment::Left)
                .border_style(Color::Yellow),
        )
        .black()
        .on_gray();

        f.render_widget(header, chunks[0]);
        f.render_widget(conversation_paragraph, body_chunks[0]);
        f.render_widget(task_paragraph, body_chunks[1]);
        f.render_widget(footer, chunks[2]);
    });

    Ok(())
}
