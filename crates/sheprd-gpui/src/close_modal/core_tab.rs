//! Asking before a herdr-core (Herdr 2) terminal tab closes. The app removes
//! such a tab itself and its terminals end with it, without a daemon request
//! to decide anything, so it asks only when `confirm_close_idle_tab` makes
//! every tab ask: the snapshot does not say whether an agent in it is busy.

use super::CloseConfirmation;
use crate::{
    SheprdWindow,
    browser::{Location, Store, Tab, TabId},
    menu::Page,
};
use gpui::{Context, Window};

impl CloseConfirmation {
    /// The confirmation of herdr-core tab `tab`; none for a mirror, which
    /// only stops watching, or a page, which ends nothing.
    fn capture_core_tab(tab: &Tab) -> Option<Self> {
        let Some(Location::CoreTerminal {
            observe: false,
            name,
            ..
        }) = &tab.location
        else {
            return None;
        };
        Some(Self {
            boot: String::new(),
            workspace: tab.workspace_id.clone(),
            tab: String::new(),
            pane: None,
            core: Some(tab.id),
            label: name.clone().unwrap_or_else(|| tab.title.clone()),
            interrupts: None,
            confirm_selected: false,
            do_not_ask_again: false,
            error: None,
        })
    }
}

impl SheprdWindow {
    /// Closes page tab `id` as its close button does: at once, except a
    /// herdr-core terminal tab while `confirm_close_idle_tab` has every tab
    /// ask, which opens the confirmation before anything is removed.
    pub(crate) fn close_page_tab(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asks = self.config.confirm_close_tab && self.config.confirm_close_idle_tab;
        let close = asks
            .then(|| {
                Store::update(cx, |store| {
                    store.get(id).and_then(CloseConfirmation::capture_core_tab)
                })
            })
            .flatten();
        let Some(close) = close else {
            self.close_browser_tab(id, window, cx);
            return;
        };
        if self.open_menu(window, cx) {
            self.menu.close = Some(close);
            self.menu.page = Some(Page::ConfirmClose);
            cx.notify();
        }
    }
}
