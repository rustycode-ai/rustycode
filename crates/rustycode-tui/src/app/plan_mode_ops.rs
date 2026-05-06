//! Plan mode UI state and helpers.
//!
//! This module keeps the user-facing plan-mode banner state separate from the
//! execution gate itself. The gate lives in `rustycode-orchestration`, while this
//! module manages how the TUI explains planning, stalls, and mode switches.

use crate::app::event_loop::TUI;
use crate::ui::header::HeaderStatus;
use rustycode_protocol::MilestoneStatus;

/// User-facing plan mode banner shown in the persistent status bar / header.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanModeBanner {
    /// Planning is active for a specific convoy.
    Planning {
        convoy_id: String,
        action_hint: String,
    },
    /// A plan has been created and is awaiting user approval.
    AwaitingApproval {
        convoy_id: String,
        plan_summary: String,
        action_hint: String,
    },
    /// The plan for the convoy has been approved and is ready to execute.
    PlanApproved {
        convoy_id: String,
        action_hint: String,
    },
    /// Implementation is actively executing for a specific convoy.
    Executing {
        convoy_id: String,
        current_task: String,
        action_hint: String,
    },
    /// A milestone is actively sequencing multiple plans.
    MilestoneProgress {
        milestone_title: String,
        status: MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: String,
        action_hint: String,
    },
}

impl PlanModeBanner {
    /// Banner title shown in the status bar.
    pub(crate) fn title(&self) -> &'static str {
        match self {
            Self::Planning { .. } => "Planning",
            Self::AwaitingApproval { .. } => "Approval Required",
            Self::PlanApproved { .. } => "Plan Approved",
            Self::Executing { .. } => "Executing",
            Self::MilestoneProgress { status, .. } => match status {
                MilestoneStatus::Validating => "Milestone Validation",
                MilestoneStatus::Completed => "Milestone Complete",
                MilestoneStatus::Paused => "Milestone Paused",
                MilestoneStatus::Failed => "Milestone Failed",
                _ => "Milestone Progress",
            },
        }
    }

    /// Main descriptive text shown in the status bar.
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Planning {
                convoy_id,
                action_hint,
            } => {
                format!("[{}] Planning active. {}", convoy_id, action_hint)
            }
            Self::AwaitingApproval {
                convoy_id,
                plan_summary,
                action_hint,
            } => {
                format!("[{}] {}. {}", convoy_id, plan_summary, action_hint)
            }
            Self::PlanApproved {
                convoy_id,
                action_hint,
            } => {
                format!("[{}] Plan approved. {}", convoy_id, action_hint)
            }
            Self::Executing {
                convoy_id,
                current_task,
                action_hint,
            } => {
                format!("[{}] {}. {}", convoy_id, current_task, action_hint)
            }
            Self::MilestoneProgress {
                milestone_title,
                status,
                plans_total,
                plans_completed,
                current_plan_summary,
                action_hint,
            } => {
                let summary = match status {
                    MilestoneStatus::Validating => format!(
                        "[{}] {}/{} plans complete. Validating. {}",
                        milestone_title, plans_completed, plans_total, action_hint
                    ),
                    MilestoneStatus::Completed => format!(
                        "[{}] {}/{} plans complete. Completed. {}",
                        milestone_title, plans_completed, plans_total, action_hint
                    ),
                    MilestoneStatus::Paused => format!(
                        "[{}] {}/{} plans complete. Paused at {}. {}",
                        milestone_title,
                        plans_completed,
                        plans_total,
                        current_plan_summary,
                        action_hint
                    ),
                    MilestoneStatus::Failed => format!(
                        "[{}] {}/{} plans complete. Failed at {}. {}",
                        milestone_title,
                        plans_completed,
                        plans_total,
                        current_plan_summary,
                        action_hint
                    ),
                    _ => format!(
                        "[{}] {}/{} plans complete. {}. {}",
                        milestone_title,
                        plans_completed,
                        plans_total,
                        current_plan_summary,
                        action_hint
                    ),
                };

                summary
            }
        }
    }

    /// Short user-facing message that can also be surfaced in chat.
    pub(crate) fn message(&self) -> String {
        self.description()
    }

    /// Color accent used for the banner.
    pub(crate) fn status_color(&self) -> ratatui::style::Color {
        match self {
            Self::Planning { .. } => ratatui::style::Color::Cyan,
            Self::AwaitingApproval { .. } => ratatui::style::Color::Yellow,
            Self::PlanApproved { .. } => ratatui::style::Color::Green,
            Self::Executing { .. } => ratatui::style::Color::Blue,
            Self::MilestoneProgress { status, .. } => match status {
                MilestoneStatus::Completed => ratatui::style::Color::Green,
                MilestoneStatus::Failed => ratatui::style::Color::Red,
                MilestoneStatus::Paused => ratatui::style::Color::Yellow,
                MilestoneStatus::Validating => ratatui::style::Color::Cyan,
                _ => ratatui::style::Color::Magenta,
            },
        }
    }

    /// Header status to use while this banner is active.
    pub(crate) fn header_status(&self) -> HeaderStatus {
        match self {
            Self::Planning { .. } | Self::AwaitingApproval { .. } => HeaderStatus::Planning,
            Self::PlanApproved { .. } | Self::Executing { .. } => HeaderStatus::RunningTools,
            Self::MilestoneProgress { status, .. } => match status {
                MilestoneStatus::Completed => HeaderStatus::Ready,
                MilestoneStatus::Failed => HeaderStatus::Error,
                MilestoneStatus::Paused => HeaderStatus::Stalled,
                _ => HeaderStatus::RunningTools,
            },
        }
    }
}

impl TUI {
    /// Replace the current plan-mode banner.
    pub(crate) fn set_plan_mode_banner(&mut self, banner: Option<PlanModeBanner>) {
        if self.plan_mode_banner == banner {
            return;
        }

        self.plan_mode_banner = banner;
        self.dirty = true;
    }

    /// Clear any active plan-mode banner.
    pub(crate) fn clear_plan_mode_banner(&mut self) {
        self.session_sidebar.clear_milestone_progress();
        self.set_plan_mode_banner(None);
    }

    /// Show that planning mode is active for a specific convoy.
    pub(crate) fn show_planning_banner(&mut self, convoy_id: &str) {
        self.session_sidebar.clear_milestone_progress();
        self.set_plan_mode_banner(Some(PlanModeBanner::Planning {
            convoy_id: convoy_id.to_string(),
            action_hint: "Building strategy...".to_string(),
        }));
    }

    /// Show that a plan is ready for review.
    pub(crate) fn show_approval_banner(&mut self, convoy_id: &str, plan_summary: &str) {
        self.session_sidebar.clear_milestone_progress();
        self.set_plan_mode_banner(Some(PlanModeBanner::AwaitingApproval {
            convoy_id: convoy_id.to_string(),
            plan_summary: plan_summary.to_string(),
            action_hint: "Review and approve plan to proceed.".to_string(),
        }));
        self.toast_manager
            .info(format!("[{}] Plan ready for review", convoy_id));
    }

    /// Show that a plan has been approved.
    pub(crate) fn show_plan_approved_banner(&mut self, convoy_id: &str) {
        self.session_sidebar.clear_milestone_progress();
        self.set_plan_mode_banner(Some(PlanModeBanner::PlanApproved {
            convoy_id: convoy_id.to_string(),
            action_hint: "Plan approved. Starting execution...".to_string(),
        }));
    }

    /// Show active execution status for a convoy task.
    pub(crate) fn show_executing_banner(&mut self, convoy_id: &str, current_task: &str) {
        self.session_sidebar.clear_milestone_progress();
        self.set_plan_mode_banner(Some(PlanModeBanner::Executing {
            convoy_id: convoy_id.to_string(),
            current_task: current_task.to_string(),
            action_hint: "Working...".to_string(),
        }));
    }

    pub(crate) fn show_milestone_progress_banner(
        &mut self,
        milestone_title: &str,
        status: MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: &str,
        action_hint: &str,
    ) {
        self.set_plan_mode_banner(Some(PlanModeBanner::MilestoneProgress {
            milestone_title: milestone_title.to_string(),
            status,
            plans_total,
            plans_completed,
            current_plan_summary: current_plan_summary.to_string(),
            action_hint: action_hint.to_string(),
        }));
    }

    pub(crate) fn is_awaiting_approval(&self) -> bool {
        matches!(
            self.plan_mode_banner,
            Some(PlanModeBanner::AwaitingApproval { .. })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn planning_banner() -> PlanModeBanner {
        PlanModeBanner::Planning {
            convoy_id: "c1".into(),
            action_hint: "Building...".into(),
        }
    }

    fn approval_banner() -> PlanModeBanner {
        PlanModeBanner::AwaitingApproval {
            convoy_id: "c1".into(),
            plan_summary: "Do stuff".into(),
            action_hint: "Review.".into(),
        }
    }

    fn approved_banner() -> PlanModeBanner {
        PlanModeBanner::PlanApproved {
            convoy_id: "c1".into(),
            action_hint: "Starting...".into(),
        }
    }

    fn executing_banner() -> PlanModeBanner {
        PlanModeBanner::Executing {
            convoy_id: "c1".into(),
            current_task: "Writing code".into(),
            action_hint: "Working...".into(),
        }
    }

    fn milestone_banner(status: MilestoneStatus) -> PlanModeBanner {
        PlanModeBanner::MilestoneProgress {
            milestone_title: "Phase 1".into(),
            status,
            plans_total: 3,
            plans_completed: 1,
            current_plan_summary: "Step 2".into(),
            action_hint: "Keep going".into(),
        }
    }

    #[test]
    fn title_matches_variant() {
        assert_eq!(planning_banner().title(), "Planning");
        assert_eq!(approval_banner().title(), "Approval Required");
        assert_eq!(approved_banner().title(), "Plan Approved");
        assert_eq!(executing_banner().title(), "Executing");
        assert_eq!(
            milestone_banner(MilestoneStatus::Validating).title(),
            "Milestone Validation"
        );
        assert_eq!(
            milestone_banner(MilestoneStatus::Completed).title(),
            "Milestone Complete"
        );
        assert_eq!(
            milestone_banner(MilestoneStatus::Failed).title(),
            "Milestone Failed"
        );
    }

    #[test]
    fn description_contains_convoy_id() {
        assert!(planning_banner().description().contains("[c1]"));
        assert!(approval_banner().description().contains("[c1]"));
        assert!(approved_banner().description().contains("[c1]"));
        assert!(executing_banner().description().contains("[c1]"));
    }

    #[test]
    fn description_includes_action_hint() {
        assert!(planning_banner().description().contains("Building..."));
        assert!(approval_banner().description().contains("Review."));
    }

    #[test]
    fn description_milestone_shows_progress() {
        let desc = milestone_banner(MilestoneStatus::default()).description();
        assert!(desc.contains("1/3"));
        assert!(desc.contains("Phase 1"));
    }

    #[test]
    fn status_color_variants() {
        assert_eq!(planning_banner().status_color(), Color::Cyan);
        assert_eq!(approval_banner().status_color(), Color::Yellow);
        assert_eq!(approved_banner().status_color(), Color::Green);
        assert_eq!(executing_banner().status_color(), Color::Blue);
        assert_eq!(
            milestone_banner(MilestoneStatus::Completed).status_color(),
            Color::Green
        );
        assert_eq!(
            milestone_banner(MilestoneStatus::Failed).status_color(),
            Color::Red
        );
    }

    #[test]
    fn header_status_mapping() {
        use HeaderStatus::*;
        assert_eq!(planning_banner().header_status(), Planning);
        assert_eq!(approval_banner().header_status(), Planning);
        assert_eq!(approved_banner().header_status(), RunningTools);
        assert_eq!(executing_banner().header_status(), RunningTools);
        assert_eq!(
            milestone_banner(MilestoneStatus::Completed).header_status(),
            Ready
        );
        assert_eq!(
            milestone_banner(MilestoneStatus::Failed).header_status(),
            Error
        );
    }

    #[test]
    fn message_delegates_to_description() {
        let banner = executing_banner();
        assert_eq!(banner.message(), banner.description());
    }
}
