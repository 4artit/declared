//! Exhaustive checks over a transition table: the combinations it leaves out,
//! the declarations it contradicts, and the names it reuses.

use std::any::TypeId;

use crate::machine::{Edge, Goto, Ignore};
use crate::{Domain, MachineSpec};

/// The result of checking every `(state × event kind)` combination.
#[derive(Debug, Default)]
pub struct Coverage {
    /// Combinations with neither an edge nor an [`Ignore`]. **Must be empty in
    /// CI.**
    pub holes: Vec<(String, String)>,
    /// Combinations an [`Ignore`] forbids and an edge handles anyway, with the
    /// offending edge ids.
    pub ignored_but_handled: Vec<(String, String, Vec<&'static str>)>,
    /// Combinations matched by more than one edge. Not necessarily wrong, since
    /// declaration order is priority, but worth reviewing.
    pub overlaps: Vec<(String, String, Vec<&'static str>)>,
    /// States that cannot be reached from the initial state.
    pub unreachable: Vec<String>,
    /// Guard node names used by more than one node type. The [`crate::machine::Memo`] key
    /// is the name, so names must be unique.
    pub duplicate_node_names: Vec<&'static str>,
    /// Edge ids carried by more than one edge. The id names a transition in
    /// [`crate::machine::Taken`], in logs and in golden diffs.
    pub duplicate_edge_ids: Vec<&'static str>,
}

impl Coverage {
    /// Whether the table is free of defects. `overlaps` is excluded: it is a
    /// review signal, not an error.
    pub fn is_clean(&self) -> bool {
        self.holes.is_empty()
            && self.ignored_but_handled.is_empty()
            && self.unreachable.is_empty()
            && self.duplicate_node_names.is_empty()
            && self.duplicate_edge_ids.is_empty()
    }
}

/// Checks a transition table for gaps, contradicted [`Ignore`]s, unreachable
/// states, and duplicate names.
///
/// - `initial`: the machine's starting state.
/// - `edges`, `ignores`: the transition table to check.
///
/// Returns a [`Coverage`] report.
pub fn coverage<M: MachineSpec>(
    initial: M::Tag,
    edges: &'static [Edge<M>],
    ignores: &'static [Ignore<M>],
) -> Coverage {
    let mut out = Coverage::default();

    for &tag in M::all_tags() {
        for &kind in <M::Domain as Domain>::all_kinds() {
            let hits: Vec<&'static str> = edges
                .iter()
                .filter(|e| e.when == kind && e.from.matches(tag))
                .map(|e| e.id)
                .collect();

            // An `Ignore` is a floor, not a fallback: guards do not enter into
            // it, so a single edge on the combination contradicts the ban.
            let ignored = ignores.iter().any(|i| i.matches(tag, kind));

            if hits.is_empty() {
                if !ignored {
                    out.holes.push((format!("{tag:?}"), format!("{kind:?}")));
                }
            } else {
                if ignored {
                    out.ignored_but_handled.push((
                        format!("{tag:?}"),
                        format!("{kind:?}"),
                        hits.clone(),
                    ));
                }
                if hits.len() > 1 {
                    out.overlaps
                        .push((format!("{tag:?}"), format!("{kind:?}"), hits));
                }
            }
        }
    }

    // Tag is only required to be Copy + Eq + Debug, not Hash, so reachability uses
    // a linear scan. State counts are small, and this avoids treating two states
    // with coincidentally equal Debug output as the same state.
    let mut reached: Vec<M::Tag> = vec![initial];
    // Iterate to a fixed point; tables are small enough that this is fine.
    loop {
        let before = reached.len();
        for e in edges {
            if let Goto::To(next) = e.goto
                && !reached.contains(&next)
                && e.from.expand().iter().any(|t| reached.contains(t))
            {
                reached.push(next);
            }
        }
        if reached.len() == before {
            break;
        }
    }
    for &tag in M::all_tags() {
        if !reached.contains(&tag) {
            out.unreachable.push(format!("{tag:?}"));
        }
    }

    // Reusing one node across many edges is normal. What must be caught is two
    // *different* node types sharing a name, since they would then share a Memo
    // entry and poison each other's cached result.
    let mut node_ids: Vec<(&'static str, TypeId)> = Vec::new();
    for e in edges {
        e.check.node_ids(&mut node_ids);
    }
    node_ids.sort_unstable();
    node_ids.dedup();
    for w in node_ids.windows(2) {
        if w[0].0 == w[1].0 && !out.duplicate_node_names.contains(&w[0].0) {
            out.duplicate_node_names.push(w[0].0);
        }
    }

    // Unlike node names, every repeat is a defect, so the list is not deduped
    // before the scan.
    let mut edge_ids: Vec<&'static str> = edges.iter().map(|e| e.id).collect();
    edge_ids.sort_unstable();
    for w in edge_ids.windows(2) {
        if w[0] == w[1] && !out.duplicate_edge_ids.contains(&w[0]) {
            out.duplicate_edge_ids.push(w[0]);
        }
    }

    out
}

/// The event kinds a transition table acts on. `Ignore`d kinds do not count —
/// pass this to [`crate::feature::unhandled_kinds`] alongside a feature list so
/// a controller mixing both layers is checked as one unit.
///
/// - `edges`: the transition table to scan.
///
/// Returns the event kinds matched by at least one edge.
pub fn handled_kinds<M: MachineSpec>(
    edges: &'static [Edge<M>],
) -> Vec<<M::Domain as Domain>::EventKind> {
    let mut out: Vec<_> = Vec::new();
    for e in edges {
        if !out.contains(&e.when) {
            out.push(e.when);
        }
    }
    out
}
