use crate::app::features::{
    FeatureRegistry, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use crate::ui::message_search::SearchState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use std::sync::Mutex;

pub struct SearchFeature {
    state: Mutex<SearchFeatureState>,
}

impl Default for SearchFeature {
    fn default() -> Self {
        Self::new()
    }
}

struct SearchFeatureState {
    search: SearchState,
    visible: bool,
}

impl SearchFeature {
    pub const SURFACE: SurfaceId = SurfaceId::new("search_overlay");
    pub const ROUTE: RouteId = RouteId::new("search");
    pub const CMD_SEARCH: &str = "/search";
    pub const CMD_FIND: &str = "/find";

    pub fn new() -> Self {
        Self {
            state: Mutex::new(SearchFeatureState {
                search: SearchState::new(),
                visible: false,
            }),
        }
    }

    pub fn show(&self) {
        let mut state = self.state.lock().unwrap();
        state.visible = true;
        state.search.visible = true;
    }

    pub fn hide(&self) {
        let mut state = self.state.lock().unwrap();
        state.visible = false;
        state.search.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.state.lock().unwrap().visible
    }

    pub fn search_state(&self) -> SearchState {
        self.state.lock().unwrap().search.clone()
    }

    pub fn handle_command(&mut self, command: &str) -> Vec<TuiAction> {
        match command {
            cmd if cmd == Self::CMD_SEARCH || cmd == Self::CMD_FIND => {
                self.show();
                vec![TuiAction::MarkDirty]
            }
            _ => Vec::new(),
        }
    }
}

impl TuiFeature for SearchFeature {
    fn id(&self) -> &'static str {
        "search"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_SEARCH, self.id());
        reg.register_command(Self::CMD_FIND, self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        if let TuiEvent::Key(key) = event {
            let mut state = self.state.lock().unwrap();
            if !state.visible {
                return Vec::new();
            }
            match key {
                KeyEvent {
                    code: KeyCode::Esc, ..
                } => {
                    state.search.clear();
                    state.visible = false;
                    return vec![TuiAction::MarkDirty];
                }
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    state.search.next_match();
                    return vec![TuiAction::MarkDirty];
                }
                KeyEvent {
                    code: KeyCode::Char('n'),
                    modifiers: KeyModifiers::NONE,
                    ..
                } => {
                    state.search.next_match();
                    return vec![TuiAction::MarkDirty];
                }
                KeyEvent {
                    code: KeyCode::Char('N'),
                    modifiers: KeyModifiers::SHIFT,
                    ..
                } => {
                    state.search.prev_match();
                    return vec![TuiAction::MarkDirty];
                }
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::NONE,
                    ..
                } => {
                    state.search.toggle_case_sensitive();
                    return vec![TuiAction::MarkDirty];
                }
                KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                } => {
                    state.search.query.pop();
                    return vec![TuiAction::MarkDirty];
                }
                KeyEvent {
                    code: KeyCode::Char(ch),
                    modifiers: KeyModifiers::NONE,
                    ..
                } => {
                    state.search.query.push(*ch);
                    return vec![TuiAction::MarkDirty];
                }
                _ => {}
            }
        }
        Vec::new()
    }

    fn render(&self, surface: SurfaceId, frame: &mut Frame, ctx: &RenderCtx) {
        if surface != Self::SURFACE {
            return;
        }
        let state = self.state.lock().unwrap();
        if !state.visible {
            return;
        }
        let _ = (frame, ctx);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_hidden_state() {
        let f = SearchFeature::new();
        assert!(!f.is_visible());
    }

    #[test]
    fn show_makes_visible() {
        let f = SearchFeature::new();
        f.show();
        assert!(f.is_visible());
    }

    #[test]
    fn hide_makes_hidden() {
        let f = SearchFeature::new();
        f.show();
        f.hide();
        assert!(!f.is_visible());
    }

    #[test]
    fn register_adds_surface_route_commands() {
        let f = SearchFeature::new();
        let mut reg = FeatureRegistry::new();
        f.register(&mut reg);
        assert_eq!(reg.surface_feature(SearchFeature::SURFACE), Some("search"));
        assert_eq!(reg.route_feature(SearchFeature::ROUTE), Some("search"));
        assert_eq!(
            reg.command_feature(SearchFeature::CMD_SEARCH),
            Some("search")
        );
    }

    #[test]
    fn handle_command_search_shows() {
        let mut f = SearchFeature::new();
        let actions = f.handle_command("/search");
        assert!(f.is_visible());
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn as_any_mut_allows_downcast() {
        let mut f = SearchFeature::new();
        let any_ref = f.as_any_mut();
        assert!(any_ref.downcast_mut::<SearchFeature>().is_some());
    }
}
