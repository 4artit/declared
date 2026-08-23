//! Diagrams and tables derived from a controller's declaration.
//!
//! The [`crate::machine`] functions read a transition table; the
//! [`crate::feature`] ones read a list of [`FeatureInfo`].

use std::fmt::Write as _;

use crate::feature::FeatureInfo;
use crate::machine::{Edge, Goto, Ignore, OnUnknown, State};
use crate::{Domain, Enumerable, MachineSpec};

/// Builds a mermaid `stateDiagram-v2` diagram from a transition table.
///
/// - `initial`: the machine's starting state.
/// - `edges`: the transitions to draw. [`Goto::Internal`] edges are omitted —
///   use [`internal_table`] for those.
/// - `states`: entry/exit actions, drawn as state descriptions.
///
/// Returns the diagram source. Converts to PlantUML almost line for line;
/// see `scripts/mermaid_to_plantuml.sh`.
pub fn to_mermaid<M: MachineSpec>(
    initial: M::Tag,
    edges: &'static [Edge<M>],
    states: &'static [State<M>],
) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    let _ = writeln!(s, "    [*] --> {initial:?}");

    // Declaration order, not the order the nodes were passed in.
    for &tag in <M::Tag as Enumerable>::ALL {
        let Some(st) = states.iter().find(|s| s.tag == tag) else {
            continue;
        };
        let mut lines: Vec<String> = Vec::new();
        if !st.entry.is_empty() {
            lines.push(format!("entry / {}", join_actions(st.entry)));
        }
        if !st.exit.is_empty() {
            lines.push(format!("exit / {}", join_actions(st.exit)));
        }
        if !lines.is_empty() {
            // A mermaid description replaces the node's label, so it has to repeat
            // the state name.
            let _ = writeln!(s, "    {tag:?} : {tag:?}<br/>{}", lines.join("<br/>"));
        }
    }

    for e in edges {
        let Goto::To(next) = e.goto else { continue };
        let guard = e.check.render();
        let unknown = if e.unknown == OnUnknown::Allow {
            "<br/>unknown=Allow"
        } else {
            ""
        };
        let run = if e.run.is_empty() {
            String::new()
        } else {
            format!("<br/>/ {}", join_actions(e.run))
        };
        for from in e.from.expand() {
            let label = if guard.is_empty() {
                format!("{:?}", e.when)
            } else {
                format!("{:?}<br/>[{guard}]", e.when)
            };
            let _ = writeln!(s, "    {from:?} --> {next:?}: {label}{unknown}{run}");
        }
    }

    s
}

/// Tabulates the transitions that do not change state ([`Goto::Internal`]).
///
/// - `edges`: the transition table to scan.
///
/// Returns a markdown table.
pub fn internal_table<M: MachineSpec>(edges: &'static [Edge<M>]) -> String {
    let mut s =
        String::from("| state | event | guard | actions | edge id |\n|---|---|---|---|---|\n");
    for e in edges {
        if !matches!(e.goto, Goto::Internal) {
            continue;
        }
        let guard = e.check.render();
        for from in e.from.expand() {
            let _ = writeln!(
                s,
                "| `{from:?}` | `{:?}` | `{}` | {} | `{}` |",
                e.when,
                if guard.is_empty() { "—" } else { &guard },
                join_actions(e.run),
                e.id
            );
        }
    }
    s
}

/// Tabulates the deliberately unhandled combinations and their reasons.
///
/// - `ignores`: the ignore list to render.
///
/// Returns a markdown table.
pub fn ignore_table<M: MachineSpec>(ignores: &'static [Ignore<M>]) -> String {
    let mut s = String::from("| state | event | reason |\n|---|---|---|\n");
    for i in ignores {
        for from in i.from.expand() {
            for kind in i.when {
                let _ = writeln!(s, "| `{from:?}` | `{kind:?}` | {} |", i.why);
            }
        }
    }
    s
}

fn join_actions<A: std::fmt::Debug>(actions: &[A]) -> String {
    actions
        .iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ─────────────────────────────────────────── feature layer

/// Tabulates what each feature reacts to and emits.
///
/// - `features`: the feature list to render.
///
/// Returns a markdown table.
pub fn io_table<D: Domain>(features: &[FeatureInfo<D>]) -> String {
    let mut s = String::from("| feature | handles | emits |\n|---|---|---|\n");
    for f in features {
        let _ = writeln!(
            s,
            "| `{}` | {} | {} |",
            f.name,
            join_or_dash(f.handles),
            join_or_dash(f.emits)
        );
    }
    s
}

/// Draws a mermaid flowchart of events → features → actions.
///
/// - `features`: the feature list to render.
///
/// Returns the diagram source. Node ids carry a prefix because a feature and
/// the action it emits routinely share a name, which would otherwise merge
/// them into one self-looping node.
pub fn io_flowchart<D: Domain>(features: &[FeatureInfo<D>]) -> String {
    let mut s = String::from("flowchart LR\n");
    for f in features {
        for k in f.handles {
            let _ = writeln!(s, "    ev_{k:?}[\"{k:?}\"] --> ft_{0}[\"{0}\"]", f.name);
        }
        for a in f.emits {
            let _ = writeln!(s, "    ft_{0}[\"{0}\"] --> ac_{a:?}[\"{a:?}\"]", f.name);
        }
    }
    s
}

fn join_or_dash<T: std::fmt::Debug>(items: &[T]) -> String {
    if items.is_empty() {
        return "—".into();
    }
    items
        .iter()
        .map(|i| format!("`{i:?}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
