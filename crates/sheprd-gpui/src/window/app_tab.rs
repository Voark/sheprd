//! A tab the app draws itself rather than a web page: each kind of
//! location goes to the view that draws it.
use super::SheprdWindow;
use crate::browser::{Location, Slot, Tab};
use gpui::{AnyElement, Context};

impl SheprdWindow {
    /// The body of `tab` in `slot`, for a location that is not a page.
    pub(super) fn render_app_tab(
        &mut self,
        slot: Slot,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match tab.location {
            Some(Location::Code { .. }) => self.render_code_tab(slot, tab, cx),
            Some(Location::Files { .. }) => self.render_file_tree_tab(slot, tab, cx),
            Some(Location::Devices) => self.render_devices_tab(slot, tab, cx),
            Some(Location::CoreTerminal { .. }) => self.render_core_terminal_tab(slot, tab, cx),
            _ => self.render_review_tab(slot, tab, cx),
        }
    }
}
