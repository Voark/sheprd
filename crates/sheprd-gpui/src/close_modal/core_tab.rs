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
use gpui::{App, Context, FocusHandle, Window};

/// What held the keyboard as a herdr-core tab's close dialog opened.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReturnFocus {
    focus: FocusHandle,
    /// Whether it was a herdr-core pane's, which may have gone since.
    from_pane: bool,
}

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
            return_focus: None,
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
        // Read before the dialog takes the keyboard.
        let return_focus = window.focused(cx).map(|focus| ReturnFocus {
            from_pane: self.is_core_pane_focus(&focus, cx),
            focus,
        });
        let asks = self.config.confirm_close_tab && self.config.confirm_close_idle_tab;
        let close = asks
            .then(|| {
                Store::update(cx, |store| {
                    store.get(id).and_then(CloseConfirmation::capture_core_tab)
                })
            })
            .flatten();
        let Some(mut close) = close else {
            self.close_browser_tab(id, window, cx);
            return;
        };
        close.return_focus = return_focus;
        if self.open_menu(window, cx) {
            self.menu.close = Some(close);
            self.menu.page = Some(Page::ConfirmClose);
            cx.notify();
        }
    }

    /// Whether the close confirmation is open for herdr-core tab `id`.
    pub(crate) fn confirming_core_tab(&self, id: TabId) -> bool {
        self.menu.page == Some(Page::ConfirmClose)
            && self
                .menu
                .close
                .as_ref()
                .is_some_and(|close| close.core == Some(id))
    }

    /// Where the keyboard goes back to as a herdr-core tab's close dialog
    /// is dismissed: what held it as the dialog opened, or the pane in use
    /// of the tab when that was a pane whose program has exited since,
    /// rather than the window, whose keys a Herdr pane would take.
    pub(crate) fn close_return_focus(&self, cx: &App) -> Option<FocusHandle> {
        let close = self.menu.close.as_ref()?;
        let tab = close.core?;
        let held = close.return_focus.as_ref()?;
        if !held.from_pane || self.is_core_pane_focus(&held.focus, cx) {
            return Some(held.focus.clone());
        }
        Some(self.core_pane_view(tab)?.read(cx).focus_handle().clone())
    }
}
