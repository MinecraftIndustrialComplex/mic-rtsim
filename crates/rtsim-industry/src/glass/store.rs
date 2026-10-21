// SPDX-License-Identifier: AGPL-3.0-or-later
//! The graph store: a district's compile is assembled from its NODE, EDGE, RECIPE and DOMAIN
//! records by compile number, and accepted only when IND_GRAPH_END's counts match exactly what
//! arrived (F5, G-B19). A dropped record throws the compile away; Java resends the last compile
//! every `glass.resendTicks` (GB4), and a resend of the compile already held is ignored.
//!
//! **Bounded.** One assembly per district at a time: a record of a newer compile drops the older
//! assembly, a record of an older one is ignored. Record indices beyond [`MAX_RECORDS`] drop the
//! assembly (Java's `glass.maxNodes` keeps real compiles far below it).

use super::records::{GraphDomain, GraphEdge, GraphEnd, GraphNode, GraphRecipe, end_flags};

/// The most records of one kind an assembly holds (a sanity bound, not a tunable).
pub const MAX_RECORDS: u32 = 1 << 20;

/// What an IND_GRAPH_END did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndOutcome {
    /// The counts matched: the compile is complete.
    Complete,
    /// A resend of the compile already held: ignored.
    ResendIgnored,
    /// The counts did not match (a record was dropped), or no records of it arrived: thrown away.
    Dropped,
    /// Older than the compile held: ignored.
    Stale,
    /// The district is not compilable (END's reason); nothing to assemble.
    NotCompilable,
}

/// One compile being received.
#[derive(Clone, Debug, Default)]
pub struct Assembly {
    pub compile: u32,
    pub nodes: Vec<Option<GraphNode>>,
    pub edges: Vec<Option<GraphEdge>>,
    pub recipes: Vec<Option<GraphRecipe>>,
    pub domains: Vec<Option<GraphDomain>>,
    /// Distinct records received.
    pub count: u32,
    /// A record index past [`MAX_RECORDS`] arrived: the assembly cannot complete.
    pub broken: bool,
}

fn put<T>(v: &mut Vec<Option<T>>, i: u32, x: T, count: &mut u32, broken: &mut bool) {
    if i >= MAX_RECORDS {
        *broken = true;
        return;
    }
    let i = i as usize;
    if v.len() <= i {
        v.resize_with(i + 1, || None);
    }
    if v[i].is_none() {
        *count += 1;
    }
    v[i] = Some(x);
}

/// A complete set of records, by index.
pub struct Complete {
    pub end: GraphEnd,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub recipes: Vec<GraphRecipe>,
    pub domains: Vec<GraphDomain>,
}

/// One district's assembly slot.
#[derive(Clone, Debug, Default)]
pub struct Assembler {
    pub asm: Option<Assembly>,
}

impl Assembler {
    /// The assembly for `compile`, or None when the record is older than the one assembling, or
    /// older than or equal to `held` (the compile already accepted).
    fn slot(&mut self, compile: u32, held: Option<u32>) -> Option<&mut Assembly> {
        if held.is_some_and(|h| compile <= h) {
            return None;
        }
        match &self.asm {
            Some(a) if a.compile > compile => return None,
            Some(a) if a.compile == compile => {}
            _ => {
                self.asm = Some(Assembly {
                    compile,
                    ..Assembly::default()
                })
            }
        }
        self.asm.as_mut()
    }

    pub fn node(&mut self, n: GraphNode, held: Option<u32>) {
        if let Some(a) = self.slot(n.head.compile, held) {
            let i = n.head.index;
            put(&mut a.nodes, i, n, &mut a.count, &mut a.broken);
        }
    }

    pub fn edge(&mut self, e: GraphEdge, held: Option<u32>) {
        if let Some(a) = self.slot(e.head.compile, held) {
            let i = e.head.index;
            put(&mut a.edges, i, e, &mut a.count, &mut a.broken);
        }
    }

    pub fn recipe(&mut self, r: GraphRecipe, held: Option<u32>) {
        if let Some(a) = self.slot(r.head.compile, held) {
            let i = r.head.index;
            put(&mut a.recipes, i, r, &mut a.count, &mut a.broken);
        }
    }

    pub fn domain(&mut self, d: GraphDomain, held: Option<u32>) {
        if let Some(a) = self.slot(d.head.compile, held) {
            let i = d.head.index;
            put(&mut a.domains, i, d, &mut a.count, &mut a.broken);
        }
    }

    /// IND_GRAPH_END: the compile, if its counts match exactly. Any assembly of this compile or
    /// older is consumed either way.
    pub fn end(&mut self, end: &GraphEnd, held: Option<u32>) -> (EndOutcome, Option<Complete>) {
        if held == Some(end.compile) && end.flags & end_flags::RESEND != 0 {
            if self.asm.as_ref().is_some_and(|a| a.compile <= end.compile) {
                self.asm = None;
            }
            return (EndOutcome::ResendIgnored, None);
        }
        if held.is_some_and(|h| end.compile < h) {
            return (EndOutcome::Stale, None);
        }
        if end.flags & end_flags::NOT_COMPILABLE != 0 {
            if self.asm.as_ref().is_some_and(|a| a.compile <= end.compile) {
                self.asm = None;
            }
            return (EndOutcome::NotCompilable, None);
        }
        let asm = match self.asm.take() {
            Some(a) if a.compile == end.compile => a,
            Some(a) if a.compile > end.compile => {
                self.asm = Some(a);
                return (EndOutcome::Stale, None);
            }
            _ => {
                // Nothing of it arrived (or only of an older compile): an empty compile is valid
                // only when END says so.
                Assembly {
                    compile: end.compile,
                    ..Assembly::default()
                }
            }
        };
        if held == Some(end.compile) {
            // The same compile again without the resend flag: still nothing new.
            return (EndOutcome::ResendIgnored, None);
        }
        let exact = |len: usize, want: u32, v: &dyn Fn(usize) -> bool| {
            len == want as usize && (0..len).all(v)
        };
        let ok = !asm.broken
            && asm.count == end.records
            && end.records == end.nodes + end.edges + end.recipes + end.domains
            && exact(asm.nodes.len(), end.nodes, &|i| asm.nodes[i].is_some())
            && exact(asm.edges.len(), end.edges, &|i| asm.edges[i].is_some())
            && exact(asm.recipes.len(), end.recipes, &|i| {
                asm.recipes[i].is_some()
            })
            && exact(asm.domains.len(), end.domains, &|i| {
                asm.domains[i].is_some()
            });
        if !ok {
            return (EndOutcome::Dropped, None);
        }
        let unwrap = |v: Vec<Option<_>>| v.into_iter().map(|x| x.expect("checked")).collect();
        (
            EndOutcome::Complete,
            Some(Complete {
                end: *end,
                nodes: unwrap(asm.nodes),
                edges: asm.edges.into_iter().map(|x| x.expect("checked")).collect(),
                recipes: asm
                    .recipes
                    .into_iter()
                    .map(|x| x.expect("checked"))
                    .collect(),
                domains: asm
                    .domains
                    .into_iter()
                    .map(|x| x.expect("checked"))
                    .collect(),
            }),
        )
    }
}
