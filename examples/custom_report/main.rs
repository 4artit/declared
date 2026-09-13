//! A document built from `render` and `verify` directly, instead of `report`.
//!
//!     cargo run --example custom_report
//!
//! Differs from what `report::features` produces in three ways:
//!
//! - its own prose and section order, and no `io_table`;
//! - `verify::unemitted_actions`, which `report` cannot call on a feature list;
//! - a stricter policy: an event nothing handles fails the run.

use declared::prelude::*;
use declared::{render, verify};

declared::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind {
        GearChanged(bool),
        DoorOpened(bool),
    }
}

#[derive(Default)]
struct World {
    reverse: bool,
    door_open: bool,
}

struct RearCam;

impl Domain for RearCam {
    type Event = Event;
    type EventKind = Kind;
    type World = World;
}

declared::cond_node!(RearCam, InReverse, |cx| Cond::from(cx.world.reverse));
declared::cond_node!(RearCam, DoorOpen, |cx| Cond::from(cx.world.door_open));

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum CamAction {
    Show,
    Hide,
}

// `unemitted_actions` needs every value of the action type.
impl Enumerable for CamAction {
    const ALL: &'static [Self] = &[Self::Show, Self::Hide];
}

struct Camera;

impl Feature for Camera {
    type Domain = RearCam;
    type Action = CamAction;

    const NAME: &'static str = "Camera";

    const RULES: &'static [Rule<RearCam, CamAction>] = &[
        Rule {
            id: "CAM_SHOW",
            when: &[Kind::GearChanged],
            check: declared::check!(InReverse),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::Show],
        },
        Rule {
            id: "CAM_HIDE",
            when: &[Kind::GearChanged],
            check: declared::check!(),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::Hide],
        },
    ];

    fn perform(action: CamAction, _ev: &Event, _world: &mut World) {
        println!("  [camera] {action:?}");
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ChimeAction {
    Warn,
}

impl Enumerable for ChimeAction {
    const ALL: &'static [Self] = &[Self::Warn];
}

struct Chime;

impl Feature for Chime {
    type Domain = RearCam;
    type Action = ChimeAction;

    const NAME: &'static str = "Chime";

    const RULES: &'static [Rule<RearCam, ChimeAction>] = &[Rule {
        id: "CHIME_DOOR_IN_REVERSE",
        when: &[Kind::GearChanged, Kind::DoorOpened],
        check: declared::check!(InReverse && DoorOpen),
        unknown: OnUnknown::Deny,
        emit: &[ChimeAction::Warn],
    }];

    fn perform(action: ChimeAction, _ev: &Event, _world: &mut World) {
        println!("  [chime] {action:?}");
    }
}

const FEATURES: &[&dyn AnyFeature<RearCam>] = &[&Camera, &Chime];

fn main() {
    let mut w = World::default();
    let steps = [
        Event::GearChanged(true),
        Event::DoorOpened(true),
        Event::GearChanged(false),
    ];
    for ev in &steps {
        println!("{ev:?}");
        match ev {
            Event::GearChanged(r) => w.reverse = *r,
            Event::DoorOpened(o) => w.door_open = *o,
        }
        for f in FEATURES {
            f.dispatch(ev, &mut w);
        }
    }

    declared::golden!("examples/custom_report/custom_report.md", &document());
}

fn document() -> String {
    let unhandled = verify::unhandled_kinds(FEATURES, &[]);
    assert!(unhandled.is_empty(), "events nothing handles: {unhandled:?}");

    let dup = verify::duplicate_node_names(FEATURES, &[]);
    assert!(dup.is_empty(), "guard names used by two node types: {dup:?}");

    let empty = verify::empty_rules(FEATURES);
    assert!(empty.is_empty(), "rules with no event or no action: {empty:?}");

    // One feature at a time, since each has its own action type.
    let dead_camera = verify::unemitted_actions::<Camera>();
    let dead_chime = verify::unemitted_actions::<Chime>();
    assert!(dead_camera.is_empty() && dead_chime.is_empty());

    let mut md = String::from(
        "\
# Rear camera

Shows the camera in reverse, and chimes if a door opens while reversing.

## Rules

Within a feature, the first rule whose guard holds runs.

",
    );
    md.push_str(&render::rule_table(FEATURES));

    for f in FEATURES {
        for when in render::when_groups(*f) {
            md.push_str(&format!("\n## {} on {when:?}\n\n", f.name()));
            md.push_str(&format!(
                "```mermaid\n{}```\n",
                render::when_flowchart(*f, &when)
            ));
        }
    }

    md.push_str(&format!(
        "\n## Checks\n\n\
         | Check | Result |\n\
         |---|---|\n\
         | Events nothing handles | {unhandled:?} |\n\
         | Guard names used by two node types | {dup:?} |\n\
         | Rules with no event or no action | {empty:?} |\n\
         | Camera actions never emitted | {dead_camera:?} |\n\
         | Chime actions never emitted | {dead_chime:?} |\n"
    ));
    md
}
