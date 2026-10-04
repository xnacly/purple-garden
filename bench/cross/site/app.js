// Renders the benchmark history published to gh-pages.
//
// Data layout (written by `purple-garden-bench`):
//   data/index.json   [{ file, sha, date, run_unix, ref }]  sorted by run_unix
//   data/<sha>.json   one report: { sha, date, runtimes, results: [...] }
//
// Four charts on one screen:
//   per-bench — latest run, CPU time per benchmark, one bar per runtime,
//               either absolute ms or relative to garden (pg = 1×)
//   time      — history: sum of per-workload mean wall time per runtime
//   cpu       — history: sum of per-workload user+system time per runtime
//   memory    — history: mean of per-workload peak RSS per runtime (summing
//               peak RSS of separate processes is not a physical quantity)

const RUNTIMES = ["garden", "bun", "luajit", "python3"];
const COLORS = {
  garden: "#a78bfa",
  bun: "#fb923c",
  luajit: "#38bdf8",
  python3: "#facc15",
};

const sum = (xs) => xs.reduce((a, b) => a + b, 0);
const mean = (xs) => sum(xs) / xs.length;

const METRICS = [
  {
    key: "time",
    label: "total wall time (ms)",
    sub: "sum of per-workload mean wall time",
    unit: "ms",
    get: (m) => m.time_ms.mean,
    agg: sum,
  },
  {
    key: "cpu",
    label: "total CPU time (ms)",
    sub: "sum of per-workload user + system time",
    unit: "ms",
    get: (m) => m.cpu_ms,
    agg: sum,
  },
  {
    key: "memory",
    label: "mean peak RSS (MiB)",
    sub: "mean of per-workload peak resident set size",
    unit: "MiB",
    get: (m) => m.memory_mb,
    agg: mean,
  },
];

const state = {
  log: true,
  perBench: "relative",
  enabled: new Set(RUNTIMES),
  reports: [],
  charts: {},
};

Chart.defaults.color = "#9a9ab0";
Chart.defaults.borderColor = "#2a2a36";
Chart.defaults.font.family = getComputedStyle(document.body).fontFamily;

async function load() {
  const index = await (await fetch("data/index.json")).json();
  const reports = await Promise.all(
    index.map(async (e) => {
      const r = await (await fetch(`data/${e.file}`)).json();
      r.ref = r.ref ?? e.ref ?? "";
      return r;
    }),
  );
  reports.sort((a, b) => a.run_unix - b.run_unix);
  state.reports = reports;
}

const short = (sha) => sha.slice(0, 7);
const fmt = (v) => v.toFixed(v >= 100 ? 0 : v >= 10 ? 1 : 2);

/// Aggregate one metric for one runtime in one report, or null if that
/// runtime has no usable values (skipped runtime, --no-memory, ...).
function aggregate(report, runtime, metric) {
  const vals = report.results
    .filter((m) => m.runtime === runtime)
    .map(metric.get)
    .filter((v) => typeof v === "number");
  return vals.length ? { value: metric.agg(vals), n: vals.length } : null;
}

function yScale(title) {
  return {
    type: state.log ? "logarithmic" : "linear",
    title: { display: true, text: title },
    grid: { color: "#2a2a36" },
    ticks: { callback: (v) => (Number.isInteger(v) || v < 1 ? v : v.toFixed(1)) },
  };
}

function renderControls() {
  const toggles = document.getElementById("runtime-toggles");
  for (const rt of RUNTIMES) {
    const l = document.createElement("label");
    l.className = "rt";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = true;
    cb.onchange = () => {
      cb.checked ? state.enabled.add(rt) : state.enabled.delete(rt);
      render();
    };
    const swatch = document.createElement("i");
    swatch.style.background = COLORS[rt];
    l.append(cb, swatch, document.createTextNode(rt));
    toggles.appendChild(l);
  }
  document.getElementById("log-toggle").onchange = (e) => {
    state.log = e.target.checked;
    render();
  };
  for (const radio of document.querySelectorAll('input[name="per-bench-mode"]')) {
    radio.onchange = () => {
      state.perBench = radio.value;
      render();
    };
  }
}

function renderMeta() {
  const latest = state.reports.at(-1);
  const versions = Object.entries(latest.runtimes)
    .map(([k, v]) => `${k}: ${v.split(" -- ")[0]}`)
    .join(" · ");
  const n = state.reports.length;
  document.getElementById("meta").innerHTML =
    `${n} benchmarked commit${n === 1 ? "" : "s"} · ` +
    `latest <code>${short(latest.sha)}</code> (${latest.date}` +
    `${latest.ref ? `, ${latest.ref}` : ""}) · ${latest.runner} · ` +
    `${latest.runs} runs, ${latest.warmup} warmup<br>${versions}`;
}

/// Latest run, CPU time per benchmark. Relative mode divides every runtime
/// by garden's CPU time for that benchmark and draws pg as a dashed 1× line
/// instead of a column of identical bars.
function renderPerBench() {
  const latest = state.reports.at(-1);
  const workloads = [...new Set(latest.results.map((m) => m.workload))].sort();
  const cpu = (w, rt) =>
    latest.results.find((m) => m.workload === w && m.runtime === rt)?.cpu_ms ?? null;
  const relative = state.perBench === "relative";
  const runtimes = RUNTIMES.filter(
    (r) => state.enabled.has(r) && !(relative && r === "garden"),
  );

  const datasets = runtimes.map((rt) => ({
    type: "bar",
    label: rt,
    data: workloads.map((w) => {
      const v = cpu(w, rt);
      if (v == null) return null;
      if (!relative) return v;
      const g = cpu(w, "garden");
      return g ? v / g : null;
    }),
    backgroundColor: COLORS[rt],
    borderRadius: 3,
  }));
  if (relative) {
    datasets.push({
      type: "line",
      label: "garden = 1×",
      data: workloads.map(() => 1),
      borderColor: COLORS.garden,
      borderDash: [5, 4],
      borderWidth: 2,
      pointRadius: 0,
    });
  }

  document.getElementById("per-bench-sub").textContent = relative
    ? `${short(latest.sha)} — each runtime's CPU time divided by garden's; below the dashed line is faster than pg`
    : `${short(latest.sha)} — CPU time in ms, lower is better`;

  state.charts.perBench?.destroy();
  state.charts.perBench = new Chart(document.getElementById("per-bench-canvas"), {
    type: "bar",
    data: { labels: workloads, datasets },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        y: yScale(relative ? "× garden CPU time" : "CPU time (ms)"),
        x: { grid: { display: false } },
      },
      plugins: {
        tooltip: {
          callbacks: {
            label: (c) => {
              if (c.dataset.type === "line") return "";
              const rt = c.dataset.label;
              return relative
                ? ` ${rt}: ${fmt(c.parsed.y)}× pg (${fmt(cpu(c.label, rt))} ms)`
                : ` ${rt}: ${fmt(c.parsed.y)} ms`;
            },
          },
        },
      },
    },
  });
}

function renderMetric(metric) {
  const runtimes = RUNTIMES.filter((r) => state.enabled.has(r));
  const series = Object.fromEntries(
    runtimes.map((rt) => [rt, state.reports.map((r) => aggregate(r, rt, metric))]),
  );

  const latest = runtimes
    .map((rt) => {
      const a = series[rt].at(-1);
      return a ? `${rt} ${fmt(a.value)}` : null;
    })
    .filter(Boolean)
    .join(" · ");
  document.getElementById(`${metric.key}-sub`).textContent =
    `${latest} ${metric.unit} — ${metric.sub}`;

  state.charts[metric.key]?.destroy();
  state.charts[metric.key] = new Chart(document.getElementById(`${metric.key}-canvas`), {
    type: "line",
    data: {
      labels: state.reports.map((r) => short(r.sha)),
      datasets: runtimes.map((rt) => ({
        label: rt,
        data: series[rt].map((a) => a?.value ?? null),
        borderColor: COLORS[rt],
        backgroundColor: COLORS[rt],
        pointRadius: state.reports.length > 40 ? 0 : 3,
        tension: 0.15,
        spanGaps: true,
      })),
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: { mode: "index", intersect: false },
      scales: {
        y: yScale(metric.label),
        x: { ticks: { maxTicksLimit: 12, autoSkip: true }, grid: { display: false } },
      },
      plugins: {
        tooltip: {
          callbacks: {
            title: (items) => {
              const r = state.reports[items[0].dataIndex];
              return `${short(r.sha)} · ${r.date}${r.ref ? ` · ${r.ref}` : ""}`;
            },
            label: (c) => {
              const a = series[c.dataset.label][c.dataIndex];
              return a ? ` ${c.dataset.label}: ${fmt(a.value)} ${metric.unit} (${a.n} workloads)` : "";
            },
          },
        },
      },
      onClick: (_, items) => {
        if (!items.length) return;
        const r = state.reports[items[0].index];
        window.open(`https://github.com/xnacly/purple-garden/commit/${r.sha}`, "_blank");
      },
    },
  });
}

function render() {
  renderMeta();
  renderPerBench();
  for (const metric of METRICS) renderMetric(metric);
}

(async () => {
  try {
    await load();
    if (!state.reports.length) throw new Error("index.json is empty");
    document.getElementById("status").hidden = true;
    document.getElementById("content").hidden = false;
    renderControls();
    render();
  } catch (e) {
    document.getElementById("status").textContent = `failed to load benchmark data: ${e.message}`;
    document.getElementById("meta").textContent = "";
  }
})();
