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
use declared::{Domain, HasKind, render, verify};

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

    golden(
        "examples/mirrors/mirrors.md",
        include_str!("mirrors.md"),
        &document(),
    );
}

/// The whole controller, drawn from its declarations.
fn document() -> String {
    // Expected to list `FoldPositionChanged` and `UserChanged`: the first is a
    // signal the controller keeps rather than decides on, the second is reacted
    // to by nobody.
    let unhandled = verify::unhandled_kinds(FEATURES, &[]);

    // Node names are unique per domain: `Memo` keys on the name, so two node
    // types answering to one would make the second inherit the first's answer.
    let dup = verify::duplicate_node_names(FEATURES, &[]);
    assert!(dup.is_empty(), "guard names used by two node types: {dup:?}");

    // The id `dispatch` hands back carries no feature name, so it has to name
    // one rule across the whole controller.
    let dup_ids = verify::duplicate_rule_ids(FEATURES);
    assert!(dup_ids.is_empty(), "rule ids used twice: {dup_ids:?}");

    // One feature at a time: with an action type per feature, a dead effect is a
    // question about the file that owns it.
    let dead: Vec<String> = [
        format!("{:?}", verify::unemitted_actions::<Heating>()),
        format!("{:?}", verify::unemitted_actions::<Dimming>()),
        format!("{:?}", verify::unemitted_actions::<Fold>()),
    ]
    .into_iter()
    .filter(|s| s != "[]")
    .collect();
    assert!(dead.is_empty(), "declared but never emitted: {dead:?}");

    format!(
        "\
# Mirrors controller

What this controller reacts to and what it does about it. Generated from the
declarations, so it cannot drift from the code — regenerate with
`cargo run --example mirrors`.

## Features

One per file. `handles` and `emits` are read off the rules below, so a feature
cannot react to or emit anything this table omits.

{table}
## Rules

One line per `input -> output` rule. Within a feature the order is priority: the
first rule whose guard holds is the one that runs, so a rule with no guard is a
fallback.

{rules}
## By feature

Each feature, split by the set of events its rules react to. Rules triggered by
the same combination share a table and a diagram, so what a feature does on
each trigger reads in one place.

{by_feature}
## Checks

| Check | Result |
|---|---|
| Events nothing handles | {unhandled:?} |
| Guard names used by two node types | {dup:?} |
| Rule ids used twice | {dup_ids:?} |
",
        table = render::io_table(FEATURES),
        rules = render::rule_table(FEATURES),
        by_feature = by_feature(),
        unhandled = unhandled,
        dup = dup,
        dup_ids = dup_ids,
    )
}

/// One section per feature, and under it one table and diagram per `when` set.
fn by_feature() -> String {
    let mut s = String::new();
    for f in FEATURES {
        s.push_str(&format!("### {}\n\n", f.name()));
        for when in render::when_groups(*f) {
            let names: Vec<String> = when.iter().map(|k| format!("`{k:?}`")).collect();
            s.push_str(&format!("#### When {}\n\n", names.join(", ")));
            s.push_str(&render::when_table(*f, &when));
            s.push_str(&format!(
                "\n```mermaid\n{}```\n\n",
                render::when_flowchart(*f, &when)
            ));
        }
    }
    s
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

/// The committed `.md` is the golden: a normal run checks the generated
/// document against it and fails on drift, so the file cannot quietly stop
/// matching the code. Regenerate after an intended change with
/// `cargo run --example mirrors -- --write`.
fn golden(path: &str, committed: &str, generated: &str) {
    if std::env::args().any(|a| a == "--write") {
        std::fs::write(path, generated).unwrap_or_else(|e| panic!("failed to write {path}: {e}"));
        println!("\nwrote {path}");
    } else {
        assert_eq!(
            generated, committed,
            "\n{path} is stale. Re-run with --write to regenerate it.\n"
        );
        println!("\n{path} is up to date");
    }
}
