// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared kinetic networks (SEC-1 slice S5-2, SECTIONS.md §2.6): a Create network whose blocks lie
//! in several districts' chunks reaches the engine as one SHARED domain per district, a *part*
//! (`records::domain_flags::SHARED`). This registry joins the parts into networks and gives each
//! part what the rest of its network gives it ([`NetInput`]), so a part solves as the whole
//! network runs: a boiler in one district drives a press line in the next.
//!
//! **Derived, never saved.** The parts are the districts' compiled graphs, which their glass pages
//! hold; the registry is rebuilt from them on a compile accepted or dropped, a district removed,
//! and a restore. Who gives what (the owners' source rows) is read when an input is computed: the
//! check solves' own (the live view) or the replays' (the engine's sim view).
//!
//! **Identity.** Create's network id is a hint only (it changes when another source takes over):
//! parts of one dimension join when they list a source at the same position (union-find). The
//! network's frame is its anchor, the lowest source position every part lists with a known ratio;
//! each part's speeds go through it.
//!
//! **Status.** A network is coupled when it holds together: at most `cluster_districts_max` parts,
//! one per district, every peer a part names has one, an anchor, every source tabulated by some
//! part, and every speed an owner's source can give a speed row in every part. Otherwise each of
//! its parts is decoupled: it runs at its compile row, its members bounded (S1's rule).

use std::collections::{BTreeMap, BTreeSet};

use super::records::GraphDomain;
use super::solve::NetInput;
use crate::ids::DistrictId;

/// One part: a district's SHARED domain, by its compile's domain index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartId {
    pub district: DistrictId,
    pub domain: u32,
}

/// Whether a network's parts evaluate together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Coupled,
    /// A part is missing or the parts do not agree (`Net::why` says how).
    Incomplete,
    /// More parts than `cluster_districts_max`.
    PastMax,
}

impl Status {
    pub fn words(self) -> &'static str {
        match self {
            Status::Coupled => "coupled",
            Status::Incomplete => "incomplete",
            Status::PastMax => "too many districts",
        }
    }
}

/// A part in its network.
#[derive(Clone, Debug)]
struct Member {
    id: PartId,
    /// The anchor's ratio in this part's frame (`f_p`; 1 when not coupled).
    frame: f64,
    /// Stress per RPM of this part's reference speed: its own members, and its tail.
    k_own: f64,
    k_tail: f64,
    sources: usize,
}

/// A source position some part lists.
#[derive(Clone, Debug)]
struct Pos {
    /// The part whose rows count (an index into `Net::parts`): the lowest district that drives it,
    /// else the lowest that tabulates it.
    owner: usize,
    /// Its index in the owner's domain's sources.
    src: u32,
    /// Its ratio in the owner's frame (None: unknown or zero).
    ratio: Option<f64>,
    /// The owner's rows: (the source's own speed, its capacity).
    rows: Vec<(f64, f64)>,
    /// Per part: its index in that part's sources, if it lists it.
    at: Vec<Option<u32>>,
}

/// One network: its parts (ascending), its sources, and its status.
#[derive(Clone, Debug)]
pub struct Net {
    pub status: Status,
    /// Why it is not coupled, for the log.
    pub why: &'static str,
    parts: Vec<Member>,
    positions: Vec<Pos>,
    /// The part that counts the tail (members in no district's chunks): the lowest with one.
    tail: Option<usize>,
}

impl Net {
    /// Its parts, ascending.
    pub fn parts(&self) -> impl Iterator<Item = PartId> + '_ {
        self.parts.iter().map(|m| m.id)
    }

    /// Its districts, ascending.
    pub fn districts(&self) -> impl Iterator<Item = DistrictId> + '_ {
        self.parts.iter().map(|m| m.id.district)
    }

    fn index(&self, id: PartId) -> Option<usize> {
        self.parts.binary_search_by(|m| m.id.cmp(&id)).ok()
    }

    /// Part `id`'s input, with each owner's rows (by its domain's source index) from `rows`; None
    /// when the network is not coupled (the part is decoupled).
    pub fn input(
        &self,
        id: PartId,
        rows: &mut dyn FnMut(PartId) -> Vec<Option<u32>>,
    ) -> Option<NetInput> {
        if self.status != Status::Coupled {
            return None;
        }
        let p = self.index(id)?;
        let me = &self.parts[p];
        let mut inp = NetInput {
            own: vec![false; me.sources],
            tail: self.tail == Some(p),
            ..NetInput::default()
        };
        let mut cache: BTreeMap<usize, Vec<Option<u32>>> = BTreeMap::new();
        for pos in &self.positions {
            if pos.owner == p {
                if let Some(i) = pos.at[p] {
                    inp.own[i as usize] = true;
                }
                continue;
            }
            let o = &self.parts[pos.owner];
            let owner_rows = cache.entry(pos.owner).or_insert_with(|| rows(o.id));
            let Some(&Some(r)) = owner_rows.get(pos.src as usize) else {
                continue;
            };
            let Some(&(sp, cap)) = pos.rows.get(r as usize) else {
                continue;
            };
            inp.cap += cap;
            let Some(ratio) = pos.ratio else {
                continue;
            };
            if sp == 0.0 {
                continue;
            }
            // The owner's reference turns at sp / ratio; the anchor at f_o times that; this
            // part's reference at the anchor's over f_p.
            let v = sp * o.frame / (ratio * me.frame);
            if v > 0.0 {
                inp.fwd = inp.fwd.max(v);
            } else {
                inp.back = inp.back.max(-v);
            }
        }
        inp.stress = self.others_stress(p);
        Some(inp)
    }

    /// The other parts' stress per RPM of part `p`'s reference speed.
    fn others_stress(&self, p: usize) -> f64 {
        let me = &self.parts[p];
        let mut stress = 0.0f64;
        for (q, m) in self.parts.iter().enumerate() {
            if q == p {
                continue;
            }
            let k = m.k_own + if self.tail == Some(q) { m.k_tail } else { 0.0 };
            stress += k * (me.frame / m.frame).abs();
        }
        stress
    }

    /// Part `id`'s stress inputs alone (no rows): the other parts' stress per RPM and whether it
    /// counts its tail, for the domain check (SECTIONS.md §3.6). None when not coupled.
    pub fn stress_of(&self, id: PartId) -> Option<(f64, bool)> {
        if self.status != Status::Coupled {
            return None;
        }
        let p = self.index(id)?;
        Some((self.others_stress(p), self.tail == Some(p)))
    }
}

/// Every network the districts' compiles make.
#[derive(Clone, Debug, Default)]
pub struct Nets {
    nets: Vec<Net>,
    of: BTreeMap<PartId, usize>,
    /// Per district: the networks it has a part in.
    by_district: BTreeMap<DistrictId, BTreeSet<usize>>,
}

/// Stress per RPM of `members`: `count × |ratio| × impact`, as `solve::domain_stress` sums it.
fn k_of(members: &[super::records::Member]) -> f64 {
    let mut k = 0.0f64;
    for m in members {
        let ratio = f64::from(m.ratio_num) / f64::from(m.ratio_den.max(1));
        k += f64::from(m.count) * ratio.abs() * m.impact;
    }
    k
}

fn find(uf: &mut [usize], mut x: usize) -> usize {
    while uf[x] != x {
        uf[x] = uf[uf[x]];
        x = uf[x];
    }
    x
}

impl Nets {
    /// Builds the registry from every SHARED domain: (part, its district's dimension, the domain).
    /// `max` is `cluster_districts_max`.
    pub fn build(parts: &[(PartId, u32, &GraphDomain)], max: usize) -> Nets {
        let mut sorted: Vec<usize> = (0..parts.len()).collect();
        sorted.sort_by_key(|&i| parts[i].0);
        let parts: Vec<(PartId, u32, &GraphDomain)> = sorted.iter().map(|&i| parts[i]).collect();
        let n = parts.len();
        let mut uf: Vec<usize> = (0..n).collect();
        let mut first: BTreeMap<(u32, i64), usize> = BTreeMap::new();
        for (i, (_, dim, dom)) in parts.iter().enumerate() {
            for s in &dom.sources {
                match first.get(&(*dim, s.pos)) {
                    Some(&j) => {
                        let (a, b) = (find(&mut uf, i), find(&mut uf, j));
                        if a != b {
                            uf[a.max(b)] = a.min(b);
                        }
                    }
                    None => {
                        first.insert((*dim, s.pos), i);
                    }
                }
            }
        }
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..n {
            let r = find(&mut uf, i);
            groups.entry(r).or_default().push(i);
        }
        let mut out = Nets::default();
        for members in groups.into_values() {
            let net = Self::net(&parts, &members, max);
            let ni = out.nets.len();
            for m in &net.parts {
                out.of.insert(m.id, ni);
                out.by_district.entry(m.id.district).or_default().insert(ni);
            }
            out.nets.push(net);
        }
        out
    }

    /// One network of `members` (indices into `parts`, ascending).
    fn net(parts: &[(PartId, u32, &GraphDomain)], members: &[usize], max: usize) -> Net {
        let doms: Vec<&GraphDomain> = members.iter().map(|&i| parts[i].2).collect();
        let mut net = Net {
            status: Status::Coupled,
            why: "",
            parts: members
                .iter()
                .map(|&i| {
                    let (id, _, dom) = parts[i];
                    let own = dom.members.len().saturating_sub(dom.foreign as usize);
                    Member {
                        id,
                        frame: 1.0,
                        k_own: k_of(&dom.members[..own]),
                        k_tail: k_of(&dom.members[own..]),
                        sources: dom.sources.len(),
                    }
                })
                .collect(),
            positions: Vec::new(),
            tail: None,
        };
        net.tail = doms.iter().position(|d| d.foreign > 0);
        let fail = |net: &mut Net, s: Status, why: &'static str| {
            net.status = s;
            net.why = why;
        };
        if net.parts.len() > max {
            fail(
                &mut net,
                Status::PastMax,
                "more districts than clusterDistrictsMax",
            );
            return net;
        }
        let districts: BTreeSet<DistrictId> = net.parts.iter().map(|m| m.id.district).collect();
        if districts.len() < net.parts.len() {
            fail(&mut net, Status::Incomplete, "two parts in one district");
            return net;
        }
        if doms
            .iter()
            .any(|d| d.peers.iter().any(|p| !districts.contains(p)))
        {
            fail(&mut net, Status::Incomplete, "a district on it has no part");
            return net;
        }
        // Every position, with each part's index of it.
        let mut at: BTreeMap<i64, Vec<Option<u32>>> = BTreeMap::new();
        for (p, d) in doms.iter().enumerate() {
            for (i, s) in d.sources.iter().enumerate() {
                at.entry(s.pos).or_insert_with(|| vec![None; doms.len()])[p] = Some(i as u32);
            }
        }
        let known = |p: usize, i: u32| -> Option<f64> {
            doms[p].sources[i as usize].ratio().filter(|&r| r != 0.0)
        };
        let anchor = at.iter().find(|(_, idx)| {
            idx.iter()
                .enumerate()
                .all(|(p, i)| i.is_some_and(|i| known(p, i).is_some()))
        });
        let Some((_, aidx)) = anchor else {
            fail(&mut net, Status::Incomplete, "no source every part places");
            return net;
        };
        for (p, m) in net.parts.iter_mut().enumerate() {
            m.frame = known(p, aidx[p].expect("listed")).expect("known");
        }
        for (_, idx) in at {
            let tab = |p: &usize| {
                idx[*p].is_some_and(|i| !doms[*p].sources[i as usize].levels.is_empty())
            };
            let driven =
                |p: &usize| idx[*p].is_some_and(|i| doms[*p].sources[i as usize].driver.is_some());
            let owner = (0..doms.len())
                .filter(|p| tab(p) && driven(p))
                .chain((0..doms.len()).filter(tab))
                .next();
            let Some(owner) = owner else {
                fail(&mut net, Status::Incomplete, "a source no part tabulates");
                return net;
            };
            let src = idx[owner].expect("tabulated");
            let s = &doms[owner].sources[src as usize];
            net.positions.push(Pos {
                owner,
                src,
                ratio: s.ratio().filter(|&r| r != 0.0),
                rows: s.levels.iter().map(|l| (l.speed(), l.capacity)).collect(),
                at: idx,
            });
        }
        // Every speed an owner's source gives has a speed row in every other part (Java adds every
        // engine speed of a foreign shaft; a part built before a source changed may lack one).
        for pos in &net.positions {
            let Some(ratio) = pos.ratio else { continue };
            let fo = net.parts[pos.owner].frame;
            for &(sp, _) in &pos.rows {
                if sp == 0.0 {
                    continue;
                }
                for (p, d) in doms.iter().enumerate() {
                    if p == pos.owner {
                        continue;
                    }
                    let v = sp * fo / (ratio * net.parts[p].frame);
                    if super::solve::speed_row_at(d, v).is_none() {
                        net.status = Status::Incomplete;
                        net.why = "a speed the network can run at that a part has no row for";
                        return net;
                    }
                }
            }
        }
        net
    }

    /// The network part `id` is in.
    pub fn net_of(&self, id: PartId) -> Option<&Net> {
        self.of.get(&id).map(|&i| &self.nets[i])
    }

    /// The index of the network part `id` is in.
    pub fn index_of(&self, id: PartId) -> Option<usize> {
        self.of.get(&id).copied()
    }

    pub fn get(&self, i: usize) -> Option<&Net> {
        self.nets.get(i)
    }

    pub fn len(&self) -> usize {
        self.nets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nets.is_empty()
    }

    /// The networks district `d` has a part in.
    pub fn of_district(&self, d: DistrictId) -> impl Iterator<Item = usize> + '_ {
        self.by_district.get(&d).into_iter().flatten().copied()
    }

    /// District `d`'s parts, ascending.
    pub fn parts_of(&self, d: DistrictId) -> impl Iterator<Item = PartId> + '_ {
        self.of
            .range(
                PartId {
                    district: d,
                    domain: 0,
                }..=PartId {
                    district: d,
                    domain: u32::MAX,
                },
            )
            .map(|(&id, _)| id)
    }

    /// Does district `d` have a part in some network?
    pub fn has(&self, d: DistrictId) -> bool {
        self.by_district.contains_key(&d)
    }
}
