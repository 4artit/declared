//! A side mirror controller split by feature.
//!
//!     cargo run --example mirrors
//!
//! | File | Why |
//! |---|---|
//! | `guards.rs` | The conditions the controller decides by |
//! | `heating.rs` | Follows the defog signal |
//! | `dimming.rs` | A function of power and gear |
//! | `fold.rs` | Folds and unfolds, reading the motor's reported position |
//!
//! A ticket about folding is answered by `fold.rs` alone. `handle_event` routes
//! and does not decide.

mod dimming;
mod fold;
mod guards;
mod heating;

use declared::feature::AnyFeature;
use declared::{Domain, HasKind, report};

use dimming::Dimming;
use fold::Fold;
use heating::Heating;

declared::events! {
    #[derive(Clone, Debug)]
    pub enum Event => Kind {
        DefogChanged(bool),
        PowerChanged(bool),
        GearChanged(bool),
        SpeedChanged(f32),
        /// Position reported by the fold motor. 0.0 folded, 1.0 unfolded.
        FoldPositionChanged(f32),
        UserChanged,
    }
}

/// The outside world. Stands in for an API bridge.
#[derive(Default)]
pub struct World {
    pub power_on: bool,
    pub gear_reverse: bool,
    pub speed: f32,
    pub fold_position: f32,
}

/// What the parts of this controller have in common: the events and the world.
/// Effects are not in common — `heating.rs`, `dimming.rs` and `fold.rs` each
/// name their own, so no file here holds another file's effects.
pub struct Mirrors;

impl Domain for Mirrors {
    type Event = Event;
    type EventKind = Kind;
    type World = World;
}

// ─────────────────────────────────────────── router

/// The features, in dispatch order. The router walks this list, so what the
/// document draws and what actually runs cannot come apart. They are trait
/// objects because each feature has its own action type.
const FEATURES: &[&dyn AnyFeature<Mirrors>] = &[&Heating, &Dimming, &Fold];

/// Routes and does not decide. A feature is a table rather than an object, so
/// there is no controller instance to keep — this is a function.
fn handle_event(ev: &Event, world: &mut World) {
    // The one global gate. It says why it dropped the event.
    if !world.power_on && requires_power(ev.kind()) {
        println!("  (dropped: powered off) {ev:?}");
        return;
    }

    for f in FEATURES {
        f.dispatch(ev, world);
    }
}

/// Which events need power, as one list rather than a check per handler.
fn requires_power(kind: Kind) -> bool {
    matches!(kind, Kind::DefogChanged)
}

// ─────────────────────────────────────────── run

fn main() {
    let mut w = World {
        fold_position: 1.0, // starts unfolded
        ..Default::default()
    };

    let steps: &[(&str, Event)] = &[
        ("defog while powered off", Event::DefogChanged(true)),
        ("power on", Event::PowerChanged(true)),
        ("defog on", Event::DefogChanged(true)),
        ("gear to reverse", Event::GearChanged(true)),
        ("power off, folding starts", Event::PowerChanged(false)),
        ("motor halfway", Event::FoldPositionChanged(0.5)),
        ("motor folded", Event::FoldPositionChanged(0.0)),
        ("power on, unfolding starts", Event::PowerChanged(true)),
        ("motor unfolded", Event::FoldPositionChanged(1.0)),
        ("user switched, handled by nobody", Event::UserChanged),
    ];

    println!("── run ──");
    for (desc, ev) in steps {
        println!("{desc}");
        apply_signal(ev, &mut w);
        handle_event(ev, &mut w);
    }

    // Lists `FoldPositionChanged` and `UserChanged` as unhandled: the first is a
    // signal the controller keeps rather than decides on, the second is reacted
    // to by nobody.
    let r = report::features("Mirrors controller", FEATURES);
    assert!(r.is_clean(), "{:?}", r.defects);
    declared::golden!("examples/mirrors/mirrors.md", &r.markdown);
}

/// Applies the value a callback carried. A real service would do this.
fn apply_signal(ev: &Event, w: &mut World) {
    match ev {
        Event::PowerChanged(on) => w.power_on = *on,
        Event::GearChanged(rev) => w.gear_reverse = *rev,
        Event::SpeedChanged(v) => w.speed = *v,
        Event::FoldPositionChanged(p) => w.fold_position = *p,
        _ => {}
    }
}
