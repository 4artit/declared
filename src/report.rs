//! One document per controller: every table and diagram [`crate::render`] draws,
//! followed by the [`crate::verify`] results.
//!
//! For a document laid out differently, call `render` and `verify` directly;
//! `examples/custom_report` does.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::feature::AnyFeature;
use crate::{Domain, MachineSpec, render, verify};

/// A generated document and the defects found while building it.
pub struct Report {
    /// The document, as markdown with mermaid blocks.
    pub markdown: String,
    /// One line per check that must be empty but is not.
    pub defects: Vec<String>,
}

impl Report {
    /// Whether no defect was found.
    pub fn is_clean(&self) -> bool {
        self.defects.is_empty()
    }
}

/// Reports on a feature list.
///
/// - `title`: the document heading.
/// - `features`: the controller's feature list.
///
/// Defects: guard names used by two node types, repeated rule ids, rules with no
/// event or no action. Unhandled event kinds are listed but are not a defect.
pub fn features<D: Domain>(title: &str, features: &[&dyn AnyFeature<D>]) -> Report {
    let unhandled = verify::unhandled_kinds(features, &[]);
    let checks = Checks::default()
        .info("Events nothing handles", &unhandled)
        .defect(
            "Guard names used by two node types",
            &verify::duplicate_node_names(features, &[]),
        )
        .defect("Rule ids used twice", &verify::duplicate_rule_ids(features))
        .defect(
            "Rules with no event or no action",
            &verify::empty_rules(features),
        );

    let mut md = format!(
        "# {title}\n\n## Features\n\n{}\n## Rules\n\n{}\n## By feature\n\n",
        render::io_table(features),
        render::rule_table(features),
    );
    for f in features {
        let _ = write!(md, "### {}\n\n", f.name());
        for when in render::when_groups(*f) {
            let names: Vec<String> = when.iter().map(|k| format!("`{k:?}`")).collect();
            let _ = write!(
                md,
                "#### When {}\n\n{}\n```mermaid\n{}```\n\n",
                names.join(", "),
                render::when_table(*f, &when),
                render::when_flowchart(*f, &when),
            );
        }
    }

    checks.finish(md)
}

/// Reports on one machine, reading its tables from `M`.
///
/// - `initial`: the state the machine starts in.
///
/// Defects: everything [`verify::Coverage::is_clean`] rejects. Overlapping edges
/// are listed but are not a defect.
pub fn machine<M: MachineSpec>(initial: M::Tag) -> Report {
    let c = verify::coverage::<M>(initial, M::EDGES, M::IGNORES);
    let checks = Checks::default()
        .defect("Uncovered combinations", &c.holes)
        .defect("Ignored but handled", &c.ignored_but_handled)
        .defect("Unreachable states", &c.unreachable)
        .defect(
            "Guard names used by two node types",
            &c.duplicate_node_names,
        )
        .defect("Edge ids used twice", &c.duplicate_edge_ids)
        .info("Overlapping edges", &c.overlaps);

    let md = format!(
        "# {}\n\n## Transitions\n\n```mermaid\n{}```\n\n## In-place transitions\n\n{}\n## Deliberately unhandled\n\n{}\n",
        M::NAME,
        render::state_diagram::<M>(initial, M::EDGES, M::STATES),
        render::internal_table::<M>(M::EDGES),
        render::ignore_table::<M>(M::IGNORES),
    );

    checks.finish(md)
}

/// The `## Checks` table and the defects it found.
#[derive(Default)]
struct Checks {
    table: String,
    defects: Vec<String>,
}

impl Checks {
    fn info<T: core::fmt::Debug>(mut self, name: &str, found: &[T]) -> Self {
        let _ = writeln!(self.table, "| {name} | {found:?} |");
        self
    }

    fn defect<T: core::fmt::Debug>(mut self, name: &str, found: &[T]) -> Self {
        if !found.is_empty() {
            self.defects.push(format!("{name}: {found:?}"));
        }
        self.info(name, found)
    }

    fn finish(self, mut md: String) -> Report {
        let _ = write!(md, "## Checks\n\n| Check | Result |\n|---|---|\n{}", self.table);
        Report {
            markdown: md,
            defects: self.defects,
        }
    }
}

/// Compares `generated` with the committed file at `path`, or rewrites the file
/// when the `DECLARED_WRITE` environment variable is set.
///
/// `path` is relative to the calling crate's `CARGO_MANIFEST_DIR`. Expands to
/// `std` code, so the calling crate needs `std`; this crate does not.
///
/// Panics if the file is missing or differs from `generated`.
///
/// ```no_run
/// # let generated = String::new();
/// declared::golden!("examples/mirrors/mirrors.md", &generated);
/// ```
#[macro_export]
macro_rules! golden {
    ($path:expr, $generated:expr) => {{
        let path = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join($path);
        let generated: &str = $generated;
        if ::std::env::var_os("DECLARED_WRITE").is_some() {
            ::std::fs::write(&path, generated)
                .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
        } else {
            let committed = ::std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "failed to read {}: {e}. Run with DECLARED_WRITE=1 to create it.",
                    path.display()
                )
            });
            assert_eq!(
                generated,
                committed,
                "\n{} is stale. Run with DECLARED_WRITE=1 to regenerate it.\n",
                path.display()
            );
        }
    }};
}
