//! The controller's conditions, in one place.
//!
//! Declared against `Mirrors`, not against a machine or a feature, so `fold.rs`
//! and `dimming.rs` decide "the power is on" by the same node rather than each
//! reading `world.power_on` for itself.

use declared::guard::Cond;

use crate::{Event, Mirrors};

declared::cond_node!(Mirrors, PowerOn, |cx| Cond::from(cx.world.power_on));
declared::cond_node!(Mirrors, PowerOff, |cx| Cond::from(!cx.world.power_on));
declared::cond_node!(Mirrors, GearReverse, |cx| Cond::from(cx.world.gear_reverse));

declared::cond_node!(Mirrors, SpeedAllowsFold, |cx| Cond::from(
    cx.world.speed < 15.0
));
declared::cond_node!(Mirrors, SpeedForcesUnfold, |cx| Cond::from(
    cx.world.speed >= 40.0
));

declared::cond_node!(Mirrors, AtFolded, |cx| Cond::from(
    cx.world.fold_position <= 0.01
));
declared::cond_node!(Mirrors, AtUnfolded, |cx| Cond::from(
    cx.world.fold_position >= 0.99
));

// Reads the event rather than the world: the defog signal is not something the
// controller keeps, it only passes through.
declared::cond_node!(Mirrors, DefogOn, |cx| match cx.event {
    Event::DefogChanged(on) => Cond::from(*on),
    _ => Cond::False,
});
