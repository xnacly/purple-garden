//! Register allocation as lanes: a lane per register or slot, a bar per value spanning its live
//! interval in the lane it was allocated to.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Location;

/// A value's bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bar {
    pub value: u32,
    /// The rows of its definition and last use.
    pub rows: (u32, u32),
}

/// The lanes of `locations` in register order, slots last, each with its bars by start.
#[must_use]
pub fn lanes(
    intervals: &[Option<(u32, u32)>],
    locations: &[Option<Location>],
) -> Vec<(Location, Vec<Bar>)> {
    // registers by their number within their class, gprs before fprs, then slots
    let key = |l: &Location| match l {
        Location::Reg(name) => {
            let digits: String = name.chars().filter(char::is_ascii_digit).collect();
            let class = u8::from(name.starts_with("xmm"));
            (0u8, class, digits.parse::<u32>().unwrap_or(0), name.clone())
        }
        Location::Slot(slot) => (1, 0, u32::from(*slot), String::new()),
    };
    let mut lanes: BTreeMap<_, (Location, Vec<Bar>)> = BTreeMap::new();
    for (value, rows) in spans(intervals) {
        let Some(Some(location)) = locations.get(value) else {
            continue;
        };
        lanes
            .entry(key(location))
            .or_insert_with(|| (location.clone(), Vec::new()))
            .1
            .push(Bar {
                value: value as u32,
                rows,
            });
    }
    lanes
        .into_values()
        .map(|(l, mut bars)| {
            bars.sort_by_key(|b| b.rows);
            (l, bars)
        })
        .collect()
}

/// `(value, (def, last))` of the intervals a bar can span. A value whose last use comes before its
/// definition has none: `Func::live_set_into` records that for a use its definition doesn't
/// dominate, in a block laid out before the definition's.
fn spans(intervals: &[Option<(u32, u32)>]) -> impl Iterator<Item = (usize, (u32, u32))> + '_ {
    intervals
        .iter()
        .enumerate()
        .filter_map(|(value, interval)| {
            interval
                .filter(|(def, last)| def <= last)
                .map(|i| (value, i))
        })
}

/// How many values are live at every row.
#[must_use]
pub fn pressure(intervals: &[Option<(u32, u32)>]) -> Vec<u32> {
    let end = spans(intervals)
        .map(|(_, (_, last))| last + 1)
        .max()
        .unwrap_or(0);
    let mut live = vec![0; end as usize];
    for (_, (def, last)) in spans(intervals) {
        for row in &mut live[def as usize..=last as usize] {
            *row += 1;
        }
    }
    live
}

#[cfg(test)]
mod tests {
    use crate::Location;

    use super::{Bar, lanes, pressure};

    /// A use its definition doesn't dominate, laid out before it, ends the interval before it
    /// starts: no bar, no pressure, no panic.
    #[test]
    fn intervals_ending_before_their_definition_draw_nothing() {
        let intervals = [Some((0, 2)), Some((273, 222)), None];
        let locations = [reg("r0"), reg("r0"), reg("r1")];
        assert_eq!(pressure(&intervals), [1, 1, 1]);
        let lanes = lanes(&intervals, &locations);
        assert_eq!(lanes.len(), 1);
        assert_eq!(
            lanes[0].1,
            [Bar {
                value: 0,
                rows: (0, 2)
            }]
        );
    }

    fn reg(name: &str) -> Option<Location> {
        Some(Location::Reg(name.to_string()))
    }

    #[test]
    fn lanes_order_registers_numerically_then_slots() {
        let intervals = [
            Some((0, 2)),
            Some((1, 4)),
            None,
            Some((3, 5)),
            Some((2, 2)),
            Some((0, 1)),
        ];
        let locations = [
            reg("r10"),
            reg("r2"),
            reg("r3"),
            Some(Location::Slot(0)),
            reg("r2"),
            reg("xmm1"),
        ];
        let lanes = lanes(&intervals, &locations);
        let names: Vec<String> = lanes.iter().map(|(l, _)| l.to_string()).collect();
        assert_eq!(
            names,
            ["r2", "r10", "xmm1", "s0"],
            "r3 holds a value without interval"
        );
        assert_eq!(
            lanes[0].1,
            [
                Bar {
                    value: 1,
                    rows: (1, 4)
                },
                Bar {
                    value: 4,
                    rows: (2, 2)
                }
            ]
        );
    }

    #[test]
    fn x86_registers_sort_by_name_without_digits() {
        let intervals = [Some((0, 1)), Some((0, 1)), Some((0, 1))];
        let locations = [reg("rsi"), reg("r8"), reg("rax")];
        let names: Vec<String> = lanes(&intervals, &locations)
            .iter()
            .map(|(l, _)| l.to_string())
            .collect();
        assert_eq!(names, ["rax", "rsi", "r8"]);
    }

    #[test]
    fn pressure_counts_overlapping_intervals() {
        assert_eq!(
            pressure(&[Some((0, 2)), None, Some((1, 1)), Some((2, 3))]),
            [1, 2, 2, 1]
        );
        assert!(pressure(&[]).is_empty());
        assert!(pressure(&[None]).is_empty());
    }
}
