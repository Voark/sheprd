//! Painting the window from prepared state. Render reads bounded caches and
//! the latest projection only: it never queries the daemon, touches disk, or
//! starts a process.
//!
//! The window draws in two parts (see `super::layers`): the chrome, a cached
//! view drawn here, and the terminals, drawn over it by the root (`root`)
//! from `cells` on every frame.

use super::{
    SheprdWindow,
    layers::{Hosted, lift},
    tab_strip::{Reach, native_page},
};
use crate::{browser::Shown, direction::Logical, terminal::*};
use gpui::{prelude::*, *};

mod cells;
mod root;

impl SheprdWindow {
    /// Config warnings, then the daemon's announcement, stacked over the
    /// top-right of the terminal area below the tab strip, whose buttons stay
    /// reachable. A menu page owns the window's attention, and zen mode keeps
    /// the window clear; they wait behind either.
    fn render_notices(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.notices_allowed() {
            return None;
        }
        let mut cards = self.config_diagnostic_cards(cx);
        cards.extend(self.announcement_card(cx));
        if cards.is_empty() {
            return None;
        }
        // Spans the terminal area so a narrow window shrinks the cards rather
        // than pushing them off the left edge. Each card sits in its own row:
        // a column only shrinks its children along its height, so a card with
        // a long line kept its full width there. The stack ends above the
        // window's foot, and a card that scrolls gives up height so the cards
        // under it stay on screen; the rest keep theirs. Neither the stack nor
        // a row takes the pointer, so the terminal around the cards scrolls
        // and selects as before.
        Some(
            div()
                .absolute()
                .top(px(self.tab_strip_height() + 8.))
                .bottom(px(8.))
                .left(px(8.))
                .right(px(8.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .children(cards.into_iter().map(|Notice { card, shrinks }| {
                    div()
                        .flex()
                        .justify_end()
                        .map(|row| match shrinks {
                            true => row.min_h_0(),
                            false => row.flex_none(),
                        })
                        .child(card)
                }))
                .into_any_element(),
        )
    }
}

/// A card the notice stack can draw.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NoticeCard {
    AppConfig,
    IconFont,
    DaemonConfig,
    Announcement,
}

/// One card in the notice stack, and whether it scrolls, so it can give up
/// height to keep the stack inside the window.
pub(super) struct Notice {
    pub(super) card: AnyElement,
    pub(super) shrinks: bool,
}

impl SheprdWindow {
    fn notices_allowed(&self) -> bool {
        self.menu.page.is_none() && !self.zen_active()
    }

    /// Whether [`Self::render_notices`] draws a card, so the keyboard can
    /// reach the cards as a region of their own.
    pub(super) fn notices_shown(&self) -> bool {
        use NoticeCard::*;
        [AppConfig, IconFont, DaemonConfig, Announcement]
            .into_iter()
            .any(|card| self.notice_shown(card))
    }

    /// Whether [`Self::render_notices`] draws `card`.
    pub(crate) fn notice_shown(&self, card: NoticeCard) -> bool {
        self.notices_allowed()
            && match card {
                NoticeCard::AppConfig => self.gui_config_diagnostic.visible().is_some(),
                NoticeCard::IconFont => self.icon_font_notice.visible().is_some(),
                NoticeCard::DaemonConfig => self
                    .endpoints
                    .get(self.selected_endpoint)
                    .is_some_and(|endpoint| endpoint.config_diagnostic.visible().is_some()),
                NoticeCard::Announcement => self.live.product_announcement().is_some(),
            }
    }
}

impl SheprdWindow {
    /// Everything the window draws except its terminals, which the chrome
    /// leaves to layers drawn over it: a cached view's content, drawn again
    /// when this view is notified (see `super::layers::ChromeView`).
    pub(super) fn render_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.layers.begin();
        self.tick_chrome(window, cx);
        self.tick_menu_appear(window);
        self.tick_sidebar_slide(window, cx);
        // The rows the accessibility tree reads are the live surface's, which
        // a replayed chrome would not follow.
        self.surface_in_chrome = window.is_a11y_active();
        let fill = self.fill();
        let sidebar = self
            .sidebar_column_width(f32::from(window.viewport_size().width))
            .map(|width| {
                // The sidebar is drawn over this box by the root; see
                // `super::layers`. It repeats the sidebar's own outer style.
                let room = div()
                    .w(px(width))
                    .flex_none()
                    .h_full()
                    .min_h_0()
                    .child(self.layers.place(Hosted::Sidebar));
                (room, width)
            });
        // The tab strip reserves its moving marks afresh; a hidden sidebar
        // leaves none behind. The root opened the frame for them.
        if let Some(marks) = crate::mark_layer::Marks::of(window, cx) {
            marks.forget(self.chrome_view.entity_id());
            if sidebar.is_none() {
                marks.forget(self.sidebar_view.entity_id());
            }
        }
        // Pad the terminal itself: the canvas bounds that painting, hit testing,
        // and IME placement all read then already exclude the padding. Only a
        // group that draws a terminal or stands in for one takes it: every
        // other kind of tab pads its own content.
        let padding = self.config.terminal_padding;
        self.ensure_layout();
        self.forget_gone_strips();
        let slots = self.drawn_group_slots();
        let shown: Vec<Shown> = slots
            .iter()
            .map(|slot| self.group_shown(slot.id, cx))
            .collect();
        // On Windows a divider's grip keeps off a native page, which would
        // cover it. A folding group stands between two groups without a
        // divider.
        let pages: Vec<bool> = if self.folding_groups().is_empty() {
            shown.iter().map(|shown| native_page(shown, cx)).collect()
        } else {
            Vec::new()
        };
        // The window's own terminal, with its input, goes to the group holding
        // its connection; other terminal groups paint parked connections.
        let primary = self.primary_group();
        let terminal_slot = slots
            .iter()
            .zip(&shown)
            .find(|(slot, shown)| {
                **shown == Shown::Terminal && primary.is_none_or(|group| group == slot.id)
            })
            .map(|(slot, _)| *slot);
        let terminal = padding
            .pad(div())
            .id("terminal")
            .debug_selector(|| "terminal".into())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.pane_pointer_moved(event, window, cx);
            }))
            .on_click(cx.listener(Self::open_terminal_link))
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            // On macOS a chat's page panel shows from beneath the window's
            // drawing, so the terminal's background paints around it.
            .when(!cfg!(target_os = "macos"), |terminal| {
                terminal.bg(fill.base(self.terminal_theme().background))
            })
            .children(self.terminal_ground(fill.base(self.terminal_theme().background)))
            .map(|terminal| self.accessible_terminal(terminal, window, cx))
            .map(|terminal| self.terminal_keys(terminal, cx))
            .on_scroll(cx.listener(Self::scroll_wheel))
            .on_drop(cx.listener(Self::drop_terminal_files))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.terminal_mouse_down(event, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.terminal_mouse_down(event, window, cx) {
                        return;
                    }
                    cx.stop_propagation();
                    this.open_pane_menu_at(event.position, window, cx);
                    this.menu.opening_right_click = this.menu.page == Some(crate::menu::Page::Pane);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.note_input();
                    if this.scrollbar_mouse_down(event, cx)
                        || this.split_mouse_down(event, cx)
                        || this.terminal_mouse_down(event, window, cx)
                    {
                        return;
                    }
                    this.pressed_terminal_link =
                        this.terminal_link_press(event.position, event.modifiers);
                    if this.menu.page.is_some() {
                        return;
                    }
                    // A press on a link may still turn into a drag across it,
                    // so the selection starts either way; the click that opens
                    // the link is the one that never left its half-cell.
                    this.begin_selection(
                        event.position,
                        event.click_count,
                        event.modifiers.shift,
                        cx,
                    );
                    if this.pressed_terminal_link.is_some() {
                        cx.stop_propagation();
                        return;
                    }
                    window.focus(&this.focus, cx);
                    if this.input_ready()
                        && let Some(surface) = &this.live.surface
                    {
                        let pane = pane_at(
                            surface,
                            this.bounds,
                            event.position,
                            this.cell_width,
                            this.config.terminal.line_height(),
                        )
                        .map(str::to_owned);
                        if let Some(id) = pane {
                            this.focus_clicked_pane(&id, cx);
                        }
                    }
                }),
            )
            // The cells, with what lies over them, are drawn over this
            // placeholder by the root; see `super::layers`.
            .child(self.layers.place(Hosted::Terminal));
        // A focus handle belongs to one element: the terminal when drawn,
        // otherwise the group in use.
        let keyboard = terminal_slot
            .is_none()
            .then(|| self.active_group())
            .flatten();
        let mut terminal = Some(terminal);
        // An activity bar section takes the body; its title bar is the
        // window's own, with no group's tab strip in it.
        let section = self.render_activity_section(window, cx);
        let merged = section.is_none()
            && slots
                .first()
                .is_some_and(|first| self.tabs_in_titlebar(first.id, cx));
        let count = slots.len();
        let mut groups = Vec::with_capacity(count);
        for (slot, shown) in slots.into_iter().zip(shown) {
            let owns_keyboard = keyboard == Some(slot.id);
            let tab = match &shown {
                Shown::Page(id) => cx
                    .try_global::<crate::browser::Store>()
                    .and_then(|store| store.get(*id))
                    .cloned(),
                _ => None,
            };
            let body = match (&shown, tab) {
                // No Herdr 1 is installed: herdr-core's shell stands in.
                (Shown::Terminal, _)
                    if terminal_slot == Some(slot) && self.core_fallback_view().is_some() =>
                {
                    self.core_fallback_view()
                        .map(IntoElement::into_any_element)
                        .unwrap_or_else(|| div().into_any_element())
                }
                (Shown::Terminal, _) if terminal_slot == Some(slot) => terminal
                    .take()
                    .map(IntoElement::into_any_element)
                    .unwrap_or_else(|| div().into_any_element()),
                (Shown::Terminal, _) if self.shows_parked_terminal(slot.id, cx) => {
                    self.render_parked_terminal(slot)
                }
                // Orchestrator tabs are dropped on load and never opened.
                (Shown::Page(_), Some(tab))
                    if matches!(
                        tab.location,
                        Some(crate::browser::Location::Orchestrator { .. })
                    ) =>
                {
                    div().into_any_element()
                }
                (Shown::Page(_), Some(tab))
                    if tab
                        .location
                        .as_ref()
                        .is_some_and(|location| !location.is_page()) =>
                {
                    match tab.location {
                        Some(crate::browser::Location::Code { .. }) => {
                            self.render_code_tab(slot, &tab, cx)
                        }
                        Some(crate::browser::Location::Devices) => {
                            self.render_devices_tab(slot, &tab, cx)
                        }
                        Some(crate::browser::Location::CoreTerminal { .. }) => {
                            self.render_core_terminal_tab(slot, &tab, cx)
                        }
                        _ => self.render_review_tab(slot, &tab, cx),
                    }
                }
                (Shown::Page(_), Some(tab)) => self.render_browser(slot, &tab, owns_keyboard, cx),
                (Shown::Elsewhere(tab), _) if self.has_live_frame_of(tab) => {
                    self.render_terminal_mirror(slot, tab)
                }
                _ => self.render_stand_in(slot, &shown, owns_keyboard, cx),
            };
            let ends = merged.then(|| crate::titlebar::Ends::of(slot.index, count));
            let reach = Reach::of(&pages, slot.index);
            groups.push(self.render_group(slot, body, ends, reach, window, cx));
        }
        let content = self.render_groups(groups, window, cx);
        let content =
            section.unwrap_or_else(|| self.with_mod_panel(self.zen_column(content), window, cx));
        // Not `||`: asking forgets group motion that has finished.
        if self.groups_moving()
            | self.tabs_growing()
            | self.annotations_moving()
            | self.chrome_sliding()
        {
            window.request_animation_frame();
        }
        // On Windows a menu that just opened hides the pages it covers once
        // it is laid out, a frame later.
        if self.present_browser(cx) {
            window.request_animation_frame();
        }
        // Zen mode keeps toasts and menus: they carry errors and what the
        // user asked for, not chrome.
        let zen = self.zen_layout();
        let root = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .map(|root| self.with_backdrop(root))
            .child({
                let entity = cx.weak_entity();
                let menu_bounds = self.menu.application_bar.content.clone();
                canvas(
                    move |bounds, _, _| menu_bounds.set(bounds),
                    move |_, _, window, _| {
                        let scroll_entity = entity.clone();
                        window.on_mouse_event(move |_: &ScrollEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ =
                                    scroll_entity.update(cx, |this, _| this.shift_taps.cancel());
                            }
                        });
                        let entity = entity.clone();
                        window.on_mouse_event(move |_: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = entity.update(cx, |this, _| this.shift_taps.cancel());
                            }
                        });
                    },
                )
                .absolute()
                .size_full()
            })
            .when(crate::menus::IN_WINDOW && !zen, |root| {
                root.child(self.render_application_bar(window, cx))
            })
            .when(!merged && !zen, |root| {
                root.child(self.render_titlebar(window, cx))
            })
            .when(zen, |root| {
                root.children(crate::titlebar::zen_inset(fill.ground(), window))
            })
            .child(
                div()
                    .debug_selector(|| "window-body".into())
                    // The sidebar leads, on the right in a right-to-left
                    // language; the terminal area beside it never mirrors.
                    .row()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .children(sidebar.map(|(sidebar, width)| {
                        let column = match merged {
                            true => self.sidebar_column(sidebar, window, cx).into_any_element(),
                            false => sidebar.into_any_element(),
                        };
                        self.sliding_sidebar(column, width)
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .relative()
                            .child(content)
                            // Over the terminal area, so over its layer.
                            .children(self.render_notices(cx).map(lift))
                            .children(self.zen_status_bar(cx)),
                    ),
            )
            // The marks the body's rows reserve are drawn by the root, over
            // the layers; what covers the body is lifted over both.
            // At the window's foot in every layout: under the traffic lights
            // it would hide them, and one place is easier to find.
            .when(!zen, |root| {
                root.children(self.render_worktree_banner().map(lift))
            })
            .children(self.render_tab_ghost(cx))
            .children(self.render_toasts(window, cx).into_iter().map(lift))
            .children(self.render_file_transfer(window, cx).map(lift))
            .children(self.render_menu_leaving(window, cx).into_iter().map(lift))
            .when(self.menu.page.is_some(), |root| {
                root.child(lift(self.render_menu(window, cx)))
            });
        crate::titlebar::frame(window, self.theme.active, root).into_any_element()
    }
}
