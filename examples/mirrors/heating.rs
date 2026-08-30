//! Mirror heating. Follows the defog signal, so it keeps no state.

use chart::feature::{Feature, FeatureInfo, Rule};
use chart::guard::OnUnknown;

use crate::guards::DefogOn;
use crate::{Action, Kind, Mirrors};

pub struct Heating;

impl Feature<Mirrors> for Heating {
    const INFO: FeatureInfo<Mirrors> = FeatureInfo {
        name: "Heating",
        rules: &[
            Rule {
                when: &[Kind::DefogChanged],
                check: chart::check!(DefogOn),
                unknown: OnUnknown::Deny,
                emit: &[Action::HeatingOn],
            },
            // No guard: the fallback line, reached only when the one above did
            // not match.
            Rule {
                when: &[Kind::DefogChanged],
                check: chart::check!(),
                unknown: OnUnknown::Deny,
                emit: &[Action::HeatingOff],
            },
        ],
    };
}
