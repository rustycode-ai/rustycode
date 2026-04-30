use crate::types::LifecycleState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEvent {
    Load,
    Activate,
    ConditionMet,
    ConditionLost,
    Suspend,
    Resume,
    Demote,
    Promote,
    Archive,
    ConfirmDelete,
    QualityDegraded,
    QualityImproved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub from: LifecycleState,
    pub event: LifecycleEvent,
    pub to: LifecycleState,
}

pub struct LifecycleFsm {
    current: LifecycleState,
    history: Vec<Transition>,
}

impl LifecycleFsm {
    pub const fn new(initial: LifecycleState) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
        }
    }

    pub const fn current_state(&self) -> LifecycleState {
        self.current
    }

    #[allow(clippy::match_same_arms)]
    pub fn transition(&mut self, event: LifecycleEvent) -> Result<LifecycleState, LifecycleState> {
        let next = match (self.current, event) {
            (LifecycleState::Discovered, LifecycleEvent::Load) => LifecycleState::Loaded,
            (
                LifecycleState::Discovered | LifecycleState::Loaded | LifecycleState::Latent,
                LifecycleEvent::Activate,
            )
            | (LifecycleState::Latent, LifecycleEvent::ConditionMet)
            | (LifecycleState::Suspended, LifecycleEvent::Resume | LifecycleEvent::Promote)
            | (LifecycleState::Archived, LifecycleEvent::Promote) => LifecycleState::Active,
            (LifecycleState::Loaded, LifecycleEvent::ConditionMet)
            | (LifecycleState::Active, LifecycleEvent::ConditionLost) => LifecycleState::Latent,
            (LifecycleState::Active, LifecycleEvent::Suspend | LifecycleEvent::Demote) => {
                LifecycleState::Suspended
            }
            (
                LifecycleState::Active | LifecycleState::Latent | LifecycleState::Suspended,
                LifecycleEvent::Archive,
            )
            | (LifecycleState::Suspended, LifecycleEvent::ConfirmDelete) => {
                LifecycleState::Archived
            }
            (_, LifecycleEvent::QualityDegraded | LifecycleEvent::QualityImproved) => {
                return Err(self.current);
            }
            _ => return Err(self.current),
        };

        let transition = Transition {
            from: self.current,
            event,
            to: next,
        };
        self.history.push(transition);
        self.current = next;
        Ok(self.current)
    }

    #[allow(clippy::unnested_or_patterns)]
    pub const fn can_transition(&self, event: LifecycleEvent) -> bool {
        matches!(
            (self.current, event),
            (
                LifecycleState::Discovered,
                LifecycleEvent::Load | LifecycleEvent::Activate
            ) | (
                LifecycleState::Loaded | LifecycleState::Latent,
                LifecycleEvent::Activate
            ) | (
                LifecycleState::Loaded | LifecycleState::Latent,
                LifecycleEvent::ConditionMet
            ) | (
                LifecycleState::Active,
                LifecycleEvent::Suspend
                    | LifecycleEvent::Demote
                    | LifecycleEvent::Archive
                    | LifecycleEvent::ConditionLost
            ) | (
                LifecycleState::Latent | LifecycleState::Suspended,
                LifecycleEvent::Archive
            ) | (
                LifecycleState::Suspended,
                LifecycleEvent::Resume | LifecycleEvent::Promote | LifecycleEvent::ConfirmDelete
            ) | (LifecycleState::Archived, LifecycleEvent::Promote)
        )
    }

    pub fn history(&self) -> &[Transition] {
        &self.history
    }

    pub const fn transition_count(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fsm_starts_at_initial() {
        let fsm = LifecycleFsm::new(LifecycleState::Discovered);
        assert_eq!(fsm.current_state(), LifecycleState::Discovered);
    }

    #[test]
    fn discovered_to_loaded() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Discovered);
        let result = fsm.transition(LifecycleEvent::Load);
        assert_eq!(result, Ok(LifecycleState::Loaded));
    }

    #[test]
    fn discovered_to_active_directly() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Discovered);
        let result = fsm.transition(LifecycleEvent::Activate);
        assert_eq!(result, Ok(LifecycleState::Active));
    }

    #[test]
    fn loaded_to_active() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Loaded);
        let result = fsm.transition(LifecycleEvent::Activate);
        assert_eq!(result, Ok(LifecycleState::Active));
    }

    #[test]
    fn loaded_to_latent() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Loaded);
        let result = fsm.transition(LifecycleEvent::ConditionMet);
        assert_eq!(result, Ok(LifecycleState::Latent));
    }

    #[test]
    fn active_to_suspended() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Active);
        let result = fsm.transition(LifecycleEvent::Suspend);
        assert_eq!(result, Ok(LifecycleState::Suspended));
    }

    #[test]
    fn active_to_archived() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Active);
        let result = fsm.transition(LifecycleEvent::Archive);
        assert_eq!(result, Ok(LifecycleState::Archived));
    }

    #[test]
    fn active_to_latent_on_condition_lost() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Active);
        let result = fsm.transition(LifecycleEvent::ConditionLost);
        assert_eq!(result, Ok(LifecycleState::Latent));
    }

    #[test]
    fn latent_to_active_on_condition_met() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Latent);
        let result = fsm.transition(LifecycleEvent::ConditionMet);
        assert_eq!(result, Ok(LifecycleState::Active));
    }

    #[test]
    fn suspended_to_active_on_resume() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Suspended);
        let result = fsm.transition(LifecycleEvent::Resume);
        assert_eq!(result, Ok(LifecycleState::Active));
    }

    #[test]
    fn suspended_to_archived() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Suspended);
        let result = fsm.transition(LifecycleEvent::Archive);
        assert_eq!(result, Ok(LifecycleState::Archived));
    }

    #[test]
    fn archived_to_active_on_promote() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Archived);
        let result = fsm.transition(LifecycleEvent::Promote);
        assert_eq!(result, Ok(LifecycleState::Active));
    }

    #[test]
    fn invalid_transition_returns_err() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Archived);
        let result = fsm.transition(LifecycleEvent::Load);
        assert_eq!(result, Err(LifecycleState::Archived));
        assert_eq!(fsm.current_state(), LifecycleState::Archived);
    }

    #[test]
    fn can_transition_check() {
        let fsm = LifecycleFsm::new(LifecycleState::Active);
        assert!(fsm.can_transition(LifecycleEvent::Suspend));
        assert!(!fsm.can_transition(LifecycleEvent::Load));
    }

    #[test]
    fn history_tracks_transitions() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Discovered);
        fsm.transition(LifecycleEvent::Load).unwrap();
        fsm.transition(LifecycleEvent::Activate).unwrap();

        assert_eq!(fsm.transition_count(), 2);
        assert_eq!(fsm.history()[0].from, LifecycleState::Discovered);
        assert_eq!(fsm.history()[0].to, LifecycleState::Loaded);
        assert_eq!(fsm.history()[1].from, LifecycleState::Loaded);
        assert_eq!(fsm.history()[1].to, LifecycleState::Active);
    }

    #[test]
    fn quality_events_return_current_state() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Active);
        let result = fsm.transition(LifecycleEvent::QualityDegraded);
        assert_eq!(result, Err(LifecycleState::Active));
        assert_eq!(fsm.current_state(), LifecycleState::Active);
    }

    #[test]
    fn full_lifecycle_path() {
        let mut fsm = LifecycleFsm::new(LifecycleState::Discovered);
        fsm.transition(LifecycleEvent::Load).unwrap();
        fsm.transition(LifecycleEvent::Activate).unwrap();
        fsm.transition(LifecycleEvent::Suspend).unwrap();
        fsm.transition(LifecycleEvent::Archive).unwrap();
        fsm.transition(LifecycleEvent::Promote).unwrap();
        assert_eq!(fsm.current_state(), LifecycleState::Active);
        assert_eq!(fsm.transition_count(), 5);
    }
}
