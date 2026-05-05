use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::{FirstRunWizard, WizardStep};

impl FirstRunWizard {
    fn render_copilot_device_flow(&mut self, frame: &mut Frame, area: Rect) {
        // Poll status on every render tick
        let complete = self.poll_copilot_status();

        if complete && self.step == WizardStep::CopilotDeviceFlow {
            self.step = WizardStep::SelectModel;
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 2/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "GitHub Copilot Login",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Content
        let mut content_lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "GitHub Copilot Device Flow",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
        ];

        if !self.copilot_verification_uri.is_empty() {
            content_lines.push(Line::from(vec![
                Span::styled("1. Open: ", Style::default().fg(Color::White)),
                Span::styled(
                    &self.copilot_verification_uri,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
            content_lines.push(Line::from(""));
            content_lines.push(Line::from(vec![
                Span::styled("2. Enter code: ", Style::default().fg(Color::White)),
                Span::styled(
                    &self.copilot_user_code,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            content_lines.push(Line::from(""));
        }

        content_lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::White)),
            Span::styled(&self.copilot_status, Style::default().fg(Color::Cyan)),
        ]));

        let content_widget = Paragraph::new(content_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Authorization"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(content_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Esc: "),
            Span::styled("Cancel", Style::default().fg(Color::Red)),
            Span::from(" | r: "),
            Span::styled("Retry", Style::default().fg(Color::Cyan)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        match self.step {
            WizardStep::Welcome => self.render_welcome(frame, area),
            WizardStep::SelectProvider => self.render_provider_selection(frame, area),
            WizardStep::CopilotDeviceFlow => self.render_copilot_device_flow(frame, area),
            WizardStep::ConfigureProvider => self.render_provider_config(frame, area),
            WizardStep::SelectModel => self.render_model_selection(frame, area),
            WizardStep::Review => self.render_review(frame, area),
            WizardStep::Complete => self.render_complete(frame, area),
        }

        // Render error message if present
        if let Some(ref error) = self.error_message {
            self.render_error_message(frame, area, error);
        }

        // Render help overlay if enabled
        if self.show_help {
            self.render_help(frame, area);
        }
    }

    /// Render welcome screen
    fn render_welcome(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(15),
                Constraint::Length(3),
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![Line::from(vec![
            Span::styled("🦀", Style::default().fg(Color::Yellow)),
            Span::styled(" Welcome to ", Style::default().fg(Color::White)),
            Span::styled(
                "RustyCode",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        // Welcome message
        let welcome_text = vec![
            Line::from(""),
            Line::from("This wizard will help you configure RustyCode for the first time."),
            Line::from(""),
            Line::from("You'll need to:"),
            Line::from("  • Choose your AI provider (Anthropic, OpenAI, etc.)"),
            Line::from("  • Enter your API key"),
            Line::from("  • Select your preferred model"),
            Line::from(""),
            Line::from("Your API key is stored locally and never sent anywhere else."),
            Line::from(""),
            Line::from(vec![
                Span::from("Press "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" to begin, or "),
                Span::styled(
                    "?",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" for help"),
            ])
            .alignment(Alignment::Center),
        ];

        let welcome = Paragraph::new(welcome_text)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(welcome, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::White)),
            Span::styled("q", Style::default().fg(Color::Red)),
            Span::styled(" to quit", Style::default().fg(Color::White)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render provider selection screen
    fn render_provider_selection(&self, frame: &mut Frame, area: Rect) {
        // Check if we have enough height for the full layout (minimum 20 rows)
        let has_enough_height = area.height >= 20;

        let chunks = if has_enough_height {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(5), // Details widget
                    Constraint::Length(3), // Instructions
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3), // Instructions only
                ])
                .split(area)
        };

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 1/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Select AI Provider",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Provider list
        let provider_lines: Vec<Line> = self
            .providers
            .iter()
            .enumerate()
            .map(|(i, provider)| {
                let is_selected = i == self.selected_provider_index;
                let prefix = if is_selected { "►" } else { " " };
                let indicator = if provider.popular { " ⭐" } else { "" };

                Line::from(vec![Span::styled(
                    format!("{} {}{} ", prefix, provider.name, indicator),
                    Style::default()
                        .fg(if is_selected {
                            Color::Cyan
                        } else {
                            Color::White
                        })
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )])
            })
            .collect();

        let providers_widget = Paragraph::new(provider_lines)
            .block(Block::default().borders(Borders::ALL).title("Providers"))
            .wrap(Wrap { trim: true });
        frame.render_widget(providers_widget, chunks[1]);

        // Provider details (only if enough height)
        if has_enough_height {
            let provider = self.selected_provider();
            let details = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Description: ", Style::default().fg(Color::Cyan)),
                    Span::from(&provider.description),
                ]),
                Line::from(vec![
                    Span::styled("API Key: ", Style::default().fg(Color::Cyan)),
                    Span::from(if provider.requires_api_key {
                        "Required"
                    } else {
                        "Not required"
                    }),
                ]),
            ];

            let details_widget = Paragraph::new(details)
                .block(Block::default().borders(Borders::ALL).title("Details"));
            frame.render_widget(details_widget, chunks[2]);
        }

        // Instructions
        let instructions = Paragraph::new(vec![Line::from(vec![
            Span::from("↑/↓: "),
            Span::styled("Navigate", Style::default().fg(Color::Cyan)),
            Span::from(" | Enter: "),
            Span::styled("Select", Style::default().fg(Color::Green)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));

        // Render instructions at the bottom (last chunk)
        let instructions_chunk = if has_enough_height {
            chunks[3]
        } else {
            chunks[2]
        };
        frame.render_widget(instructions, instructions_chunk);
    }

    /// Render provider configuration screen
    fn render_provider_config(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(10),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 2/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Configure Provider",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // API key input
        let provider = self.selected_provider();
        let instructions = if provider.requires_api_key {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Provider: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        &provider.name,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from("Enter your API key:"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Key: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        if self.api_key_input.is_empty() {
                            "(not entered)".to_string()
                        } else {
                            // Show only last 8 characters
                            if self.api_key_input.chars().count() > 8 {
                                let chars: Vec<char> = self.api_key_input.chars().collect();
                                format!(
                                    "...{}",
                                    chars[chars.len() - 8..].iter().collect::<String>()
                                )
                            } else {
                                self.api_key_input.clone()
                            }
                        },
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled("_", Style::default().fg(Color::White)), // Cursor
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Get your API key from: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        self.get_api_key_url(&provider.id),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Provider: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        &provider.name,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("✓", Style::default().fg(Color::Green)),
                    Span::styled(
                        " No API key required for this provider",
                        Style::default().fg(Color::White),
                    ),
                ]),
                Line::from(""),
                Line::from("Press Enter to continue..."),
            ]
        };

        let input_widget = Paragraph::new(instructions)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Configuration"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(input_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Type: "),
            Span::styled("API key", Style::default().fg(Color::Cyan)),
            Span::from(" | Enter: "),
            Span::styled("Continue", Style::default().fg(Color::Green)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render model selection screen
    fn render_model_selection(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 3/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Select Model",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Model list
        let models = self.available_models();
        let model_lines: Vec<Line> = models
            .iter()
            .enumerate()
            .map(|(i, model)| {
                let is_selected = i == self.selected_model_index;
                let prefix = if is_selected { "►" } else { " " };

                Line::from(vec![Span::styled(
                    format!("{} {}", prefix, model),
                    Style::default()
                        .fg(if is_selected {
                            Color::Cyan
                        } else {
                            Color::White
                        })
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )])
            })
            .collect();

        let models_widget = Paragraph::new(model_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Available Models"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(models_widget, chunks[1]);

        // Instructions
        let instructions = Paragraph::new(vec![Line::from(vec![
            Span::from("↑/↓: "),
            Span::styled("Navigate", Style::default().fg(Color::Cyan)),
            Span::from(" | Enter: "),
            Span::styled("Select", Style::default().fg(Color::Green)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));

        let bottom_area = Rect {
            y: area.height.saturating_sub(3),
            height: 3,
            ..area
        };
        frame.render_widget(instructions, bottom_area);
    }

    /// Render review screen
    fn render_review(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 4/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Review & Save",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Review content
        let provider = self.selected_provider();
        let model = self.selected_model();

        let review_lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "Configuration Summary:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Provider: ", Style::default().fg(Color::White)),
                Span::styled(&provider.name, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Model: ", Style::default().fg(Color::White)),
                Span::styled(&model, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("API Key: ", Style::default().fg(Color::White)),
                Span::styled(
                    if provider.requires_api_key && !self.api_key_input.is_empty() {
                        "••••••••••••••••"
                    } else if provider.requires_api_key {
                        "(not configured)"
                    } else {
                        "N/A"
                    },
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Config file: ", Style::default().fg(Color::White)),
                Span::styled(
                    self.config_path.display().to_string(),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::White)),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " to save and start using RustyCode",
                    Style::default().fg(Color::White),
                ),
            ])
            .alignment(Alignment::Center),
        ];

        let review_widget = Paragraph::new(review_lines)
            .block(Block::default().borders(Borders::ALL).title("Review"))
            .wrap(Wrap { trim: true });
        frame.render_widget(review_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Enter: "),
            Span::styled("Save & Start", Style::default().fg(Color::Green)),
            Span::from(" | r: "),
            Span::styled("Reconfigure", Style::default().fg(Color::Cyan)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render completion screen
    fn render_complete(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled(
                "✓",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " Configuration Complete!",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Success message
        let success_lines = vec![
            Line::from(""),
            Line::from("Your RustyCode configuration has been saved successfully!"),
            Line::from(""),
            Line::from("You're all set to start using RustyCode."),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::from("Press "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" or "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" to start coding"),
            ])
            .alignment(Alignment::Center),
        ];

        let success_widget = Paragraph::new(success_lines)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(success_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Press "),
            Span::styled("Enter/Esc", Style::default().fg(Color::Green)),
            Span::from(" to exit wizard"),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render error message overlay
    fn render_error_message(&self, frame: &mut Frame, area: Rect, error: &str) {
        let error_paragraph = Paragraph::new(vec![Line::from(vec![
            Span::styled("✖", Style::default().fg(Color::Red)),
            Span::styled(format!(" {}", error), Style::default().fg(Color::White)),
        ])
        .alignment(Alignment::Center)])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Red)),
        );

        let error_area = Rect {
            width: area.width.min(60),
            height: 3,
            x: (area.width.saturating_sub(60)) / 2,
            y: area.height.saturating_sub(5),
        };

        frame.render_widget(error_paragraph, error_area);
    }

    /// Render help overlay
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help_lines = vec![
            Line::from(vec![Span::styled(
                "Keyboard Shortcuts",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )])
            .alignment(Alignment::Center),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑/k or ↓/j - Move up/down"),
            Line::from(""),
            Line::from("Actions:"),
            Line::from("  Enter     - Confirm/Continue"),
            Line::from("  Esc       - Go back"),
            Line::from("  q         - Quit wizard"),
            Line::from("  ?         - Toggle this help"),
            Line::from(""),
            Line::from(vec![
                Span::from("Press "),
                Span::styled("?", Style::default().fg(Color::Cyan)),
                Span::from(" to close help"),
            ])
            .alignment(Alignment::Center),
        ];

        let help_paragraph = Paragraph::new(help_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .style(Style::default().bg(Color::Black)),
        );

        let help_area = Rect {
            width: area.width.min(50),
            x: (area.width.saturating_sub(50)) / 2,
            y: (area.height.saturating_sub(15)) / 2,
            height: 15,
        };

        frame.render_widget(help_paragraph, help_area);
    }

    pub(crate) fn get_api_key_url(&self, provider_id: &str) -> String {
        match provider_id {
            "anthropic" => "https://console.anthropic.com/settings/keys".to_string(),
            "openai" => "https://platform.openai.com/api-keys".to_string(),
            "openrouter" => "https://openrouter.ai/keys".to_string(),
            "copilot" => "https://github.com/settings/copilot".to_string(),
            _ => "https://example.com/get-api-key".to_string(),
        }
    }
}
