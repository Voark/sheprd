//! The composer's agent button: while a chat may still switch agents
//! (`sheprd-acp --choose`, until its first prompt), it names the agent
//! running and opens a menu of the others installed where the chat runs.

use super::super::super::chats::{ChatPane, Menu};
use crate::{SheprdWindow, i18n::fl, icons::AgentIcon};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{Agent, Command, Transcript};

/// The agent's mark on the button and in the menu.
const MARK: f32 = 12.;

/// The agents the chat could switch to, the running one among them, when
/// there is more than one to choose from.
pub(super) fn offered(transcript: &Transcript) -> Option<(&[Agent], Option<Agent>)> {
    let harnesses = transcript.harnesses.as_ref()?;
    (harnesses.available.len() > 1).then_some((harnesses.available.as_slice(), harnesses.current))
}

impl SheprdWindow {
    /// The button that opens the agent menu, while the chat offers one.
    pub(super) fn render_chat_agent_button(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let transcript = chat.session.transcript();
        offered(transcript)?;
        let name = transcript.agent.clone();
        let pane = chat.target.pane_id.clone();
        Some(
            self.chat_header_button("chat-agent-picker".into())
                .tooltip(self.chat_setting_hint(&fl!("chat-settings-agent"), &name))
                .child(
                    AgentIcon::from_identity(Some(chat.session.agent()))
                        .element(MARK, self.theme.foreground),
                )
                .child(
                    div()
                        .max_w(px(160.))
                        .truncate()
                        .child(crate::notifications::safe_text(&name, 80)),
                )
                .child(
                    svg()
                        .path("icons/chevron-down.svg")
                        .flex_none()
                        .size(px(10.))
                        .text_color(rgb(self.theme.subtext())),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.toggle_chat_menu(&pane, Menu::Agent, window, cx);
                })),
        )
    }

    /// The agent menu's rows, one per agent offered, shaped by `row`.
    pub(super) fn chat_agent_rows(
        &self,
        chat: &ChatPane,
        row: impl Fn(usize, bool) -> Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some((agents, current)) = offered(chat.session.transcript()) else {
            return Vec::new();
        };
        let pane_id = chat.target.pane_id.clone();
        agents
            .iter()
            .enumerate()
            .map(|(index, &agent)| {
                let pane = pane_id.clone();
                row(index, current == Some(agent))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                AgentIcon::from_identity(Some(agent.name()))
                                    .element(MARK, self.theme.foreground),
                            )
                            .child(agent.display_name()),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.toggle_chat_menu(&pane, Menu::Agent, window, cx);
                        this.choose_chat_agent(&pane, agent, cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// Switches the chat in `pane_id` to `agent`, which the launcher then
    /// starts the next chat with too.
    pub(in crate::chat) fn choose_chat_agent(
        &mut self,
        pane_id: &str,
        agent: Agent,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .chats
            .pane_mut(pane_id)
            .and_then(|pane| offered(pane.session.transcript()).and_then(|(_, current)| current));
        if current == Some(agent) {
            return;
        }
        self.chats.last_started = Some(agent);
        self.chat_send(pane_id, Command::SetAgent { agent }, cx);
    }
}
