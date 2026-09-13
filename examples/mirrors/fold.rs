//! Mirror folding.
//!
//! There is no state to keep. Where the mirror is, is `world.fold_position` —
//! reported by the motor, absorbed before any rule runs — and `AtFolded` /
//! `AtUnfolded` read it. A mirror halfway through its travel satisfies neither,
//! so no rule fires at it, which is how a power change cannot reverse a motor
//! that is already moving.

use declared::prelude::*;

use crate::guards::{AtFolded, AtUnfolded, PowerOff, PowerOn, SpeedAllowsFold, SpeedForcesUnfold};
use crate::{Event, Kind, Mirrors, World};

/// Folding's effects. Its own type, like every other feature's.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Fold,
    Unfold,
}

// Only needed by `verify::unemitted_actions`; `Feature` does not require it.
impl declared::Enumerable for Action {
    const ALL: &'static [Self] = &[Self::Fold, Self::Unfold];
}

pub struct Fold;

impl Feature for Fold {
    type Domain = Mirrors;
    type Action = Action;

    const NAME: &'static str = "Fold";

    const RULES: &'static [Rule<Mirrors, Action>] = &[
        Rule {
            id: "FOLD_ON_POWER_OFF",
            when: &[Kind::PowerChanged],
            check: declared::check!(PowerOff && SpeedAllowsFold && AtUnfolded),
            unknown: OnUnknown::Deny,
            emit: &[Action::Fold],
        },
        Rule {
            id: "UNFOLD_ON_POWER_ON",
            when: &[Kind::PowerChanged],
            check: declared::check!(PowerOn && AtFolded),
            unknown: OnUnknown::Deny,
            emit: &[Action::Unfold],
        },
        Rule {
            id: "UNFOLD_ON_SPEED",
            when: &[Kind::SpeedChanged],
            check: declared::check!(SpeedForcesUnfold && AtFolded),
            unknown: OnUnknown::Deny,
            emit: &[Action::Unfold],
        },
    ];

    fn perform(action: Action, _ev: &Event, world: &mut World) {
        match action {
            Action::Fold => log::debug!("fold (speed {:.0})", world.speed),
            Action::Unfold => log::debug!("unfold"),
        }
    }
}
