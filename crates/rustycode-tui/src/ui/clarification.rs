//! Detection of clarification questions in AI responses and a TUI panel for batch answering.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// A selectable option for a question
#[derive(Debug, Clone)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// A detected clarification question
#[derive(Debug, Clone)]
pub struct Question {
    pub text: String,
    /// Context line preceding the question.
    pub context: Option<String>,
    pub options: Vec<QuestionOption>,
}

impl Question {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            context: None,
            options: vec![],
        }
    }

    pub fn with_options(text: impl Into<String>, options: Vec<QuestionOption>) -> Self {
        Self {
            text: text.into(),
            context: None,
            options,
        }
    }
}

/// Panel for displaying and answering clarification questions
#[derive(Clone)]
pub struct ClarificationPanel {
    pub questions: Vec<Question>,
    pub selected_index: usize,
    pub visible: bool,
    /// User answers keyed by question index.
    pub answers: Vec<String>,
    pub completed: bool,
    /// Per-question highlighted option index (for option-based questions).
    pub selected_option_indices: Vec<usize>,
}

impl ClarificationPanel {
    pub fn new(questions: Vec<Question>) -> Self {
        let answers = vec![String::new(); questions.len()];
        let selected_option_indices = vec![0usize; questions.len()];
        Self {
            questions,
            selected_index: 0,
            visible: true,
            answers,
            completed: false,
            selected_option_indices,
        }
    }

    pub fn hidden() -> Self {
        Self {
            questions: vec![],
            selected_index: 0,
            visible: false,
            answers: vec![],
            completed: false,
            selected_option_indices: vec![],
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn select_next(&mut self) {
        if !self.questions.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.questions.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.questions.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.questions.len() - 1
            } else {
                self.selected_index - 1
            };
        }
    }

    pub fn set_current_answer(&mut self, answer: String) {
        if self.selected_index < self.answers.len() {
            self.answers[self.selected_index] = answer;
        }
    }

    pub fn current_answer(&self) -> &str {
        if self.selected_index < self.answers.len() {
            &self.answers[self.selected_index]
        } else {
            ""
        }
    }

    pub fn all_answered(&self) -> bool {
        self.questions
            .iter()
            .enumerate()
            .all(|(i, _)| !self.answers.get(i).map(|a| a.is_empty()).unwrap_or(true))
    }

    pub fn complete(&mut self) {
        self.completed = true;
    }

    pub fn reset(&mut self) {
        self.questions.clear();
        self.answers.clear();
        self.selected_index = 0;
        self.completed = false;
        self.selected_option_indices.clear();
    }

    pub fn current_has_options(&self) -> bool {
        self.questions
            .get(self.selected_index)
            .map(|q| !q.options.is_empty())
            .unwrap_or(false)
    }

    pub fn current_option_count(&self) -> usize {
        self.questions
            .get(self.selected_index)
            .map(|q| q.options.len())
            .unwrap_or(0)
    }

    pub fn current_option_index(&self) -> usize {
        self.selected_option_indices
            .get(self.selected_index)
            .copied()
            .unwrap_or(0)
    }

    pub fn select_next_option(&mut self) {
        let count = self.current_option_count();
        if let Some(idx) = self.selected_option_indices.get_mut(self.selected_index) {
            if count > 0 {
                *idx = (*idx + 1) % count;
            }
        }
    }

    pub fn select_previous_option(&mut self) {
        let count = self.current_option_count();
        if let Some(idx) = self.selected_option_indices.get_mut(self.selected_index) {
            if count > 0 {
                *idx = if *idx == 0 { count - 1 } else { *idx - 1 };
            }
        }
    }

    /// Select the currently highlighted option as the answer
    pub fn select_current_option(&mut self) {
        if self.current_has_options() {
            let opt_idx = self.current_option_index();
            if let Some(question) = self.questions.get(self.selected_index) {
                if let Some(option) = question.options.get(opt_idx) {
                    self.set_current_answer(option.label.clone());
                }
            }
        }
    }

    pub fn answered_count(&self) -> usize {
        self.answers.iter().filter(|a| !a.is_empty()).count()
    }

    fn build_content(&self) -> Vec<Line<'_>> {
        let mut lines = Vec::new();

        // Header
        lines.push(Line::from(vec![Span::styled(
            "❓ Clarification Questions",
            Style::default()
                .fg(Color::Rgb(255, 165, 0)) // Orange
                .add_modifier(Modifier::BOLD),
        )]));

        lines.push(Line::from(Span::styled(
            "─".repeat(40),
            Style::default().fg(Color::DarkGray),
        )));

        // Progress
        let answered = self.answered_count();
        let total = self.questions.len();
        lines.push(Line::from(vec![Span::styled(
            format!("Answered: {}/{}", answered, total),
            Style::default().fg(Color::White),
        )]));
        lines.push(Line::from(""));

        // Questions
        for (i, question) in self.questions.iter().enumerate() {
            let is_selected = i == self.selected_index;
            let is_answered = !self.answers.get(i).map(|a| a.is_empty()).unwrap_or(true);

            let (icon, color) = if is_answered {
                ("✓", Color::Green)
            } else if is_selected {
                ("▶", Color::Yellow)
            } else {
                ("○", Color::Gray)
            };

            // Question number and icon
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} {}. ", icon, i + 1),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if is_selected {
                        question.text.clone()
                    } else {
                        truncate_text(&question.text, 50)
                    },
                    Style::default().fg(if is_selected {
                        Color::White
                    } else {
                        Color::Gray
                    }),
                ),
            ]));

            // Show selectable options if this question has them and is selected
            if is_selected && !question.options.is_empty() {
                let highlighted = self.selected_option_indices.get(i).copied().unwrap_or(0);
                for (opt_i, opt) in question.options.iter().enumerate() {
                    let is_highlighted = opt_i == highlighted;
                    let (prefix, opt_color) = if is_highlighted {
                        ("  ◉ ", Color::Cyan)
                    } else {
                        ("  ○ ", Color::DarkGray)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(opt_color)),
                        Span::styled(
                            &opt.label,
                            Style::default()
                                .fg(if is_highlighted {
                                    Color::White
                                } else {
                                    Color::Gray
                                })
                                .add_modifier(if is_highlighted {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                }),
                        ),
                        if !opt.description.is_empty() {
                            Span::styled(
                                format!(" - {}", truncate_text(&opt.description, 40)),
                                Style::default().fg(Color::DarkGray),
                            )
                        } else {
                            Span::raw("")
                        },
                    ]));
                }
                // Navigation hint for options
                lines.push(Line::from(vec![Span::styled(
                    "    ←/→: choose option",
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            // Show answer if provided (free text typed by user)
            if let Some(answer) = self.answers.get(i) {
                if !answer.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("   ↳ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(truncate_text(answer, 50), Style::default().fg(Color::Green)),
                    ]));
                }
            }

            // Add spacing between questions
            if i < self.questions.len() - 1 {
                lines.push(Line::from(""));
            }
        }

        // Footer instructions
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "─".repeat(40),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(vec![
            Span::styled("↑/↓: Select  ", Style::default().fg(Color::DarkGray)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Type to answer  ", Style::default().fg(Color::DarkGray)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Enter: Submit all",
                Style::default().fg(if self.all_answered() {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
        ]));

        lines
    }
}

impl Default for ClarificationPanel {
    fn default() -> Self {
        Self::hidden()
    }
}

impl Widget for ClarificationPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.visible || area.width < 20 || area.height < 10 {
            return;
        }

        // Clear the background first to prevent text bleeding through
        Clear.render(area, buf);

        let content = self.build_content();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(255, 165, 0))) // Orange border
            .title(Span::styled(
                " Clarification ",
                Style::default()
                    .fg(Color::Rgb(255, 165, 0))
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .block(block)
            .style(Style::default().fg(Color::Gray));

        paragraph.render(area, buf);
    }
}

/// Detect clarification questions in AI response content
///
/// Returns a list of detected questions with optional context.
pub fn detect_questions(content: &str) -> Vec<Question> {
    let mut questions = Vec::new();

    // Split into sentences (simple approach: split by punctuation)
    let sentences: Vec<&str> = content
        .split(&['.', '?', '!', '\n'][..])
        .filter(|s| !s.trim().is_empty())
        .collect();

    for sentence in &sentences {
        let trimmed = sentence.trim();

        // Check if it's a direct question (ends with ?)
        if trimmed.ends_with('?') {
            // Filter out rhetorical or non-clarifying questions
            if !is_rhetorical_question(trimmed) {
                questions.push(Question {
                    text: trimmed.to_string(),
                    context: None,
                    options: vec![],
                });
            }
        }
        // Check for interrogative patterns without ?
        else if is_interrogative_pattern(trimmed) {
            questions.push(Question {
                text: format!("{}?", trimmed),
                context: None,
                options: vec![],
            });
        }
    }

    // If no questions detected, check for clarification keywords in context
    if questions.is_empty() && contains_clarification_request(content) {
        // Extract the sentence containing the clarification request
        for sentence in &sentences {
            if contains_clarification_keywords(sentence) {
                questions.push(Question {
                    text: sentence.trim().to_string(),
                    context: None,
                    options: vec![],
                });
            }
        }
    }

    questions
}

fn is_rhetorical_question(question: &str) -> bool {
    let lower = question.to_lowercase();

    // Common rhetorical patterns
    let rhetorical_patterns = [
        "would you like",
        "shall i",
        "can i help",
        "is there anything else",
        "do you want me to",
        "should i",
        "let me know if",
    ];

    rhetorical_patterns.iter().any(|p| lower.contains(p))
}

fn is_interrogative_pattern(text: &str) -> bool {
    let lower = text.to_lowercase();

    // Patterns that indicate a question even without ?
    let patterns = [
        "can you ",
        "could you ",
        "do you ",
        "does the ",
        "is the ",
        "are the ",
        "what ",
        "which ",
        "who ",
        "when ",
        "where ",
        "how ",
        "why ",
        "please confirm",
        "please clarify",
        "please specify",
        "please let me know",
    ];

    patterns.iter().any(|p| lower.starts_with(p))
        && !lower.starts_with("do you want me to")
        && !lower.starts_with("would you like")
}

fn contains_clarification_request(content: &str) -> bool {
    let lower = content.to_lowercase();

    let clarification_phrases = [
        "please clarify",
        "please confirm",
        "please specify",
        "let me know",
        "i need to know",
        "can you tell me",
        "could you tell me",
    ];

    clarification_phrases.iter().any(|p| lower.contains(p))
}

fn contains_clarification_keywords(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();

    let keywords = [
        "clarify", "confirm", "specify", "which", "what", "how", "when", "where", "who", "why",
    ];

    keywords.iter().any(|kw| lower.contains(*kw))
}

/// Truncate text to max length with ellipsis
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else if max_len > 3 {
        let trunc_at = text.floor_char_boundary(max_len - 3);
        format!("{}...", &text[..trunc_at])
    } else {
        "...".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_direct_questions() {
        let content = "I need more information. What is the expected input format? What are the output requirements?";
        let questions = detect_questions(content);

        assert_eq!(questions.len(), 2);
        assert!(questions[0]
            .text
            .contains("What is the expected input format"));
        assert!(questions[1]
            .text
            .contains("What are the output requirements"));
    }

    #[test]
    fn test_detect_interrogative_patterns() {
        let content = "Please tell me more about the project. Can you provide the specifications?";
        let questions = detect_questions(content);

        // Should detect the interrogative pattern
        assert!(questions.iter().any(|q| q.text.contains("Can you provide")));
    }

    #[test]
    fn test_rhetorical_questions_filtered() {
        let content = "Would you like me to help with that? I can assist with the implementation.";
        let questions = detect_questions(content);

        // Rhetorical questions should be filtered out
        assert!(!questions.iter().any(|q| q.text.contains("Would you like")));
    }

    #[test]
    fn test_clarification_panel_creation() {
        let questions = vec![
            Question {
                text: "What is the input format?".to_string(),
                context: None,
                options: vec![],
            },
            Question {
                text: "What is the expected output?".to_string(),
                context: None,
                options: vec![],
            },
        ];

        let panel = ClarificationPanel::new(questions.clone());

        assert!(panel.visible);
        assert_eq!(panel.questions.len(), 2);
        assert!(!panel.completed);
        assert_eq!(panel.answered_count(), 0);
    }

    #[test]
    fn test_clarification_panel_answers() {
        let questions = vec![
            Question {
                text: "What is the input format?".to_string(),
                context: None,
                options: vec![],
            },
            Question {
                text: "What is the expected output?".to_string(),
                context: None,
                options: vec![],
            },
        ];

        let mut panel = ClarificationPanel::new(questions);

        assert!(!panel.all_answered());

        panel.set_current_answer("JSON format".to_string());
        assert_eq!(panel.current_answer(), "JSON format");
        assert_eq!(panel.answered_count(), 1);
        assert!(!panel.all_answered());

        panel.select_next();
        panel.set_current_answer("XML output".to_string());
        assert!(panel.all_answered());
    }

    #[test]
    fn test_clarification_panel_navigation() {
        let questions = vec![
            Question {
                text: "Question 1".to_string(),
                context: None,
                options: vec![],
            },
            Question {
                text: "Question 2".to_string(),
                context: None,
                options: vec![],
            },
            Question {
                text: "Question 3".to_string(),
                context: None,
                options: vec![],
            },
        ];

        let mut panel = ClarificationPanel::new(questions);

        assert_eq!(panel.selected_index, 0);

        panel.select_next();
        assert_eq!(panel.selected_index, 1);

        panel.select_next();
        assert_eq!(panel.selected_index, 2);

        panel.select_next();
        assert_eq!(panel.selected_index, 0); // Wraps around

        panel.select_previous();
        assert_eq!(panel.selected_index, 2); // Wraps around backwards
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("Hello", 10), "Hello");
        assert_eq!(truncate_text("Hello World", 10), "Hello W...");
        assert_eq!(truncate_text("Hello", 3), "...");
        assert_eq!(truncate_text("Hello", 4), "H...");
    }

    #[test]
    fn test_detect_questions_empty_content() {
        let questions = detect_questions("");
        assert!(questions.is_empty());
    }

    #[test]
    fn test_detect_questions_no_questions() {
        let content = "This is a statement. No questions here.";
        let questions = detect_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_detect_multiple_questions_various_formats() {
        let content = "What format? How should I proceed?
        I need to know your preference.
        When can you start?";
        let questions = detect_questions(content);

        // Should detect multiple questions
        assert!(questions.len() >= 2);
        assert!(questions.iter().any(|q| q.text.contains("What format")));
        assert!(questions.iter().any(|q| q.text.contains("How should")));
    }

    #[test]
    fn test_do_you_want_me_to_filtered() {
        let content = "Do you want me to implement the feature now?";
        let questions = detect_questions(content);

        // Should filter out "Do you want me to" questions
        assert!(!questions
            .iter()
            .any(|q| q.text.contains("Do you want me to")));
    }

    #[test]
    fn test_contains_clarification_request() {
        assert!(contains_clarification_request(
            "Please clarify the requirements"
        ));
        assert!(contains_clarification_request(
            "Can you tell me more about it"
        ));
        assert!(contains_clarification_request("Please specify the format"));
        assert!(contains_clarification_request("Please confirm your choice"));
        assert!(!contains_clarification_request("I will implement it now"));
    }

    #[test]
    fn test_contains_clarification_keywords() {
        assert!(contains_clarification_keywords(
            "Which approach should I take?"
        ));
        assert!(contains_clarification_keywords(
            "What is the expected behavior?"
        ));
        assert!(contains_clarification_keywords(
            "How should this be handled?"
        ));
        assert!(contains_clarification_keywords("When should we deploy?"));
        assert!(contains_clarification_keywords("Where is the config file?"));
        assert!(contains_clarification_keywords("Who is the target user?"));
        assert!(contains_clarification_keywords("Why is this needed?"));
        assert!(!contains_clarification_keywords("I will implement it"));
    }

    #[test]
    fn test_clarification_panel_all_answered() {
        let questions = vec![
            Question::new("Q1"),
            Question::new("Q2"),
            Question::new("Q3"),
        ];

        let mut panel = ClarificationPanel::new(questions);

        assert!(!panel.all_answered());

        panel.set_current_answer("A1".to_string());
        panel.select_next();
        assert!(!panel.all_answered());

        panel.set_current_answer("A2".to_string());
        panel.select_next();
        assert!(!panel.all_answered());

        panel.set_current_answer("A3".to_string());
        assert!(panel.all_answered());
        // Note: all_answered() doesn't automatically set completed
        assert!(!panel.completed);
        panel.complete();
        assert!(panel.completed);
    }

    #[test]
    fn test_clarification_panel_empty_questions() {
        let panel = ClarificationPanel::new(vec![]);
        assert!(panel.all_answered()); // Vacuously true
        assert_eq!(panel.answered_count(), 0);
    }

    #[test]
    fn test_clarification_panel_reset() {
        let questions = vec![Question::new("Question 1")];
        let mut panel = ClarificationPanel::new(questions);

        panel.set_current_answer("Answer 1".to_string());
        assert_eq!(panel.answered_count(), 1);

        panel.reset();
        assert_eq!(panel.answered_count(), 0);
        // Note: reset() doesn't change visible flag
        assert!(panel.visible);
        assert!(!panel.completed);
        assert_eq!(panel.questions.len(), 0);
    }

    #[test]
    fn test_is_rhetorical_question() {
        // These contain rhetorical patterns
        assert!(is_rhetorical_question("Would you like me to help?"));
        assert!(is_rhetorical_question("Do you want me to continue?"));
        assert!(is_rhetorical_question("Should I implement it now?"));
        assert!(is_rhetorical_question("Can I help with something else?"));

        // These don't contain rhetorical patterns
        assert!(!is_rhetorical_question("What is the input format?"));
        assert!(!is_rhetorical_question("How does this work?"));
        assert!(!is_rhetorical_question("When is the deadline?"));
        assert!(!is_rhetorical_question("Where is the config?"));
    }

    #[test]
    fn test_option_based_question() {
        let options = vec![
            QuestionOption {
                label: "Option A".to_string(),
                description: "First choice".to_string(),
            },
            QuestionOption {
                label: "Option B".to_string(),
                description: "Second choice".to_string(),
            },
        ];
        let questions = vec![Question::with_options("Which approach?", options)];
        let mut panel = ClarificationPanel::new(questions);

        assert!(panel.current_has_options());
        assert_eq!(panel.current_option_count(), 2);
        assert_eq!(panel.current_option_index(), 0);

        // Navigate options
        panel.select_next_option();
        assert_eq!(panel.current_option_index(), 1);

        panel.select_previous_option();
        assert_eq!(panel.current_option_index(), 0);

        // Select option
        panel.select_current_option();
        assert_eq!(panel.current_answer(), "Option A");
        assert!(panel.all_answered());
    }
}
