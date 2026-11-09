//! The terminals the chrome leaves to layers (see `super::super::layers`):
//! the window's own grid with what lies over its cells, and the frames of
//! parked and mirrored groups. Built on every frame the root draws, from the
//! latest surface, while the chrome replays.

use super::super::{PressedLink, SheprdWindow, layers::Hosted};
use crate::{
    config::ClipboardToastPosition,
    terminal::*,
    terminal_painter::{self, Ground, ImageTarget, PlacedImages},
};
use gpui::{prelude::*, *};
use sheprd_client::ConnectOptions;

impl SheprdWindow {
    /// What goes over the placeholder the chrome drew for `hosted`.
    pub(in crate::window) fn render_hosted(
        &mut self,
        hosted: &Hosted,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match hosted {
            Hosted::Sidebar => {
                let width = self.sidebar_column_width(self.viewport_width).unwrap_or(0.);
                crate::sidebar::cached_view(&self.sidebar_view, width).into_any_element()
            }
            Hosted::Terminal => self.render_terminal_cells(window, cx),
            Hosted::Parked(slot) => {
                let surface = self.parked_frame(slot.id);
                self.render_terminal_picture(*slot, surface, true, cx)
            }
            Hosted::Mirror { slot, tab } => {
                let window_frame = self.presentation.picture(&self.live);
                let surface = self.live_frame_of(tab, window_frame);
                self.render_terminal_picture(*slot, surface, false, cx)
            }
        }
    }

    /// The keys and actions of the terminal, on the terminal's box in the
    /// chrome and again on its cells in the layer, so what lies over the
    /// cells (the find field, a chat's composer, a note) hands keys on to
    /// the terminal as it did when it sat inside the terminal's box.
    pub(in crate::window) fn terminal_keys<E: InteractiveElement + FluentBuilder>(
        &self,
        element: E,
        cx: &mut Context<Self>,
    ) -> E {
        element
            // Holding the link modifier over a link in a mouse-reporting
            // application changes what a click does, so the pointer follows.
            .on_modifiers_changed(
                cx.listener(|this, event: &ModifiersChangedEvent, window, cx| {
                    this.hover_link(window.mouse_position(), event.modifiers, cx);
                    let hovered =
                        this.terminal_link_hovered(window.mouse_position(), event.modifiers);
                    if hovered != this.hovered_terminal_link {
                        this.hovered_terminal_link = hovered;
                        cx.notify();
                    }
                }),
            )
            .capture_key_down(cx.listener(Self::chat_zoom_key))
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            // The terminal has nothing for Cut to act on, and Copy only while
            // a released selection is still highlighted.
            .on_action(cx.listener(|this, _: &crate::actions::Paste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &crate::actions::SelectAll, _, cx| {
                this.select_all(cx);
            }))
            .when(self.selection_retained(), |element| {
                element.on_action(cx.listener(|this, _: &crate::actions::Copy, _, cx| {
                    this.copy_retained_selection(cx);
                }))
            })
    }

    /// The window's terminal grid: the cached regions, the canvas that takes
    /// input and paints what changes without them, and what lies over the
    /// cells. It fills the placeholder inside the terminal's padding.
    fn render_terminal_cells(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let font = self.config.terminal.font();
        let cell_height = self.config.terminal.line_height();
        let ground = self.ground();
        // Paints the frame on screen, which during a focus change is the one
        // presented before it: the terminal area never blanks between two
        // projections. What the client knows to be current stays in `live`.
        let picture = self.presentation.picture(&self.live);
        let surface = picture.as_ref().map(|picture| picture.frame.clone());
        let images = picture.as_ref().map(|picture| picture.images.clone());
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let focus = self.focus.clone();
        let menu_open = self.menu.page.is_some();
        let cell_width = self.cell_width;
        let painter = self.painter.clone();
        // The highlight is grid coordinates, so it paints with the frame that
        // owns the cells rather than being recomputed from the pointer here.
        let selection_rows = |owned: bool| -> Vec<_> {
            surface
                .as_deref()
                .zip(self.selection.as_ref().filter(|_| owned))
                .into_iter()
                .flat_map(|(surface, selection)| selection.rows(surface, cell_width, cell_height))
                .map(|(row, columns)| terminal_painter::Highlight {
                    row,
                    columns,
                    tint: terminal_painter::Tint::Selection,
                })
                .collect()
        };
        // The highlight belongs to the frame that owns the cells, so only one
        // of the panes and the popup paints it.
        let popup_highlights = surface
            .as_deref()
            .and_then(|surface| surface.popup.as_ref())
            .map(|popup| {
                selection_rows(
                    self.selection
                        .as_ref()
                        .is_some_and(|selection| selection.in_popup(&popup.terminal_id)),
                )
            })
            .unwrap_or_default();
        let pane_selection = selection_rows(
            self.selection
                .as_ref()
                .is_some_and(|selection| selection.in_panes()),
        );
        // Like the highlight, the link underline paints with the frame that
        // owns its cells, and only while that frame shows the content the
        // daemon resolved it from. Without one, the local reading under the
        // pointer is underlined instead, on every row it covers.
        let link_rows: Vec<_> = surface
            .as_deref()
            .zip(self.hovered_daemon_link())
            .filter(|(surface, link)| link.cell.current(surface))
            .map(|(_, link)| link.frame_rows().collect())
            .or_else(|| Some(self.hovered_local_link()?.rows().collect()))
            .unwrap_or_default();
        // Search matches, mapped onto the frame on screen, tint below the
        // selection, which reads as chosen over them. A popup covers the
        // panes, so their matches stay under it.
        let mut highlights = surface
            .as_deref()
            .map(|surface| {
                let mut highlights = self.find_highlights(surface);
                highlights.extend(self.copy_mode_highlights(surface));
                highlights
            })
            .unwrap_or_default();
        highlights.extend(pane_selection);
        let look = super::super::regions::Look {
            font: font.clone(),
            font_size: self.config.terminal.size,
            cell_width,
            cell_height,
            theme: self.terminal_theme().clone(),
            ground,
        };
        if !self.slides_allowed() {
            self.presentation.scroll.clear();
        }
        let now = crate::motion::now();
        let slide = self.presentation.scroll.slide(now);
        // A link in a pane drawn mid-row underlines where its text is drawn.
        let link_shift = link_rows.first().and_then(|(row, columns)| {
            self.shift_at(point(
                px(f32::from(columns.start) * cell_width),
                px(f32::from(*row) * cell_height),
            ))
        });
        if self.presentation.scroll.moving(now) {
            self.redraw_terminal_next_frame(window);
        }
        let regions = self.terminal_regions(surface.as_ref(), &highlights, slide, &look, cx);
        // Without regions the canvas paints the whole grid, images included.
        let whole = regions.is_empty();
        // A chat's page panel lies beneath the window's drawing on macOS,
        // over the cells of the chat's pane, so the cells paint around it.
        #[cfg(target_os = "macos")]
        let drawn_pages = self.browser.drawn_pages();
        // The IME composition paints inline at the input cursor; a menu's
        // text field shows its own.
        // It anchors to the live surface, as the IME's candidate window does,
        // so a retained frame never separates the text from the window.
        let marked = (!menu_open && !self.marked.is_empty()).then(|| {
            (
                self.marked.clone(),
                self.live.surface.clone(),
                self.ime_shift(),
            )
        });
        let find_bar = self.render_find_bar(surface.as_deref(), cx);
        let terminal_note = self.render_terminal_note(surface.as_deref(), cx);
        let copy_badge = self.render_copy_mode_badge(surface.as_deref(), cx);
        let chats = self.render_chats(surface.as_deref(), cx);
        // The cached regions paint the cells; the canvas above them takes
        // input and paints what changes without them. Both fill this box,
        // so every region shares the canvas's bounds. What is laid over the
        // cells sits in the same box, inside the terminal's padding. A frame
        // the daemon has not resized yet may be larger than the box, so the
        // box clips it and the padding stays clear.
        let cells = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .children(regions)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _| {
                            this.bounds = bounds;
                            this.options = ConnectOptions {
                                surface_size: viewport(
                                    bounds.size.width.to_f64() as f32,
                                    bounds.size.height.to_f64() as f32,
                                    cell_width,
                                    cell_height,
                                ),
                                cell_width_px: cell_width.round().max(1.) as u32,
                                cell_height_px: cell_height.round().max(1.) as u32,
                            };
                            this.resize();
                        });
                    },
                    move |bounds, _, window, cx| {
                        // Capture movement outside the terminal too, before any
                        // element can stop propagation of a drag-away event.
                        let entity = paint_entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                entity.update(cx, |this, cx| {
                                    // Window-wide, so leaving the terminal
                                    // drops the underline too.
                                    this.hover_link(event.position, event.modifiers, cx);
                                    if this.scrollbar_mouse_move(event, cx)
                                        || this.split_mouse_move(event, cx)
                                        || this.terminal_mouse_move(event, cx)
                                    {
                                        cx.stop_propagation();
                                        return;
                                    }
                                    if this.pressed_terminal_link.as_ref().is_some_and(
                                        |PressedLink { position, .. }| {
                                            (event.position.x - position.x).abs() > px(4.)
                                                || (event.position.y - position.y).abs() > px(4.)
                                        },
                                    ) {
                                        this.pressed_terminal_link = None;
                                    }
                                    // A drag that leaves the terminal keeps
                                    // selecting, and hover work elsewhere stays
                                    // out of the gesture.
                                    if this.extend_selection(event.position, cx) {
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        let released = paint_entity.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                released.update(cx, |this, cx| {
                                    // Global: the overlay occludes the opener, and release
                                    // may also precede the overlay's first frame.
                                    if matches!(
                                        event.button,
                                        MouseButton::Left | MouseButton::Right
                                    ) {
                                        this.menu.opening_right_click = false;
                                    }
                                    if this.scrollbar_mouse_up(event, cx)
                                        || this.split_mouse_up(event, cx)
                                        || this.terminal_mouse_up(event, cx)
                                        || (event.button == MouseButton::Left
                                            && !cx.has_active_drag()
                                            && this.release_selection(cx))
                                    {
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        window.handle_input(
                            &focus,
                            crate::input::TerminalInputHandler::new(
                                bounds,
                                paint_entity.clone(),
                                menu_open,
                            ),
                            cx,
                        );
                        if let Some(surface) = &surface {
                            if whole {
                                let paint = |window: &mut Window, cx: &mut App| {
                                    painter.borrow_mut().paint_frame(
                                        &surface.frame,
                                        bounds.origin,
                                        Some(bounds.size),
                                        ground,
                                        cell_width,
                                        &font,
                                        &highlights,
                                        &surface.panes,
                                        None,
                                        images.as_deref().map(|images| PlacedImages {
                                            placements: &surface.graphics.placements,
                                            images,
                                            target: ImageTarget::Main,
                                        }),
                                        window,
                                        cx,
                                    );
                                };
                                #[cfg(target_os = "macos")]
                                {
                                    let pages: Vec<Bounds<Pixels>> = drawn_pages
                                        .borrow()
                                        .values()
                                        .copied()
                                        .filter(|page| page.intersects(&bounds))
                                        .collect();
                                    if pages.is_empty() {
                                        paint(window, cx);
                                    } else {
                                        for piece in crate::browser::around_pages(bounds, pages) {
                                            window.with_content_mask(
                                                Some(ContentMask { bounds: piece }),
                                                |window| paint(window, cx),
                                            );
                                        }
                                    }
                                }
                                #[cfg(not(target_os = "macos"))]
                                paint(window, cx);
                            } else {
                                // Regions place no images, so nothing
                                // else releases the ones that went away.
                                painter.borrow_mut().release_idle_images(window);
                            }
                            let paint_link = |window: &mut Window, shift| {
                                painter.borrow().paint_link(
                                    &surface.frame,
                                    bounds.origin + point(px(0.), shift),
                                    cell_width,
                                    &link_rows,
                                    window,
                                )
                            };
                            match link_shift {
                                Some((pane, shift)) => window.with_content_mask(
                                    Some(ContentMask {
                                        bounds: Bounds::new(bounds.origin + pane.origin, pane.size),
                                    }),
                                    |window| paint_link(window, shift),
                                ),
                                None => paint_link(window, px(0.)),
                            }
                            if let Some(popup) = &surface.popup {
                                let offset = popup_origin(
                                    &surface.frame,
                                    &popup.frame,
                                    cell_width,
                                    cell_height,
                                );
                                painter.borrow_mut().paint_frame(
                                    &popup.frame,
                                    bounds.origin + offset,
                                    None,
                                    Ground::Opaque,
                                    cell_width,
                                    &font,
                                    &popup_highlights,
                                    &[],
                                    None,
                                    images.as_deref().map(|images| PlacedImages {
                                        placements: &surface.graphics.placements,
                                        images,
                                        target: ImageTarget::Popup(&popup.terminal_id),
                                    }),
                                    window,
                                    cx,
                                );
                            }
                        }
                        if let Some((marked, live, shift)) = &marked {
                            let live = live.as_deref();
                            painter.borrow().paint_composition(
                                marked,
                                input_cursor_bounds(live, bounds.origin, cell_width, cell_height)
                                    .origin
                                    + point(px(0.), *shift),
                                input_area(live, bounds, cell_width, cell_height),
                                &font,
                                window,
                            );
                        }
                    },
                )
                .size_full(),
            );
        let cells = cells
            .children(chats)
            .children(self.render_reconnecting(cx))
            .when_some(find_bar, |terminal, bar| terminal.child(bar))
            .when_some(terminal_note, |terminal, note| terminal.child(note))
            .when_some(copy_badge, |terminal, badge| terminal.child(badge))
            // Direct feedback for the user's own gesture, not a daemon notice:
            // it sits over the cells it copied and needs no dismissing.
            .when_some(self.flash_shown(), |terminal, (flash, shown)| {
                use ClipboardToastPosition::*;
                let position = self.config.clipboard_toast.position;
                // It comes in from the edge it sits by and goes back to it.
                let edge = px(12. - (1. - shown) * super::super::flash::SLIDE);
                terminal.child(
                    div()
                        .absolute()
                        .opacity(shown)
                        .map(|row| match position {
                            TopLeft | TopCenter | TopRight => row.top(edge),
                            BottomLeft | BottomCenter | BottomRight => row.bottom(edge),
                        })
                        .map(|row| match position {
                            TopLeft | BottomLeft => row.justify_start(),
                            TopCenter | BottomCenter => row.justify_center(),
                            TopRight | BottomRight => row.justify_end(),
                        })
                        // The flash spans the cells, so centering centers on
                        // them and a corner is the corner of the grid.
                        .left_0()
                        .right_0()
                        .px(px(12.))
                        .flex()
                        .overflow_hidden()
                        .child(
                            div()
                                .debug_selector(|| "flash".into())
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .px(px(12.))
                                .py(px(6.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .border_1()
                                .border_color(rgb(flash.accent(&self.theme)))
                                .bg(rgb(self.theme.surface))
                                .text_color(rgb(self.theme.foreground))
                                .child(
                                    div()
                                        .size(px(6.))
                                        .flex_none()
                                        .rounded_full()
                                        .bg(rgb(flash.accent(&self.theme))),
                                )
                                .child(div().truncate().child(flash.text.clone())),
                        ),
                )
            });
        cells
            .id("terminal-cells")
            .debug_selector(|| "terminal-cells".into())
            .when(self.hovered_terminal_link, |cells| cells.cursor_pointer())
            .when_some(self.split_cursor, |cells, cursor| cells.cursor(cursor))
            .map(|cells| self.terminal_keys(cells, cx))
            .into_any_element()
    }
}
