//! Mirror heating. Follows the defog signal, so it keeps no state.
//!
//! Its effects, its rules and its execution are all here; nothing about heating
//! is declared anywhere else.

use chart::feature::{Feature, Rule};
use chart::guard::OnUnknown;

use crate::guards::DefogOn;
use crate::{Event, Kind, Mirrors, World};

/// Heating's effects. Its own type, so `perform` below is exhaustive over
/// exactly these and adding one is a compile error in this file.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    On,
    Off,
}

// Only needed by `verify::unemitted_actions`; `Feature` does not require it.
impl chart::Enumerable for Action {
    const ALL: &'static [Self] = &[Self::On, Self::Off];
}

pub struct Heating;

impl Feature for Heating {
    type Domain = Mirrors;
    type Action = Action;

    const NAME: &'static str = "Heating";

    const RULES: &'static [Rule<Mirrors, Action>] = &[
        Rule {
            id: "HEAT_ON",
            when: &[Kind::DefogChanged],
            check: chart::check!(DefogOn),
            unknown: OnUnknown::Deny,
            emit: &[Action::On],
        },
        // No guard: the fallback line, reached only when the one above did not
        // match.
        Rule {
            id: "HEAT_OFF",
            when: &[Kind::DefogChanged],
            check: chart::check!(),
            unknown: OnUnknown::Deny,
            emit: &[Action::Off],
        },
    ];

    fn perform(action: Action, _ev: &Event, _world: &mut World) {
        match action {
            Action::On => log::debug!("heating on"),
            Action::Off => log::debug!("heating off"),
        }
    }
}
