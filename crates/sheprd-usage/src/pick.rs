//! Which of several agents a new chat should start with, judged by how much
//! room each one's plan has left.
//!
//! The person's preferred agent stays the choice while its plan has room,
//! or while nothing is known of it: usage is a hint, read every few minutes,
//! and the agent's own refusal at its limit is what really stops it. Only a
//! preferred plan that is spent, or close enough to its limit to run out
//! soon, gives way to the candidate with the most room.

use std::time::{Duration, SystemTime};

use crate::model::Report;

/// The share of a window past which a plan counts as tight.
const TIGHT: f32 = 80.;
/// A window on pace to run out before it resets counts as tight from here,
/// so an early burst in a fresh window does not.
const RUNNING_OUT: f32 = 50.;

/// How much room a plan has, from its tightest window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Room {
    /// Under its limits and not on pace to reach them.
    Ample { used: f32 },
    /// No reading: not signed in, not read yet, or a plan without windows.
    Unknown,
    /// Close to a limit, or on pace to reach one before it resets.
    Tight {
        used: f32,
        resets_in: Option<Duration>,
        /// When it runs out at the rate used so far, if before it resets.
        runs_out: Option<Duration>,
    },
    /// A limit is reached.
    Spent { resets_in: Option<Duration> },
}

impl Room {
    /// The room `report` shows at `now`; `None` reads as unknown.
    pub fn of(report: Option<&Report>, now: SystemTime) -> Self {
        let Some(window) = report.and_then(Report::tightest) else {
            return Self::Unknown;
        };
        let resets_in = window.resets_in(now);
        if window.used >= 100. {
            return Self::Spent { resets_in };
        }
        let runs_out = window.pace(now).and_then(|pace| pace.runs_out);
        if window.used >= TIGHT || (runs_out.is_some() && window.used >= RUNNING_OUT) {
            return Self::Tight {
                used: window.used,
                resets_in,
                runs_out,
            };
        }
        Self::Ample { used: window.used }
    }

    /// Whether the preferred agent should give way to another.
    pub fn short(&self) -> bool {
        matches!(self, Self::Tight { .. } | Self::Spent { .. })
    }

    /// Orders candidates, most room first: ample plans by how little they
    /// used, then unknown ones, then tight ones by how little they used,
    /// then spent ones.
    fn rank(&self) -> (u8, f32) {
        match self {
            Self::Ample { used } => (0, *used),
            Self::Unknown => (1, 0.),
            Self::Tight { used, .. } => (2, *used),
            Self::Spent { .. } => (3, 0.),
        }
    }
}

/// The agent to start with, and the preferred one it replaces, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick<K> {
    pub chosen: K,
    /// The preferred agent and its room, when its plan was too short to
    /// keep it.
    pub instead_of: Option<(K, Room)>,
}

/// Chooses among `candidates`, each an agent and its plan's room, keeping
/// `preferred` unless its plan is short and another's is not. A
/// preferred agent that is not a candidate, as one no longer installed,
/// leaves the choice to room alone. `None` when there are no candidates.
pub fn pick<K: Copy + Eq>(candidates: &[(K, Room)], preferred: Option<K>) -> Option<Pick<K>> {
    let best = candidates
        .iter()
        .min_by(|a, b| {
            let (a, b) = (a.1.rank(), b.1.rank());
            a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
        })
        .copied()?;
    let Some((preferred, room)) = preferred
        .and_then(|preferred| candidates.iter().find(|(key, _)| *key == preferred))
        .copied()
    else {
        return Some(Pick {
            chosen: best.0,
            instead_of: None,
        });
    };
    // Another plan only wins by being in a better class: one tight plan
    // never replaces another over a few percent.
    if !room.short() || best.0 == preferred || best.1.rank().0 >= room.rank().0 {
        return Some(Pick {
            chosen: preferred,
            instead_of: None,
        });
    }
    Some(Pick {
        chosen: best.0,
        instead_of: Some((preferred, room)),
    })
}

#[cfg(test)]
mod tests;
