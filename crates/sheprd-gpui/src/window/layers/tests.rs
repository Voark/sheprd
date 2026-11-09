#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    group_terminals::tests::{fixture_window_on_t0, surface},
    sidebar::layout_tests::full_draw,
};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};

/// The window on tab "t0", with a frame for its terminal.
fn window_with_terminal(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = fixture_window_on_t0(cx);
    view.update(cx, |view, _| {
        let snapshot = view.live.snapshot.clone().unwrap();
        view.live.surface = Some(surface(&snapshot, "p0"));
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

fn chrome_renders(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| view.read(cx).chrome_view.read(cx).renders)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn a_terminal_redraw_replays_the_chrome(cx: &mut TestAppContext) {
    let (view, cx) = window_with_terminal(cx);
    let first = chrome_renders(&view, cx);
    assert!(first > 0);
    assert!(cx.update(|_, cx| view.read(cx).layers.shows(&Hosted::Terminal)));
    let cells = cx.debug_bounds("terminal-cells").unwrap();

    // Terminal output draws the cells again over the chrome it replays.
    for _ in 0..3 {
        view.update(cx, |view, cx| view.redraw_terminal(cx));
        draw(cx);
    }
    assert_eq!(chrome_renders(&view, cx), first);
    assert_eq!(cx.debug_bounds("terminal-cells"), Some(cells));
    // The canvas over the cells laid itself out this frame, where they are.
    let bounds = cx.update(|_, cx| view.read(cx).bounds);
    assert_eq!(bounds, cells);

    // Anything else notifies the window, which draws the chrome again.
    view.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert_eq!(chrome_renders(&view, cx), first + 1);
}

#[gpui::test]
fn the_cells_lie_inside_the_terminal_padding(cx: &mut TestAppContext) {
    let (view, cx) = window_with_terminal(cx);
    let terminal = cx.debug_bounds("terminal").unwrap();
    let cells = cx.debug_bounds("terminal-cells").unwrap();
    let padding = cx.update(|_, cx| view.read(cx).config.terminal_padding);
    assert_eq!(cells.left(), terminal.left() + px(padding.x.start()));
    assert_eq!(cells.top(), terminal.top() + px(padding.y.start()));
    assert!(cells.right() <= terminal.right());
    assert!(cells.bottom() <= terminal.bottom());
}

#[gpui::test]
fn the_cells_follow_a_resized_window(cx: &mut TestAppContext) {
    let (view, cx) = window_with_terminal(cx);
    let before = cx.debug_bounds("terminal-cells").unwrap();
    cx.simulate_resize(size(px(1200.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let after = cx.debug_bounds("terminal-cells").unwrap();
    assert!(after.size.width < before.size.width);
    assert!(after.size.height < before.size.height);
    let terminal = cx.debug_bounds("terminal").unwrap();
    assert!(after.right() <= terminal.right());
    assert_eq!(cx.update(|_, cx| view.read(cx).bounds), after);
}
