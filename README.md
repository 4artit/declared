# chart

A declarative controller framework for Rust. You describe what a controller
reacts to and what it does about it as static data — a state table, or a
simple list of inputs and outputs — and that single declaration drives the
runtime, generates a mermaid diagram, and gets checked for gaps.

[한국어 문서](README_KR.md)

## Why

Controller logic tends to spread across `match` arms until "what happens when
X arrives" can only be answered by reading the whole file, and the diagram
someone drew for it slowly stops matching reality. chart flips that: the
transition table (or feature list) *is* the source of truth, so the diagram
and the exhaustive coverage check are generated from the exact data the
executor runs. There's nothing to keep in sync because there's only one copy.

## Two layers, one vocabulary

| Layer | Use when | You declare |
|---|---|---|
| `feature` | behavior doesn't depend on history | each feature's events in, actions out |
| `machine` | the same event means different things in different states | a transition table |

Both layers share a `Domain` — the events, actions, and outside-world type a
controller works with — so a stateless feature that later needs history
doesn't change; you just add a small `MachineSpec` next to it. Most
controllers are mostly `feature`, with a `machine` where it's actually needed.

Every declaration file imports `chart::prelude::*` — the traits, the row types
and the guard vocabulary in one line. The executors stay out of it, so
`machine::dispatch` still says where it comes from.

## Install

Not published to crates.io — use it as a path dependency.

```toml
[dependencies]
chart = { path = "../chart" }
```

## Quick start

A two-state light switch:

```rust
use chart::machine;
use chart::prelude::*;

chart::tags! { enum Tag { Off, On } }
chart::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind { Toggle }
}

/// Reactions to an event. Only these are handed the event.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action { Click }

/// Effects of being in a state, run whichever edge led there.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum StateAction { TurnOn, TurnOff }

struct Env;
struct Light;

impl Domain for Light {
    type Event = Event;
    type EventKind = Kind;
    type Env = Env;
}

impl MachineSpec for Light {
    const NAME: &'static str = "Light";

    type Domain = Light;
    type Tag = Tag;
    type Action = Action;
    type StateAction = StateAction;

    const STATES: &'static [State<Light>] = STATES;
    const EDGES: &'static [Edge<Light>] = EDGES;
    const IGNORES: &'static [Ignore<Light>] = IGNORES;

    fn perform(action: Action, _ev: &Event, _world: &mut Env) {
        match action {
            Action::Click => println!("click"),
        }
    }

    fn perform_state(action: StateAction, _world: &mut Env) {
        match action {
            StateAction::TurnOn => println!("on"),
            StateAction::TurnOff => println!("off"),
        }
    }
}

static STATES: &[State<Light>] = &[
    State { tag: Tag::Off, entry: &[StateAction::TurnOff], exit: &[] },
    State { tag: Tag::On,  entry: &[StateAction::TurnOn],  exit: &[] },
];

static EDGES: &[Edge<Light>] = &[
    Edge { id: "TURN_ON",  from: Source::These(&[Tag::Off]), when: Kind::Toggle,
           check: chart::check!(), unknown: OnUnknown::Deny,
           emit: &[Action::Click], goto: Goto::To(Tag::On) },
    Edge { id: "TURN_OFF", from: Source::These(&[Tag::On]),  when: Kind::Toggle,
           check: chart::check!(), unknown: OnUnknown::Deny,
           emit: &[Action::Click], goto: Goto::To(Tag::Off) },
];

static IGNORES: &[Ignore<Light>] = &[];

fn main() {
    let mut world = Env;
    // A machine resumes rather than starts: `Tag::Off` says the lamp is
    // already off, so `Off`'s entry action does not run here.
    let mut m = Machine::<Light>::new(Tag::Off);
    machine::dispatch(&mut m, &Event::Toggle, &mut world); // -> click, on
    machine::dispatch(&mut m, &Event::Toggle, &mut world); // -> click, off
}
```

Bigger examples:
- [`examples/door_lock`](examples/door_lock/main.rs) — four states, guard
  conditions, `Ignore` with wildcard sources. `cargo run --example door_lock`.
- [`examples/mirrors`](examples/mirrors/main.rs) — a controller mixing both
  layers: two stateless features next to one state machine.
  `cargo run --example mirrors`.

## What you get for declaring instead of coding

- **A runtime.** `machine::dispatch` (or `feature::dispatch`) reads the same
  table you wrote — no separate interpretation step to fall out of sync.
- **A diagram.** `render::state_diagram` turns the transition table into a
  `stateDiagram-v2` you can drop straight into docs, or convert to PlantUML
  with `scripts/mermaid_to_plantuml.sh`.
- **A gap check.** `verify::coverage` walks every `(state, event)`
  combination and reports the ones with no edge and no declared `Ignore`.
  Assert `is_clean()` in a test, and a forgotten case fails CI instead of
  surfacing in production.
- **One definition per condition.** A guard is declared against the `Domain`,
  so "power is on" is one node shared by every table rather than one per table.
  Node names are unique per domain: `verify::duplicate_node_names` reads the
  features and the machines together and reports two node types sharing a name.
- **Guards that admit failure.** Conditions evaluate to `True`/`False`/
  `Unknown` instead of `bool`, and `Edge::unknown` names the fail-open or
  fail-closed policy explicitly — it shows up on the diagram instead of
  hiding inside a guard function.
- **Traceable side effects.** A `perform` is the only place the outside world is
  touched, so every effect a dispatch produced is a plain value you can log or
  assert on. Every row carries an id — `Edge::id`, `Rule::id` — and both
  layers' `dispatch` hands back the one that ran.
- **Effects belong to whoever emits them.** Each feature and each machine names
  its own action type — `Feature::Action`, `MachineSpec::Action` — so every
  `perform` is exhaustive over exactly the effects its own file declares. Adding
  one is a compile error there and nowhere else, and no file collects effects
  belonging to another. `Domain` holds only what the parts really share: the
  events and the world.
- **Entry effects that cannot read the event.** Entry and exit run whichever
  edge led there, so they have their own vocabulary, `MachineSpec::StateAction`,
  and `perform_state` is handed no event — an effect that needs one goes on an
  edge. It lives on the machine because only a machine has states.

## Project layout

```
src/
  lib.rs          // Domain, MachineSpec — library entry points
  guard.rs        // shared by both layers: Cond, OnUnknown
  guard/          // CondNode, Cx, Memo, Expr
  feature.rs      // stateless layer: Feature, Rule, AnyFeature
  machine.rs      // stateful layer: Machine, dispatch, Taken
  machine/        // State, Edge, Source, Goto, Ignore
  verify.rs       // coverage, duplicate_node_names, unhandled_kinds, ...
  render.rs       // state_diagram, io_flowchart, *_table
examples/
  door_lock/      // cargo run --example door_lock
  mirrors/        // cargo run --example mirrors
```

Full API details (the `Domain`/`MachineSpec` contract, guard authoring,
dispatch order, etc.) are documented on the types themselves — run
`cargo doc --open`.

## Tests

```sh
cargo test
```
