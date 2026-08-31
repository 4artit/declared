//! Diagrams and tables derived from a controller's declaration.
//!
//! The [`crate::machine`] functions read a transition table; the
//! [`crate::feature`] ones read a list of [`AnyFeature`].

use std::fmt::Write as _;

use crate::feature::{AnyFeature, RuleRow};
use crate::guard::OnUnknown;
use crate::machine::{Edge, Goto, Ignore, State};
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
        let emit = if e.emit.is_empty() {
            String::new()
        } else {
            format!("<br/>/ {}", join_actions(e.emit))
        };
        for from in e.from.expand() {
            let label = if guard.is_empty() {
                format!("{:?}", e.when)
            } else {
                format!("{:?}<br/>[{guard}]", e.when)
            };
            let _ = writeln!(s, "    {from:?} --> {next:?}: {label}{unknown}{emit}");
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
                join_actions(e.emit),
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
pub fn io_table<D: Domain>(features: &[&dyn AnyFeature<D>]) -> String {
    let mut s = String::from("| feature | handles | emits |\n|---|---|---|\n");
    for f in features {
        let _ = writeln!(
            s,
            "| `{}` | {} | {} |",
            f.name(),
            join_or_dash(&f.handles()),
            join_names(&f.emits())
        );
    }
    s
}

/// Tabulates every rule of every feature: the `input -> output` lines a feature
/// is made of.
///
/// - `features`: the feature list to render.
///
/// Returns a markdown table. Row order is declaration order, which is also
/// priority — the first matching rule of a feature is the one that runs.
pub fn rule_table<D: Domain>(features: &[&dyn AnyFeature<D>]) -> String {
    let mut s = String::from("| feature | when | guard | emits |\n|---|---|---|---|\n");
    for f in features {
        let rows = f.rows();
        for (i, r) in rows.iter().enumerate() {
            let unknown = if r.unknown == OnUnknown::Allow {
                " (unknown=Allow)"
            } else {
                ""
            };
            let _ = writeln!(
                s,
                "| `{}` | {} | {}{unknown} | {} |",
                f.name(),
                join_or_dash(r.when),
                guard_cell(&rows, i),
                join_names(&r.emit),
            );
        }
    }
    s
}

/// Draws a mermaid flowchart of events → features → actions.
///
/// - `features`: the feature list to render.
///
/// Returns the diagram source. The guard of the rule that produces an action
/// labels the arrow to it, so the diagram says *why* an action is emitted and
/// not only that it can be. Action node ids carry the feature name because an
/// action belongs to one feature: two features that happen to name an effect
/// alike are drawing two different effects.
pub fn io_flowchart<D: Domain>(features: &[&dyn AnyFeature<D>]) -> String {
    let mut s = String::from("flowchart LR\n");
    for f in features {
        let name = f.name();
        for k in f.handles() {
            let _ = writeln!(s, "    ev_{k:?}[\"{k:?}\"] --> ft_{name}[\"{name}\"]");
        }
        let rows = f.rows();
        for (i, r) in rows.iter().enumerate() {
            let label = match (r.guard.is_empty(), is_fallback(&rows, i)) {
                (false, _) => format!("|\"{}\"|", r.guard),
                (true, true) => "|else|".to_string(),
                (true, false) => String::new(),
            };
            for a in &r.emit {
                let _ = writeln!(
                    s,
                    "    ft_{name}[\"{name}\"] -->{label} ac_{name}_{a}[\"{a}\"]"
                );
            }
        }
    }
    s
}

/// Whether an unguarded rule is a fallback rather than an unconditional one:
/// an earlier rule of the same feature covers one of its kinds, so this line is
/// reached only when that one did not match.
fn is_fallback<D: Domain>(rows: &[RuleRow<D>], i: usize) -> bool {
    rows[..i]
        .iter()
        .any(|e| e.when.iter().any(|k| rows[i].when.contains(k)))
}

/// The guard column of one rule: the expression, `else` for a fallback, or a
/// dash for a rule that is genuinely unconditional.
fn guard_cell<D: Domain>(rows: &[RuleRow<D>], i: usize) -> String {
    match (rows[i].guard.is_empty(), is_fallback(rows, i)) {
        (false, _) => format!("`{}`", rows[i].guard),
        (true, true) => "else".to_string(),
        (true, false) => "—".to_string(),
    }
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

/// Like [`join_or_dash`] for names already rendered to text.
fn join_names(items: &[String]) -> String {
    if items.is_empty() {
        return "—".into();
    }
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
