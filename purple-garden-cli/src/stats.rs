use purple_garden_allocators::metric::{MetricAlloc, Metrics};

#[derive(Debug, Default)]
pub struct Phases<A> {
    /// Shared by every phase for memory only needed while it runs
    pub scratch: A,
    pub parse: A,
    pub typecheck: A,
    pub lower: A,
    pub opt: A,
    pub cc: A,
    pub run: A,
}

impl<A> Phases<A> {
    pub fn each_ref(&self) -> Phases<&A> {
        Phases {
            scratch: &self.scratch,
            parse: &self.parse,
            typecheck: &self.typecheck,
            lower: &self.lower,
            opt: &self.opt,
            cc: &self.cc,
            run: &self.run,
        }
    }
}

impl<A> Phases<MetricAlloc<A>> {
    /// Total peak sums the phase peaks, an upper bound of the real one.
    pub fn table(&self) -> String {
        let rows = [
            ("scratch", self.scratch.metrics()),
            ("parse", self.parse.metrics()),
            ("typecheck", self.typecheck.metrics()),
            ("lower ir", self.lower.metrics()),
            ("opt", self.opt.metrics()),
            ("bytecode/jit", self.cc.metrics()),
            ("run", self.run.metrics()),
        ];
        let total = rows.iter().fold(Metrics::default(), |t, (_, m)| Metrics {
            allocs: t.allocs + m.allocs,
            frees: t.frees + m.frees,
            resizes: t.resizes + m.resizes,
            bytes: t.bytes + m.bytes,
            live: t.live + m.live,
            peak: t.peak + m.peak,
        });

        let mut out = format!(
            "{:<12} {:>9} {:>9} {:>9} {:>10} {:>10}\n",
            "phase", "allocs", "frees", "resizes", "bytes", "peak live"
        );
        for (name, m) in rows.iter().chain([&("total", total)]) {
            out += &format!(
                "{:<12} {:>9} {:>9} {:>9} {:>10} {:>10}\n",
                name,
                m.allocs,
                m.frees,
                m.resizes,
                bytes(m.bytes),
                bytes(m.peak)
            );
        }
        out
    }
}

fn bytes(n: usize) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..0x10_0000 => format!("{} KiB", n >> 10),
        _ => format!("{} MiB", n >> 20),
    }
}
