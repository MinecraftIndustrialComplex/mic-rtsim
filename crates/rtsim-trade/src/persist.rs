// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving markets (the phase 1 world's snapshot, docs/overnight-choices-persist.md). Unlike the
//! state hash's [`rtsim_kernel::codec::Encode`] for [`Market`] in `market.rs`, which writes a
//! projection, these write every field, so a market read back is equal to the one saved and
//! continues exactly.
//!
//! The functions are `save_*` and `load_*` rather than trait impls where a type already has a
//! hash encoding (`Market`).

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_settle::Culture;

use crate::goods::GoodId;
use crate::labour::{Costs, Lint, Local, ResourceId};
use crate::line::{Book, Line, Rates};
use crate::market::{GoodState, Input, Market, MarketKind};
use crate::price::PriceCurve;
use crate::tunables::{Ratio, Rules};
use crate::units::Quantum;

fn opt_culture(w: &mut Writer, c: Option<Culture>) {
    match c {
        Some(c) => {
            w.u8(1);
            c.encode(w);
        }
        None => w.u8(0),
    }
}

fn get_opt_culture(r: &mut Reader<'_>) -> codec::Result<Option<Culture>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(Culture::decode(r)?),
        t => return Err(CodecError::Invalid(format!("option tag {t}"))),
    })
}

/// An optional good: `u16::MAX` stands for none (no registry gets that big).
pub fn put_opt_good(w: &mut Writer, g: Option<GoodId>) {
    w.u16(g.map_or(u16::MAX, |g| g.0));
}

pub fn get_opt_good(r: &mut Reader<'_>) -> codec::Result<Option<GoodId>> {
    let v = r.u16()?;
    Ok((v != u16::MAX).then_some(GoodId(v)))
}

fn put_ratio(w: &mut Writer, r: Ratio) {
    w.var_i64(r.num);
    w.var_i64(r.den);
}

fn get_ratio(r: &mut Reader<'_>) -> codec::Result<Ratio> {
    let num = r.var_i64()?;
    let den = r.var_i64()?;
    if den == 0 {
        return Err(CodecError::Invalid("a ratio over zero".into()));
    }
    Ok(Ratio { num, den })
}

fn put_rules(w: &mut Writer, x: &Rules) {
    for v in [
        x.news_resolution_div,
        x.goods_slack_steps,
        x.sticky_div,
        x.step_floor_milli_per_day,
        x.pay_band_div,
    ] {
        w.var_i64(v);
    }
    let flags = [
        x.money_sink,
        x.exporter_band,
        x.absorb_round_up,
        x.ask_at_anchor,
        x.split_among_suppliers,
        x.clamp_to_inflow,
        x.barter_in_flows,
        x.ration,
        x.sticky_goods,
        x.hysteresis,
        x.input_hysteresis,
        x.budget,
    ];
    let mut bits = 0u16;
    for (i, f) in flags.iter().enumerate() {
        bits |= u16::from(*f) << i;
    }
    w.u16(bits);
}

fn get_rules(r: &mut Reader<'_>) -> codec::Result<Rules> {
    let news_resolution_div = r.var_i64()?;
    let goods_slack_steps = r.var_i64()?;
    let sticky_div = r.var_i64()?;
    let step_floor_milli_per_day = r.var_i64()?;
    let pay_band_div = r.var_i64()?;
    let bits = r.u16()?;
    let f = |i: u16| bits & (1 << i) != 0;
    Ok(Rules {
        news_resolution_div,
        goods_slack_steps,
        money_sink: f(0),
        exporter_band: f(1),
        absorb_round_up: f(2),
        ask_at_anchor: f(3),
        split_among_suppliers: f(4),
        clamp_to_inflow: f(5),
        barter_in_flows: f(6),
        sticky_div,
        step_floor_milli_per_day,
        ration: f(7),
        sticky_goods: f(8),
        hysteresis: f(9),
        input_hysteresis: f(10),
        budget: f(11),
        pay_band_div,
    })
}

fn put_line(w: &mut Writer, l: &Line) {
    w.var_i64(l.stock);
    w.var_i64(l.target);
    w.var_i64(l.ceiling);
    let x = &l.rates;
    for v in [x.cap, x.cons, x.inflow, x.outflow, x.slack] {
        w.var_i64(v);
    }
    let b = &l.book;
    for v in [
        b.produced,
        b.consumed,
        b.arrived,
        b.departed,
        b.player_in,
        b.player_out,
        b.genesis,
        b.destroyed,
        b.sunk,
        b.lump_in,
        b.lump_out,
    ] {
        w.var_i64(v);
    }
}

fn get_line(r: &mut Reader<'_>) -> codec::Result<Line> {
    let stock = r.var_i64()?;
    let target = r.var_i64()?;
    let ceiling = r.var_i64()?;
    let rates = Rates {
        cap: r.var_i64()?,
        cons: r.var_i64()?,
        inflow: r.var_i64()?,
        outflow: r.var_i64()?,
        slack: r.var_i64()?,
    };
    let book = Book {
        produced: r.var_i64()?,
        consumed: r.var_i64()?,
        arrived: r.var_i64()?,
        departed: r.var_i64()?,
        player_in: r.var_i64()?,
        player_out: r.var_i64()?,
        genesis: r.var_i64()?,
        destroyed: r.var_i64()?,
        sunk: r.var_i64()?,
        lump_in: r.var_i64()?,
        lump_out: r.var_i64()?,
    };
    Ok(Line {
        stock,
        target,
        ceiling,
        rates,
        book,
    })
}

fn put_good_state(w: &mut Writer, g: &GoodState) {
    w.var_i64(g.curve.anchor);
    put_ratio(w, g.curve.r);
    w.var_i64(g.curve.target);
    w.var_i64(g.household);
    w.var_i64(g.nominal_cap);
    w.var_u64(g.inputs.len() as u64);
    for i in &g.inputs {
        w.u16(i.good.0);
        w.var_i64(i.per);
        w.var_i64(i.yield_);
    }
    w.bool(g.used_here);
    w.bool(g.currency);
}

fn get_good_state(r: &mut Reader<'_>) -> codec::Result<GoodState> {
    let curve = PriceCurve {
        anchor: r.var_i64()?,
        r: get_ratio(r)?,
        target: r.var_i64()?,
    };
    let household = r.var_i64()?;
    let nominal_cap = r.var_i64()?;
    let n = r.len_prefix(4)?;
    let mut inputs = Vec::with_capacity(n);
    for _ in 0..n {
        inputs.push(Input {
            good: GoodId(r.u16()?),
            per: r.var_i64()?,
            yield_: r.var_i64()?,
        });
    }
    Ok(GoodState {
        curve,
        household,
        nominal_cap,
        inputs,
        used_here: r.bool()?,
        currency: r.bool()?,
    })
}

/// Every field of a market.
pub fn save_market(w: &mut Writer, m: &Market) {
    w.u64(m.key);
    w.u8(match m.kind {
        MarketKind::Storehouse => 0,
        MarketKind::Base => 1,
    });
    opt_culture(w, m.culture);
    put_opt_good(w, m.currency);
    w.var_u64(m.quantum.len());
    w.var_u64(m.t_quanta);
    w.var_u64(m.lines.len() as u64);
    m.lines.iter().for_each(|l| put_line(w, l));
    w.var_u64(m.goods.len() as u64);
    m.goods.iter().for_each(|g| put_good_state(w, g));
    w.var_u64(m.order.len() as u64);
    m.order.iter().for_each(|g| w.u16(g.0));
    w.var_u64(m.base_prices.len() as u64);
    for p in &m.base_prices {
        match p {
            Some((price, at)) => {
                w.u8(1);
                w.var_i64(*price);
                w.u64(at.0);
            }
            None => w.u8(0),
        }
    }
    put_rules(w, &m.rules);
    w.var_i64(m.owed);
    w.bool(m.cut);
}

pub fn load_market(r: &mut Reader<'_>) -> codec::Result<Market> {
    let key = r.u64()?;
    let kind = match r.u8()? {
        0 => MarketKind::Storehouse,
        1 => MarketKind::Base,
        t => return Err(CodecError::Invalid(format!("market kind {t}"))),
    };
    let culture = get_opt_culture(r)?;
    let currency = get_opt_good(r)?;
    let q = r.var_u64()?;
    if q == 0 {
        return Err(CodecError::Invalid("a quantum of zero ticks".into()));
    }
    let quantum = Quantum::ticks(q);
    let t_quanta = r.var_u64()?;
    let n = r.len_prefix(19)?;
    let lines = (0..n).map(|_| get_line(r)).collect::<codec::Result<_>>()?;
    let n = r.len_prefix(8)?;
    let goods = (0..n)
        .map(|_| get_good_state(r))
        .collect::<codec::Result<_>>()?;
    let n = r.len_prefix(2)?;
    let order = (0..n)
        .map(|_| r.u16().map(GoodId))
        .collect::<codec::Result<_>>()?;
    let n = r.len_prefix(1)?;
    let mut base_prices = Vec::with_capacity(n);
    for _ in 0..n {
        base_prices.push(match r.u8()? {
            0 => None,
            1 => Some((r.var_i64()?, Tick(r.u64()?))),
            t => return Err(CodecError::Invalid(format!("option tag {t}"))),
        });
    }
    let rules = get_rules(r)?;
    Ok(Market {
        key,
        kind,
        culture,
        currency,
        quantum,
        t_quanta,
        lines,
        goods,
        order,
        base_prices,
        rules,
        owed: r.var_i64()?,
        cut: r.bool()?,
    })
}

impl Encode for Costs {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.labour.len() as u64);
        self.labour.iter().for_each(|v| w.f64(*v));
        w.var_u64(self.recipe.len() as u64);
        for r in &self.recipe {
            w.var_u64(r.map_or(0, |i| i as u64 + 1));
        }
        w.var_u64(self.capacity_per_day.len() as u64);
        self.capacity_per_day.iter().for_each(|v| w.var_i64(*v));
    }
}

impl Decode for Costs {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let n = r.len_prefix(8)?;
        let labour = (0..n).map(|_| r.f64()).collect::<codec::Result<_>>()?;
        let n = r.len_prefix(1)?;
        let recipe = (0..n)
            .map(|_| r.var_u64().map(|v| v.checked_sub(1).map(|i| i as usize)))
            .collect::<codec::Result<_>>()?;
        let n = r.len_prefix(1)?;
        let capacity_per_day = (0..n).map(|_| r.var_i64()).collect::<codec::Result<_>>()?;
        Ok(Costs {
            labour,
            recipe,
            capacity_per_day,
        })
    }
}

impl Encode for Local {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.pop);
        w.var_u64(self.resources.len() as u64);
        for (k, v) in &self.resources {
            w.u16(k.0);
            w.var_i64(*v);
        }
    }
}

impl Decode for Local {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let pop = r.u32()?;
        let mut resources = BTreeMap::new();
        for _ in 0..r.len_prefix(3)? {
            resources.insert(ResourceId(r.u16()?), r.var_i64()?);
        }
        Ok(Local { pop, resources })
    }
}

fn put_goods(w: &mut Writer, g: &[GoodId]) {
    w.var_u64(g.len() as u64);
    g.iter().for_each(|g| w.u16(g.0));
}

fn get_goods(r: &mut Reader<'_>) -> codec::Result<Vec<GoodId>> {
    let n = r.len_prefix(2)?;
    (0..n).map(|_| r.u16().map(GoodId)).collect()
}

impl Encode for Lint {
    fn encode(&self, w: &mut Writer) {
        match self {
            Lint::NotProductive { goods } => {
                w.u8(0);
                put_goods(w, goods);
            }
            Lint::NoProducer(g) => {
                w.u8(1);
                w.u16(g.0);
            }
            Lint::NoSink(g) => {
                w.u8(2);
                w.u16(g.0);
            }
            Lint::NoRouteWithoutTrade(g) => {
                w.u8(3);
                w.u16(g.0);
            }
        }
    }
}

impl Decode for Lint {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(match r.u8()? {
            0 => Lint::NotProductive {
                goods: get_goods(r)?,
            },
            1 => Lint::NoProducer(GoodId(r.u16()?)),
            2 => Lint::NoSink(GoodId(r.u16()?)),
            3 => Lint::NoRouteWithoutTrade(GoodId(r.u16()?)),
            t => return Err(CodecError::Invalid(format!("lint {t}"))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::tests::tiny;

    #[test]
    fn a_market_reads_back_equal() {
        let mut m = tiny(17 * crate::MILLI, 5 * crate::MILLI);
        m.base_prices = vec![Some((1_234_567, Tick(99))), None];
        m.owed = -42;
        m.cut = true;
        m.rules.ration = !m.rules.ration;
        let mut w = Writer::new();
        save_market(&mut w, &m);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let back = load_market(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn costs_local_and_lints_read_back_equal() {
        let c = Costs {
            labour: vec![0.25, f64::INFINITY, 1e-9],
            recipe: vec![None, Some(0), Some(7)],
            capacity_per_day: vec![0, -5, 1 << 40],
        };
        assert_eq!(Costs::from_bytes(&c.to_bytes()).unwrap(), c);
        let mut l = Local {
            pop: 12,
            ..Local::default()
        };
        l.resources.insert(ResourceId(3), 4_000);
        assert_eq!(Local::from_bytes(&l.to_bytes()).unwrap(), l);
        for x in [
            Lint::NotProductive {
                goods: vec![GoodId(1), GoodId(4)],
            },
            Lint::NoProducer(GoodId(2)),
            Lint::NoSink(GoodId(3)),
            Lint::NoRouteWithoutTrade(GoodId(5)),
        ] {
            assert_eq!(Lint::from_bytes(&x.to_bytes()).unwrap(), x);
        }
    }
}
