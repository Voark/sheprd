//! Dismissing a herdr-core tab's close dialog hands the keyboard back to
//! what held it, rather than to the window, whose keys a Herdr pane takes.

use super::commands::{id, run, window_with_split};
use crate::{
    SheprdWindow,
    browser::{Pick, TabId},
    controls::Command,
    menu::Page,
};
use gpui::{Entity, KeyDownEvent, Keystroke, TestAppContext, VisualTestContext};

fn ask_for_every_tab(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.config.confirm_close_tab = true;
            view.config.confirm_close_idle_tab = true;
        })
    });
}

fn asking(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: TabId) -> bool {
    cx.update(|_, cx| view.read(cx).confirming_core_tab(tab))
}

fn dialog_open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| view.read(cx).menu.page == Some(Page::ConfirmClose))
}

/// Brings `tab` to the front of its group, where the close commands act,
/// which also gives its pane in use the keyboard.
fn show(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: TabId) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.show_browser_tab(tab, window, cx)));
}

fn press(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, key: &'static str) {
    let event = KeyDownEvent {
        keystroke: Keystroke::parse(key).unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_confirmation_key(&event, window, cx)
        })
    });
}

fn pane_focused(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    tab: TabId,
    name: &str,
) -> bool {
    cx.update(|window, cx| {
        view.read(cx).core_terminals.views[&(tab, id(name))]
            .read(cx)
            .focus_handle()
            .is_focused(window)
    })
}

#[gpui::test]
fn cancelling_the_dialog_returns_the_keyboard_to_the_terminal(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a"]);
    ask_for_every_tab(&view, cx);
    show(&view, cx, tab);
    assert!(pane_focused(&view, cx, tab, "a"));
    // Escape, Enter on the selected Cancel, and the close button's dialog
    // all leave the tab open with its terminal typing again.
    for cancel in ["escape", "enter"] {
        run(&view, cx, Command::CloseTab);
        assert!(asking(&view, cx, tab));
        assert!(
            !pane_focused(&view, cx, tab, "a"),
            "the dialog has the keys"
        );
        press(&view, cx, cancel);
        assert!(!dialog_open(&view, cx));
        assert!(pane_focused(&view, cx, tab, "a"), "{cancel}");
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_strip_tab(Pick::Page(tab), window, cx)
        })
    });
    assert!(asking(&view, cx, tab));
    press(&view, cx, "escape");
    assert!(pane_focused(&view, cx, tab, "a"));
}

#[gpui::test]
fn a_pane_that_exits_under_the_dialog_hands_the_keyboard_on(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a", "b"]);
    ask_for_every_tab(&view, cx);
    show(&view, cx, tab);
    run(&view, cx, Command::FocusRight);
    assert!(pane_focused(&view, cx, tab, "b"));
    run(&view, cx, Command::CloseTab);
    assert!(asking(&view, cx, tab));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // Its program exits while the dialog is open.
            assert!(view.remove_core_pane(tab, &id("b"), window, cx));
            view.core_terminals.views.remove(&(tab, id("b")));
            view.dismiss_menu(window, cx);
        })
    });
    assert!(pane_focused(&view, cx, tab, "a"));
}
