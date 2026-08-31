//! Mirror dimming. A function of the current power state and gear, so it keeps
//! no state either.
//!
//! Both rules cover both kinds: whichever of the two signals changed, the answer
//! is read from the world, which the caller has already updated. That is why the
//! condition is written once rather than once per event.

use chart::feature::{Feature, Rule};
use chart::guard::OnUnknown;

use crate::guards::{GearReverse, PowerOn};
use crate::{Event, Kind, Mirrors, World};

/// Dimming's effects.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    On,
    Off,
}

// Only needed by `verify::unemitted_actions`; `Feature` does not require it.
impl chart::Enumerable for Action {
    const ALL: &'static [Self] = &[Self::On, Self::Off];
}

pub struct Dimming;

/// The two signals dimming is a function of.
const INPUTS: &[Kind] = &[Kind::PowerChanged, Kind::GearChanged];

impl Feature for Dimming {
    type Domain = Mirrors;
    type Action = Action;

    const NAME: &'static str = "Dimming";

    const RULES: &'static [Rule<Mirrors, Action>] = &[
        Rule {
            when: INPUTS,
            check: chart::check!(PowerOn && !GearReverse),
            unknown: OnUnknown::Deny,
            emit: &[Action::On],
        },
        Rule {
            when: INPUTS,
            check: chart::check!(),
            unknown: OnUnknown::Deny,
            emit: &[Action::Off],
        },
    ];

    fn perform(action: Action, _ev: &Event, _world: &mut World) {
        match action {
            Action::On => log::debug!("dimming on"),
            Action::Off => log::debug!("dimming off"),
        }
    }
}
