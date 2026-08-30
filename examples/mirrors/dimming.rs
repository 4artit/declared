//! Mirror dimming. A function of the current power state and gear, so it keeps
//! no state either.
//!
//! Both rules cover both kinds: whichever of the two signals changed, the
//! answer is read from the world, which the caller has already updated. That is
//! why the condition is written once rather than once per event.

use chart::feature::{Feature, FeatureInfo, Rule};
use chart::guard::OnUnknown;

use crate::guards::{GearReverse, PowerOn};
use crate::{Action, Kind, Mirrors};

pub struct Dimming;

/// The two signals dimming is a function of.
const INPUTS: &[Kind] = &[Kind::PowerChanged, Kind::GearChanged];

impl Feature<Mirrors> for Dimming {
    const INFO: FeatureInfo<Mirrors> = FeatureInfo {
        name: "Dimming",
        rules: &[
            Rule {
                when: INPUTS,
                check: chart::check!(PowerOn && !GearReverse),
                unknown: OnUnknown::Deny,
                emit: &[Action::DimmingOn],
            },
            Rule {
                when: INPUTS,
                check: chart::check!(),
                unknown: OnUnknown::Deny,
                emit: &[Action::DimmingOff],
            },
        ],
    };
}
