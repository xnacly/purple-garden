use std::{collections::HashMap, fs};

macro_rules! livesetvisprintln {
        ($fmt:literal, $($value:expr),*) => {
            {
                println!("[livesetvis] {}", format_args!($fmt, $($value),*));
            }
        };
        ($fmt:literal) => {
            {
                println!("[livesetvis] {}", $fmt);
            }
        };
    }

fn parse_live_intervals(raw: &str) -> HashMap<u32, (u32, u32)> {
    raw.lines()
        .filter_map(|l| {
            // each line is: <%v>: (<def>,<last_use>)
            let (lhs, rhs) = l.split_once(":")?;
            let v = lhs.parse().ok()?;
            let (def, last_use) = rhs.trim_ascii().split_once(',')?;
            let (def, last_use) = (
                def[1..].parse().ok()?,
                last_use[..last_use.len() - 1].parse().ok()?,
            );
            Some((v, (def, last_use)))
        })
        .collect()
}

fn visualize(parsed: HashMap<u32, (u32, u32)>) {
    let max_end = parsed.values().map(|(_, end)| *end).max().unwrap_or(0);
    let mut entries: Vec<_> = parsed.iter().collect();
    entries.sort_by_key(|(id, _)| *id);

    print!("   ");
    for i in 0..max_end {
        print!("{:2} ", i);
    }
    println!();

    for (id, (start, end)) in entries {
        print!("v{:<3}", id);

        for i in 0..max_end {
            if i >= *start && i < *end {
                print!("X  ");
            } else {
                print!("   ");
            }
        }

        println!("  [{}..{})", start, end);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_name = std::env::args().nth(1).unwrap();
    let bytes = fs::read(file_name)?;
    let s = str::from_utf8(&bytes.trim_ascii())?;
    let parsed = parse_live_intervals(s);
    visualize(parsed);
    Ok(())
}
