//! A herdr-core tab's close button, middle-click, menu, and the close
//! command ask first while `confirm_close_idle_tab` has every tab ask, and
//! end nothing until the dialog is confirmed.

use super::commands::{id, run, window_with_split};
use crate::{
    SheprdWindow,
    browser::{Location, Pick, Store, TabId},
    controls::Command,
    core_terminals::{
        Host,
        menu::{Action, Target},
        panes::Panes,
    },
    menu::Page,
};
use gpui::{Entity, KeyDownEvent, Keystroke, TestAppContext, VisualTestContext, point, px};

fn asking(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| view.read(cx).menu.page == Some(Page::ConfirmClose))
}

fn open(tab: TabId, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| cx.global::<Store>().get(tab).is_some())
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

fn set_options(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: bool, idle: bool) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.config.confirm_close_tab = tab;
            view.config.confirm_close_idle_tab = idle;
        })
    });
}

/// What the tab's close button and a middle-click on it both run.
fn click_close(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: TabId) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_strip_tab(Pick::Page(tab), window, cx)
        })
    });
}

#[gpui::test]
fn a_core_tab_closes_at_once_unless_every_tab_asks(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a"]);
    click_close(&view, cx, tab);
    assert!(!asking(&view, cx));
    assert!(!open(tab, cx));
    let ending = cx.update(|_, cx| Store::update(cx, Store::take_ending));
    assert_eq!(ending, vec![(Host::Local, id("a"))]);
}

#[gpui::test]
fn turning_tab_confirmation_off_wins_over_the_idle_option(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a"]);
    set_options(&view, cx, false, true);
    click_close(&view, cx, tab);
    assert!(!asking(&view, cx));
    assert!(!open(tab, cx));
}

#[gpui::test]
fn the_close_button_asks_and_ends_nothing_until_confirmed(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a"]);
    set_options(&view, cx, true, true);
    click_close(&view, cx, tab);
    assert!(asking(&view, cx));
    assert!(open(tab, cx));
    assert!(cx.update(|_, cx| Store::update(cx, Store::take_ending).is_empty()));
    // Cancel is selected: Enter alone keeps the tab and its terminal.
    press(&view, cx, "enter");
    assert!(!asking(&view, cx));
    assert!(open(tab, cx));
    assert!(cx.update(|_, cx| Store::update(cx, Store::take_ending).is_empty()));

    click_close(&view, cx, tab);
    press(&view, cx, "tab");
    press(&view, cx, "enter");
    assert!(!asking(&view, cx));
    assert!(!open(tab, cx));
    let ending = cx.update(|_, cx| Store::update(cx, Store::take_ending));
    assert_eq!(ending, vec![(Host::Local, id("a"))]);
}

#[gpui::test]
fn the_close_commands_and_the_tab_menu_ask_too(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["a"]);
    set_options(&view, cx, true, true);
    // With one pane, closing the pane closes the tab.
    for command in [Command::CloseTab, Command::ClosePane] {
        run(&view, cx, command);
        assert!(asking(&view, cx), "{command:?}");
        assert!(open(tab, cx), "{command:?}");
        press(&view, cx, "escape");
        assert!(!asking(&view, cx));
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_core_menu(Target::Tab(tab), point(px(10.), px(10.)), window, cx);
            view.activate_core_menu(Action::Close, window, cx);
        })
    });
    assert!(asking(&view, cx));
    assert!(open(tab, cx));
}

#[gpui::test]
fn a_mirror_and_a_page_still_close_at_once(cx: &mut TestAppContext) {
    let (view, cx, _) = window_with_split(cx, &["a"]);
    set_options(&view, cx, true, true);
    let (mirror, page) = cx.update(|_, cx| {
        let (scope, workspace) = view.read(cx).browser_key().unwrap();
        // A mirror only watches `a`: closing it ends nothing.
        let watching = Location::core_terminal(Panes::single(id("a")), true);
        Store::update(cx, |store| {
            (
                store
                    .open(scope.clone(), &workspace, Some(watching), None)
                    .unwrap(),
                store.open(scope, &workspace, None, None).unwrap(),
            )
        })
    });
    for tab in [mirror, page] {
        click_close(&view, cx, tab);
        assert!(!asking(&view, cx));
        assert!(!open(tab, cx));
    }
    assert!(cx.update(|_, cx| Store::update(cx, Store::take_ending).is_empty()));
}
