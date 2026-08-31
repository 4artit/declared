//! A side mirror controller split by feature, mixing both layers.
//!
//!     cargo run --example mirrors
//!
//! | File | Layer | Why |
//! |---|---|---|
//! | `guards.rs` | both | The conditions either layer decides by |
//! | `heating.rs` | feature | Follows the defog signal |
//! | `dimming.rs` | feature | A function of power and gear |
//! | `fold.rs` | machine | Folding and unfolding are observable states |
//!
//! A ticket about folding is answered by `fold.rs` alone. `handle_event` routes
//! and does not decide.

mod dimming;
mod fold;
mod guards;
mod heating;

use chart::feature::{self, AnyFeature};
use chart::machine::{self, Machine};
use chart::{Domain, HasKind, render, verify};

use dimming::Dimming;
use fold::FoldSm;
use heating::Heating;

chart::events! {
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
    type Env = World;
}

// ─────────────────────────────────────────── router

/// The features, in dispatch order. The router walks this list, so what the
/// document draws and what actually runs cannot come apart. They are trait
/// objects because each feature has its own action type.
const FEATURES: &[&dyn AnyFeature<Mirrors>] = &[&Heating, &Dimming];

/// Holds only the machine: a feature is a table, not an object, so there is no
/// feature instance for the controller to keep.
struct Controller {
    fold: Machine<FoldSm>,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            fold: fold::machine(),
        }
    }
}

impl Controller {
    fn handle_event(&mut self, ev: &Event, world: &mut World) {
        // The one global gate. It says why it dropped the event.
        if !world.power_on && requires_power(ev.kind()) {
            println!("  (dropped: powered off) {ev:?}");
            return;
        }

        for f in FEATURES {
            f.dispatch(ev, world);
        }
        machine::dispatch(&mut self.fold, ev, world);
    }
}

/// Which events need power, as one list rather than a check per handler.
fn requires_power(kind: Kind) -> bool {
    matches!(kind, Kind::DefogChanged)
}

// ─────────────────────────────────────────── run

fn main() {
    let mut c = Controller::default();
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
        c.handle_event(ev, &mut w);
    }

    let path = "examples/mirrors/mirrors.md";
    std::fs::write(path, document()).expect("failed to write mirrors.md");
    println!("\nwrote {path}");
}

/// The whole controller, drawn from its declarations.
fn document() -> String {
    // Both layers at once: the machine's events are not holes.
    let by_fold = fold::handled_kinds();
    let unhandled = feature::unhandled_kinds(FEATURES, &[&by_fold]);

    // `unhandled` is expected to list `UserChanged`, which nothing reacts to.
    // A defective fold table is not expected, so it fails the run rather than
    // being written into the document as a line claiming otherwise.
    let cov = fold::coverage();
    assert!(cov.is_clean(), "{cov:?}");

    // Both layers at once again, for the other thing they share: node names are
    // unique per domain, and only a check spanning both can say so.
    let dup = verify::duplicate_node_names(FEATURES, &[&fold::guard_nodes()]);
    assert!(dup.is_empty(), "guard names used by two node types: {dup:?}");

    format!(
        "\
# Mirrors controller

What this controller reacts to and what it does about it. Generated from the
declarations, so it cannot drift from the code — regenerate with
`cargo run --example mirrors`.

## Features

Stateless features, one per file. `handles` and `emits` are read off the rules
below, so a feature cannot react to or emit anything this table omits.

{table}
## Rules

One line per `input -> output` rule. Within a feature the order is priority: the
first rule whose guard holds is the one that runs, so a rule with no guard is a
fallback.

{rules}
## Events, features and actions

```mermaid
{flow}```

## Folding

Folding and unfolding are observable states, so this one is a state machine.

```mermaid
{diagram}```

## Checks

| Check | Result |
|---|---|
| Events nothing handles | {unhandled:?} |
| Holes in the fold table | {holes:?} |
| Fold table is clean | {clean} |
| Guard names used by two node types | {dup:?} |
",
        table = render::io_table(FEATURES),
        rules = render::rule_table(FEATURES),
        flow = render::io_flowchart(FEATURES),
        diagram = fold::diagram(),
        unhandled = unhandled,
        holes = cov.holes,
        clean = cov.is_clean(),
        dup = dup,
    )
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
