//! Diagrams and tables derived from a controller's declaration.
//!
//! Everything here reads a list of [`AnyFeature`] and nothing else, so a
//! document cannot disagree with what the router walks.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::feature::{AnyFeature, RuleRow};
use crate::guard::OnUnknown;
use crate::Domain;



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
    let mut s =
        String::from("| feature | rule | when | guard | emits |\n|---|---|---|---|---|\n");
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
                "| `{}` | `{}` | {} | {}{unknown} | {} |",
                f.name(),
                r.id,
                join_or_dash(r.when),
                guard_cell(&r.guard, is_fallback(&rows, i)),
                join_names(&r.emit),
            );
        }
    }
    s
}

// ─────────────────────────────────────────── one event at a time

/// Tabulates one event kind's rules, across every feature that reacts to it.
///
/// - `features`: the feature list to render.
/// - `kind`: the event kind to select rows for.
///
/// Returns a markdown table, empty of rows if nothing reacts to `kind`. Row
/// order is dispatch order. A rule reacting to several kinds appears under each
/// of them, which the `when` column shows.
pub fn event_table<D: Domain>(features: &[&dyn AnyFeature<D>], kind: D::EventKind) -> String {
    let mut s =
        String::from("| feature | rule | when | guard | emits |\n|---|---|---|---|---|\n");
    for f in features {
        // Every selected row reacts to `kind`, so an unguarded one is a
        // fallback exactly when it is not the feature's first here.
        let mut seen = false;
        for r in f.rows().iter().filter(|r| r.when.contains(&kind)) {
            let unknown = if r.unknown == OnUnknown::Allow {
                " (unknown=Allow)"
            } else {
                ""
            };
            let _ = writeln!(
                s,
                "| `{}` | `{}` | {} | {}{unknown} | {} |",
                f.name(),
                r.id,
                join_or_dash(r.when),
                guard_cell(&r.guard, seen),
                join_names(&r.emit),
            );
            seen = true;
        }
    }
    s
}

/// Draws one event kind's path through the controller: the event, the features
/// that react to it, and the actions their rules emit.
///
/// - `features`: the feature list to render.
/// - `kind`: the event kind to draw.
///
/// Returns the diagram source, holding nothing but the header if no feature
/// reacts to `kind`. Priority is not drawn; [`event_table`] is what orders the
/// rules.
pub fn event_flowchart<D: Domain>(features: &[&dyn AnyFeature<D>], kind: D::EventKind) -> String {
    let mut s = String::from("flowchart LR\n");
    for f in features {
        let name = f.name();
        let rows = f.rows();
        if !rows.iter().any(|r| r.when.contains(&kind)) {
            continue;
        }
        let ev = node_id(&format!("ev_{kind:?}"));
        let ft = node_id(&format!("ft_{name}"));
        let _ = writeln!(s, "    {ev}[\"{kind:?}\"] --> {ft}[\"{name}\"]");
        let mut seen = false;
        for r in rows.iter().filter(|r| r.when.contains(&kind)) {
            let label = edge_label(r.id, &r.guard, seen);
            for a in &r.emit {
                let ac = node_id(&format!("ac_{name}_{a}"));
                let _ = writeln!(s, "    {ft}[\"{name}\"] -->|\"{label}\"| {ac}[\"{a}\"]");
            }
            seen = true;
        }
    }
    s
}

/// Folds a `Debug` rendering into a mermaid node identifier, replacing every
/// character mermaid could read as syntax. Identifiers only — a label is
/// quoted and keeps the original text.
fn node_id(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The label on a rule's arrow: its id, then the guard it decides by. An
/// unconditional rule is its id alone.
///
/// - `id`: the rule's [`crate::feature::Rule::id`].
/// - `guard`: its rendered guard, empty for an unguarded rule.
/// - `fallback`: whether an earlier rule shadows this one. The caller's to
///   decide, since a whole-table view and a per-event one differ on it.
fn edge_label(id: &str, guard: &str, fallback: bool) -> String {
    match (guard.is_empty(), fallback) {
        (false, _) => format!("{id}<br/>{guard}"),
        (true, true) => format!("{id}<br/>else"),
        (true, false) => id.into(),
    }
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
/// dash for a rule that is genuinely unconditional. `fallback` is the caller's
/// to decide, as in [`edge_label`].
fn guard_cell(guard: &str, fallback: bool) -> String {
    match (guard.is_empty(), fallback) {
        (false, _) => format!("`{guard}`"),
        (true, true) => "else".into(),
        (true, false) => "—".into(),
    }
}

fn join_or_dash<T: core::fmt::Debug>(items: &[T]) -> String {
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
