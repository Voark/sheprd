//! The window's root view: the chrome as a cached view, and the terminals
//! drawn over it (see `super::super::layers`). It draws on every frame, so it
//! holds only what every frame needs: the window's own listeners, the
//! terminal's metrics, and what the layers read.

use super::super::SheprdWindow;
use crate::{
    CheckForUpdates, Minimize, PlaySound, RunCommand, ShowHerdrNotDetected, ShowUpdatePreview,
    ZoomWindow,
    actions::{RingBellPreview, ShowToastPreview},
    direction::Logical,
    fonts::StyledFont,
};
use gpui::{prelude::*, *};

impl Render for SheprdWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.restore_menu_focus(window, cx);
        self.leave_vanished_region(window, cx);
        self.sync_backdrop(window, cx);
        // Chats and the copy flash lie over the cells, in the terminal's
        // layer, so their motion redraws only it.
        if self
            .chats
            .glide(crate::motion::now(), self.config.chat.glide)
            | self.flash_moving()
        {
            self.redraw_terminal_next_frame(window);
        }
        self.sync_device_themes(cx);
        self.viewport_width = f32::from(window.viewport_size().width);
        let font = self.config.terminal.font();
        self.painter.borrow_mut().set_appearance(
            self.config.terminal.size,
            self.config.terminal.line_height(),
            self.terminal_theme().clone(),
        );
        self.cell_width = self.painter.borrow_mut().cell_width(&font, window, cx);
        // Every frame opens the marks for what draws again in it: the chrome,
        // the sidebar, or neither.
        if let Some(marks) = crate::mark_layer::Marks::of(window, cx) {
            marks.begin_frame(cx.entity_id());
        }
        // Registers the window for surface-only redraws; see `redraw_terminal`.
        self.surface_signal.read(cx);
        self.hovered_terminal_link =
            self.terminal_link_hovered(window.mouse_position(), window.modifiers());
        self.split_cursor = self.split_cursor_at(window.mouse_position());
        let chrome = self
            .chrome_view
            .clone()
            .cached(StyleRefinement::default().size_full());
        div()
            .capture_key_down(cx.listener(Self::application_menu_shortcut))
            .capture_key_down(cx.listener(Self::chrome_focus_key))
            .on_modifiers_changed(cx.listener(Self::double_shift_modifiers))
            .capture_any_mouse_down(cx.listener(|this, _, _, _| this.shift_taps.cancel()))
            .on_action(
                cx.listener(|this, action: &crate::actions::SetLayout, _, cx| {
                    this.set_layout(action.mode, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &crate::actions::SetChatLayout, _, cx| {
                    this.set_chat_layout(action.mode, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &crate::actions::SetTabLayout, _, cx| {
                    this.set_tab_layout(action.mode, cx);
                }),
            )
            .on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                this.command(action.command, window, cx);
            }))
            .on_action(cx.listener(|_, _: &Minimize, window, _| {
                window.minimize_window();
            }))
            .on_action(cx.listener(|_, _: &ZoomWindow, window, _| {
                window.zoom_window();
            }))
            .on_action(cx.listener(|this, _: &ShowHerdrNotDetected, window, cx| {
                this.show_install_modal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CheckForUpdates, window, cx| {
                this.open_app_update(false, window, cx);
                this.updater.check();
            }))
            .on_action(cx.listener(|this, _: &ShowUpdatePreview, window, cx| {
                this.open_app_update(true, window, cx);
            }))
            .on_action(cx.listener(
                |this, _: &crate::actions::ShowUpdateDownloadPreview, window, cx| {
                    this.open_update_progress_preview(
                        crate::updater::State::Downloading {
                            received: 50_000_000,
                            total: 100_000_000,
                        },
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::actions::ShowUpdateHomebrewPreview, window, cx| {
                    this.open_update_progress_preview(
                        crate::updater::State::Upgrading {
                            detail: crate::updater::Progress::Refreshing,
                        },
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(|this, action: &ShowToastPreview, _, cx| {
                this.show_toast_preview(action.kind, cx);
            }))
            .on_action(cx.listener(
                |this, _: &crate::actions::ShowSystemNotificationPreview, window, cx| {
                    this.show_system_notification_preview(window, cx);
                },
            ))
            .on_action(cx.listener(|this, _: &PlaySound, _, _| {
                this.sound.preview();
            }))
            .on_action(cx.listener(|this, _: &RingBellPreview, window, cx| {
                this.preview_bell(window, cx);
            }))
            // A mouse's side buttons walk the focus trail, as they do pages
            // in a browser or folders in a file manager.
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, window, cx| {
                    this.command(crate::controls::Command::Back, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, window, cx| {
                    this.command(crate::controls::Command::Forward, window, cx);
                }),
            )
            .size_full()
            .relative()
            .text_color(rgb(self.theme.foreground))
            .text_font(&self.config.ui)
            .text_size(px(self.config.ui.size))
            // Text sets from the side it starts, the right in Hebrew; the
            // terminal paints its own cells and never follows.
            .text_start()
            .child(chrome)
            .child(self.layers.element(cx.entity()))
            // After the body and its layers, whose rows reserve the marks it
            // draws, and under everything the chrome lifts over the body.
            // Its own motion redraws it alone, not the chrome.
            .child(AnyView::from(self.mark_layer.clone()))
    }
}

impl SheprdWindow {
    /// Draws the terminal layers again on the next frame, for motion in
    /// them. `Window::request_animation_frame` would notify this view, which
    /// draws the chrome again too.
    pub(in crate::window) fn redraw_terminal_next_frame(&self, window: &Window) {
        let signal = self.surface_signal.clone();
        window.on_next_frame(move |_, cx| signal.update(cx, |_, cx| cx.notify()));
    }

    /// The layers have painted. On macOS the window's grounds leave holes
    /// where pages show from beneath its drawing, and a page in a chat moves
    /// with the chat, in a layer: when the pages moved, the chrome draws its
    /// holes again on the next frame.
    pub(in crate::window) fn layers_painted(&mut self, window: &mut Window) {
        #[cfg(target_os = "macos")]
        {
            let pages: Vec<Bounds<Pixels>> = self
                .browser
                .drawn_pages()
                .borrow()
                .values()
                .copied()
                .collect();
            if pages != self.painted_pages {
                self.painted_pages = pages;
                let chrome = self.chrome_view.clone();
                window.on_next_frame(move |_, cx| chrome.update(cx, |_, cx| cx.notify()));
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window;
    }
}
