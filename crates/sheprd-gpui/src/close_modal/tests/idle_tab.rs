use super::*;

/// A window over the fixture snapshot with nothing mid-task in its first tab.
fn idle_fixture(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        for tab in &mut snapshot.tabs {
            tab.agent_status = AgentStatus::Idle;
        }
        for agent in &mut snapshot.agents {
            agent.agent_status = AgentStatus::Done;
        }
        view.live.snapshot = Some(Arc::new(snapshot));
        view
    })
}

/// Whether the close went out without asking: the disconnected fixture
/// attempts an immediate close and refuses it, which leaves an error.
fn closed_at_once(view: &SheprdWindow) -> bool {
    view.menu.close.as_ref().unwrap().error.is_some()
}

#[gpui::test]
fn an_idle_tab_asks_only_with_confirm_close_idle_tab(cx: &mut TestAppContext) {
    let (view, cx) = idle_fixture(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let id = view.live.snapshot.as_ref().unwrap().tabs[0].tab_id.clone();
            assert!(view.config.confirm_close_tab);
            assert!(!view.config.confirm_close_idle_tab);
            // By default the close button and the command close it at once.
            view.open_tab_close(&id, window, cx);
            assert!(closed_at_once(view));
            view.dismiss_menu(window, cx);
            view.open_close_confirmation(Command::CloseTab, window, cx);
            assert!(closed_at_once(view));
            view.dismiss_menu(window, cx);

            view.config.confirm_close_idle_tab = true;
            view.open_tab_close(&id, window, cx);
            assert!(!closed_at_once(view));
            assert_eq!(view.menu.page, Some(Page::ConfirmClose));
            // Nothing is mid-task, so the dialog names no agent.
            assert_eq!(view.menu.close.as_ref().unwrap().interrupts, None);
            view.dismiss_menu(window, cx);
            view.open_close_confirmation(Command::CloseTab, window, cx);
            assert!(!closed_at_once(view));
            view.dismiss_menu(window, cx);

            // Turning tab confirmation off wins over the idle option.
            view.config.confirm_close_tab = false;
            view.open_tab_close(&id, window, cx);
            assert!(closed_at_once(view));
        })
    });
}
