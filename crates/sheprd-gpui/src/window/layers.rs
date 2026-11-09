//! Terminals drawn over the window's chrome rather than inside it, so a
//! terminal printing redraws only itself.
//!
//! GPUI draws a view again with every view it sits in, and the window's root
//! view draws on every frame. Drawn inside the chrome, a terminal frame would
//! rebuild the header, tab strips, status bar, and menus. Instead the chrome
//! is a cached view ([`ChromeView`]) that draws a [`Placeholder`] where each
//! terminal goes, which notes where it lands, and the root view draws the
//! chrome and then [`HostLayers`], which lays each terminal out over its
//! placeholder. A surface-only update then redraws the thin root and the
//! terminals while the chrome replays what it painted.
//!
//! A frame that draws the chrome notes every terminal's place before the
//! layers are laid out, so a terminal follows a new layout or a resized
//! window in that same frame. A frame that replays the chrome keeps the
//! places it noted: nothing in the chrome moved.
//!
//! A layer paints after all of the chrome, so what the chrome lays over a
//! terminal is [`lift`]ed to paint later still, with the window's deferred
//! draws: menus, toasts, notices, and the marks and grips over the groups.
//!
//! The terminal's focus and accessibility node, whose rows follow the
//! surface, are the cells', in the layer. macOS pages, which the chrome's
//! grounds paint around, draw the chrome again when they move; see
//! `SheprdWindow::layers_painted`.

use super::SheprdWindow;
use crate::browser::Slot;
use gpui::{prelude::*, *};
use std::{cell::RefCell, rc::Rc};

/// A terminal the chrome leaves to a layer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Hosted {
    /// The sidebar, a cached view of its own: drawn inside the chrome, every
    /// frame that draws the chrome would build its rows again, as GPUI
    /// draws every cached view inside one it draws.
    Sidebar,
    /// The window's own terminal, with its input.
    Terminal,
    /// A parked group's connection, painted without input.
    Parked(Slot),
    /// A group's picture of `tab`, which another group holds.
    Mirror { slot: Slot, tab: String },
}

/// The chrome, cached: it draws again when the window is notified, and
/// replays while only a terminal changes.
pub(crate) struct ChromeView {
    window: WeakEntity<SheprdWindow>,
    /// How many times the chrome was drawn, for tests to see it replayed.
    #[cfg(test)]
    pub(crate) renders: usize,
}

impl ChromeView {
    pub(crate) fn new(window: WeakEntity<SheprdWindow>) -> Self {
        Self {
            window,
            #[cfg(test)]
            renders: 0,
        }
    }
}

impl Render for ChromeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        self.window
            .update(cx, |this, cx| this.render_chrome(window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// The terminals the chrome showed, and where.
#[derive(Default)]
pub(crate) struct Layers {
    /// What the chrome's last drawn frame showed, in tree order.
    slots: RefCell<Vec<(Hosted, Place)>>,
    /// The view each shown terminal draws in, kept while it is shown.
    views: RefCell<Vec<(Hosted, Entity<LayerView>)>>,
}

/// A terminal's layer, as a view of its own: motion inside it (a chat
/// following its transcript, a fold opening) asks for the next frame by
/// notifying this view, which draws the layer again and not the chrome.
struct LayerView {
    window: WeakEntity<SheprdWindow>,
    hosted: Hosted,
}

impl Render for LayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hosted = self.hosted.clone();
        self.window
            .update(cx, |this, cx| this.render_hosted(&hosted, window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// Where a placeholder landed, once it has been laid out.
type Place = Rc<RefCell<Option<Spot>>>;

/// Where a placeholder landed, and what it would have drawn its terminal with.
#[derive(Clone)]
struct Spot {
    bounds: Bounds<Pixels>,
    /// What clips it: the boxes it is in.
    mask: ContentMask<Pixels>,
    text: TextStyle,
    rem: Pixels,
}

impl Layers {
    /// The chrome starts drawing a frame: what it shows is noted afresh.
    pub(crate) fn begin(&self) {
        self.slots.borrow_mut().clear();
    }

    /// Whether the chrome's last frame showed `hosted`.
    #[cfg(test)]
    pub(crate) fn shows(&self, hosted: &Hosted) -> bool {
        self.slots.borrow().iter().any(|(shown, _)| shown == hosted)
    }

    /// What the chrome draws where `hosted` goes: it fills its parent's box,
    /// and the terminal is drawn over it.
    pub(crate) fn place(&self, hosted: Hosted) -> AnyElement {
        let spot = Rc::new(RefCell::new(None));
        self.slots.borrow_mut().push((hosted, spot.clone()));
        Placeholder { spot }.into_any_element()
    }

    /// The view `hosted` draws in, made the first time it is shown.
    fn view(&self, hosted: &Hosted, window: &Entity<SheprdWindow>, cx: &mut App) -> AnyView {
        let mut views = self.views.borrow_mut();
        if let Some((_, view)) = views.iter().find(|(shown, _)| shown == hosted) {
            return view.clone().into();
        }
        let view = cx.new(|_| LayerView {
            window: window.downgrade(),
            hosted: hosted.clone(),
        });
        views.push((hosted.clone(), view.clone()));
        view.into()
    }

    /// Lets go of the views of terminals the chrome no longer shows.
    fn forget_hidden(&self) {
        let slots = self.slots.borrow();
        self.views
            .borrow_mut()
            .retain(|(hosted, _)| slots.iter().any(|(shown, _)| shown == hosted));
    }

    /// The layers, for the root to draw after the chrome.
    pub(crate) fn element(self: &Rc<Self>, window: Entity<SheprdWindow>) -> HostLayers {
        HostLayers {
            layers: self.clone(),
            window,
        }
    }
}

/// Fills its parent's box, as the terminal drawn there does, and notes where
/// it landed.
struct Placeholder {
    spot: Rc<RefCell<Option<Spot>>>,
}

impl IntoElement for Placeholder {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Placeholder {
    type PrepaintState = ();
    type RequestLayoutState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        *self.spot.borrow_mut() = Some(Spot {
            bounds,
            mask: window.content_mask(),
            text: window.text_style(),
            rem: window.rem_size(),
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

/// The terminals the chrome showed, each laid out over its placeholder. It
/// takes no room of its own; it goes after the chrome.
pub(crate) struct HostLayers {
    layers: Rc<Layers>,
    window: Entity<SheprdWindow>,
}

/// A terminal laid out, and where.
pub(crate) struct Drawn {
    element: AnyElement,
    spot: Spot,
}

impl IntoElement for HostLayers {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for HostLayers {
    type PrepaintState = Vec<Drawn>;
    type RequestLayoutState = ();

    fn id(&self) -> Option<ElementId> {
        Some("terminal-layers".into())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    /// After the chrome's prepaint, which noted where each terminal goes this
    /// frame (or kept where it went, replayed). The terminals are drawn here
    /// rather than in the root's render, so they follow what the chrome
    /// drew this frame.
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Drawn> {
        let slots = self.layers.slots.borrow().clone();
        let mut drawn = Vec::with_capacity(slots.len());
        for (hosted, spot) in slots {
            // Not laid out: under a group that is closing, say.
            let Some(spot) = spot.borrow().clone() else {
                continue;
            };
            let mut element = self
                .layers
                .view(&hosted, &self.window, cx)
                .into_any_element();
            within(&spot, window, |window| {
                let room = spot.bounds.size.map(AvailableSpace::Definite);
                element.layout_as_root(room, window, cx);
                element.prepaint_at(spot.bounds.origin, window, cx);
            });
            drawn.push(Drawn { element, spot });
        }
        self.layers.forget_hidden();
        drawn
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        drawn: &mut Vec<Drawn>,
        window: &mut Window,
        cx: &mut App,
    ) {
        for Drawn { element, spot } in drawn {
            within(spot, window, |window| element.paint(window, cx));
        }
        self.window
            .update(cx, |this, _| this.layers_painted(window));
    }
}

/// Runs `f` as if at the placeholder of `spot`: clipped by what clips it,
/// with its text style and rem size, which the terminal would have inherited.
fn within<R>(spot: &Spot, window: &mut Window, f: impl FnOnce(&mut Window) -> R) -> R {
    window.with_rem_size(Some(spot.rem), |window| {
        window.with_text_style(Some(refinement(&spot.text)), |window| {
            window.with_content_mask(Some(spot.mask), f)
        })
    })
}

/// `style` as a refinement that sets every field, so that it is the text
/// style whatever is set around it.
fn refinement(style: &TextStyle) -> TextStyleRefinement {
    let style = style.clone();
    TextStyleRefinement {
        color: Some(style.color),
        font_family: Some(style.font_family),
        font_features: Some(style.font_features),
        font_fallbacks: style.font_fallbacks,
        font_size: Some(style.font_size),
        line_height: Some(style.line_height),
        font_weight: Some(style.font_weight),
        font_style: Some(style.font_style),
        background_color: style.background_color,
        underline: style.underline,
        strikethrough: style.strikethrough,
        white_space: Some(style.white_space),
        text_overflow: style.text_overflow,
        text_align: Some(style.text_align),
        line_clamp: style.line_clamp,
    }
}

/// Draws the view being drawn again, for what a draw measured that changes
/// how it lays out: on the next frame, and on any draw that comes first.
/// `Window::refresh` does nothing while the window draws, which went unseen
/// while the whole window drew on every frame; the cached chrome does not.
pub(crate) fn draw_again(window: &Window, cx: &mut App) {
    let view = window.current_view();
    cx.notify(view);
    window.on_next_frame(move |_, cx| cx.notify(view));
}

/// `element`, laid out where it is but painted with the window's deferred
/// draws, over the terminal layers: what the chrome lays over a terminal.
/// It keeps what clips it and its place among the keyboard's ancestors.
pub(crate) fn lift(element: impl IntoElement) -> AnyElement {
    Lifted {
        child: Some(element.into_any_element()),
    }
    .into_any_element()
}

struct Lifted {
    child: Option<AnyElement>,
}

impl IntoElement for Lifted {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Lifted {
    type PrepaintState = ();
    type RequestLayoutState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let layout = match self.child.as_mut() {
            Some(child) => child.request_layout(window, cx),
            None => window.request_layout(Style::default(), None, cx),
        };
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        if let Some(child) = self.child.take() {
            let mask = window.content_mask();
            window.defer_draw(child, window.element_offset(), 0, Some(mask));
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

#[cfg(test)]
mod tests;
