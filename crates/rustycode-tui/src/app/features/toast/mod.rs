use std::sync::{Arc, Mutex};

use crate::app::features::{
    FeatureRegistry, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use crate::theme::ThemeColors;
use crate::ui::errors::{ErrorDisplay, ErrorManager, ErrorSeverity};
use crate::ui::toast::{Toast, ToastLevel, ToastManager};

pub struct ToastFeature {
    toasts: Mutex<ToastManager>,
    errors: Mutex<ErrorManager>,
    theme_colors: Arc<Mutex<ThemeColors>>,
    tick_delta_ms: u64,
}

impl ToastFeature {
    pub const SURFACE: SurfaceId = SurfaceId::new("toast_overlay");
    pub const ROUTE: RouteId = RouteId::new("toast");
    pub const CMD_ADD: &str = "/toast";
    pub const CMD_ERROR: &str = "/error";

    pub fn new(theme_colors: Arc<Mutex<ThemeColors>>) -> Self {
        Self {
            toasts: Mutex::new(ToastManager::new()),
            errors: Mutex::new(ErrorManager::new()),
            theme_colors,
            tick_delta_ms: 16,
        }
    }

    pub fn with_tick_delta(mut self, delta_ms: u64) -> Self {
        self.tick_delta_ms = delta_ms;
        self
    }

    pub fn show_toast(&self, level: ToastLevel, message: &str) {
        let toast = Toast::new(level, message);
        if let Ok(mut mgr) = self.toasts.lock() {
            mgr.add(toast);
        }
    }

    pub fn show_error(&self, severity: ErrorSeverity, message: &str) {
        let display = ErrorDisplay::new(severity, message);
        if let Ok(mut mgr) = self.errors.lock() {
            mgr.show(display);
        }
    }

    pub fn dismiss_toast(&self, id: usize) -> bool {
        if let Ok(mut mgr) = self.toasts.lock() {
            return mgr.remove(id);
        }
        false
    }

    pub fn clear_toasts(&self) {
        if let Ok(mut mgr) = self.toasts.lock() {
            mgr.clear();
        }
    }

    pub fn has_active_toasts(&self) -> bool {
        if let Ok(mgr) = self.toasts.lock() {
            return mgr.has_active();
        }
        false
    }

    pub fn active_count(&self) -> usize {
        if let Ok(mgr) = self.toasts.lock() {
            return mgr.active().len();
        }
        0
    }

    pub fn handle_command(&mut self, command: &str) -> Vec<TuiAction> {
        if command.starts_with(Self::CMD_ADD) {
            let rest = command.strip_prefix(Self::CMD_ADD).unwrap_or("").trim();
            if !rest.is_empty() {
                self.show_toast(ToastLevel::Info, rest);
            }
        } else if command.starts_with(Self::CMD_ERROR) {
            let rest = command.strip_prefix(Self::CMD_ERROR).unwrap_or("").trim();
            if !rest.is_empty() {
                self.show_error(ErrorSeverity::Error, rest);
            }
        }
        Vec::new()
    }
}

impl TuiFeature for ToastFeature {
    fn id(&self) -> &'static str {
        "toast"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_ADD, self.id());
        reg.register_command(Self::CMD_ERROR, self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Tick => {
                if let Ok(mut mgr) = self.toasts.lock() {
                    mgr.tick(self.tick_delta_ms);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn render(&self, surface: SurfaceId, frame: &mut ratatui::Frame, ctx: &RenderCtx) {
        if surface != Self::SURFACE {
            return;
        }
        if let Ok(mgr) = self.toasts.lock() {
            if !mgr.has_active() {
                return;
            }
            let area = ctx.frame_area;
            let colors = self.theme_colors.lock().ok();
            let theme_ref = colors.as_ref().map(|c| c as &ThemeColors);
            mgr.render(frame, area, theme_ref);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;
    use std::sync::{Arc, Mutex};

    fn test_theme_colors() -> ThemeColors {
        ThemeColors {
            background: Color::Black,
            foreground: Color::White,
            primary: Color::Cyan,
            secondary: Color::Magenta,
            accent: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
        }
    }

    fn make_feature() -> ToastFeature {
        let colors = Arc::new(Mutex::new(test_theme_colors()));
        ToastFeature::new(colors)
    }

    fn make_update_ctx<'a>(
        theme_colors: &'a ThemeColors,
        navigate: &'a mut dyn FnMut(RouteId),
        dispatch_command: &'a mut dyn FnMut(&str),
        approve_tool: &'a mut dyn FnMut(String, bool),
    ) -> UpdateCtx<'a> {
        UpdateCtx {
            has_focus: false,
            focused_surface: None,
            is_streaming: false,
            pending_tools: 0,
            plan_mode_active: false,
            auto_continue_enabled: false,
            theme_colors,
            navigate,
            dispatch_command,
            approve_tool,
        }
    }

    #[test]
    fn new_creates_empty_state() {
        let f = make_feature();
        assert!(!f.has_active_toasts());
        assert_eq!(f.active_count(), 0);
    }

    #[test]
    fn show_toast_adds_active() {
        let f = make_feature();
        f.show_toast(ToastLevel::Info, "hello");
        assert!(f.has_active_toasts());
        assert_eq!(f.active_count(), 1);
    }

    #[test]
    fn show_multiple_toasts() {
        let f = make_feature();
        f.show_toast(ToastLevel::Info, "first");
        f.show_toast(ToastLevel::Success, "second");
        f.show_toast(ToastLevel::Warning, "third");
        assert_eq!(f.active_count(), 3);
    }

    #[test]
    fn dismiss_toast_removes_it() {
        let f = make_feature();
        let id = {
            let mut mgr = f.toasts.lock().unwrap();
            mgr.add(Toast::new(ToastLevel::Info, "bye"))
        };
        assert!(f.dismiss_toast(id));
        assert!(!f.has_active_toasts());
    }

    #[test]
    fn dismiss_nonexistent_returns_false() {
        let f = make_feature();
        assert!(!f.dismiss_toast(999));
    }

    #[test]
    fn clear_toasts_removes_all() {
        let f = make_feature();
        f.show_toast(ToastLevel::Info, "a");
        f.show_toast(ToastLevel::Info, "b");
        f.clear_toasts();
        assert!(!f.has_active_toasts());
    }

    #[test]
    fn show_error_stores_in_error_manager() {
        let f = make_feature();
        f.show_error(ErrorSeverity::Error, "something broke");
        let mut mgr = f.errors.lock().unwrap();
        assert!(mgr.is_showing());
    }

    #[test]
    fn register_registers_surface_and_route() {
        let f = make_feature();
        let mut reg = FeatureRegistry::new();
        f.register(&mut reg);
        assert_eq!(reg.surface_feature(ToastFeature::SURFACE), Some("toast"));
        assert_eq!(reg.route_feature(ToastFeature::ROUTE), Some("toast"));
    }

    #[test]
    fn register_registers_commands() {
        let f = make_feature();
        let mut reg = FeatureRegistry::new();
        f.register(&mut reg);
        assert_eq!(reg.command_feature(ToastFeature::CMD_ADD), Some("toast"));
        assert_eq!(reg.command_feature(ToastFeature::CMD_ERROR), Some("toast"));
    }

    #[test]
    fn update_tick_advances_timers() {
        let mut f = make_feature();
        f.show_toast(ToastLevel::Info, "short-lived");
        let tc = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut cmd = |_c: &str| {};
        let mut tool = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&tc, &mut nav, &mut cmd, &mut tool);
        let actions = f.update(&TuiEvent::Tick, &mut ctx);
        assert!(actions.is_empty());
        assert!(f.has_active_toasts());
    }

    #[test]
    fn update_ignores_non_tick() {
        let mut f = make_feature();
        let tc = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut cmd = |_c: &str| {};
        let mut tool = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&tc, &mut nav, &mut cmd, &mut tool);
        let actions = f.update(&TuiEvent::FocusGained, &mut ctx);
        assert!(actions.is_empty());
    }

    #[test]
    fn handle_command_add_toast() {
        let mut f = make_feature();
        f.handle_command("/toast hello world");
        assert!(f.has_active_toasts());
    }

    #[test]
    fn handle_command_add_error() {
        let mut f = make_feature();
        f.handle_command("/error something failed");
        let mut mgr = f.errors.lock().unwrap();
        assert!(mgr.is_showing());
    }

    #[test]
    fn handle_command_unknown_returns_empty() {
        let mut f = make_feature();
        let actions = f.handle_command("/unknown");
        assert!(actions.is_empty());
        assert!(!f.has_active_toasts());
    }

    #[test]
    fn as_any_mut_allows_downcast() {
        let mut f = make_feature();
        let any_ref = f.as_any_mut();
        let downcast = any_ref.downcast_mut::<ToastFeature>();
        assert!(downcast.is_some());
    }

    #[test]
    fn with_tick_delta_customizes_delta() {
        let colors = Arc::new(Mutex::new(test_theme_colors()));
        let f = ToastFeature::new(colors).with_tick_delta(50);
        assert_eq!(f.tick_delta_ms, 50);
    }
}
