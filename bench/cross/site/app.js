// Renders the benchmark history published to gh-pages.
//
// Data layout (written by `purple-garden-bench`):
//   data/index.json   [{ file, sha, date, run_unix, ref }]  sorted by run_unix
//   data/<sha>.json   one report: { sha, date, runtimes, results: [...] }
//
// Charts, each switchable to a table of the same data:
//   per-bench — latest run, CPU time per benchmark in ms, one bar per runtime
//   history   — one chart per benchmark, CPU time in ms per commit, one line per runtime

const RUNTIMES = ["garden", "bun", "luajit", "python3"];
const COLORS = {
  garden: "#a78bfa",
  bun: "#fb923c",
  luajit: "#38bdf8",
  python3: "#facc15",
};

const state = {
  log: true,
  enabled: new Set(RUNTIMES),
  reports: [],
  history: [],
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

/// CPU time of one workload on one runtime, null if the runtime skipped it.
const cpu = (report, workload, runtime) =>
  report.results.find((m) => m.workload === workload && m.runtime === runtime)?.cpu_ms ?? null;

const workloads = (report) => [...new Set(report.results.map((m) => m.workload))].sort();

function yScale(title) {
  return {
    type: state.log ? "logarithmic" : "linear",
    title: { display: true, text: title },
    grid: { color: "#2a2a36" },
    ticks: { callback: (v) => (Number.isInteger(v) || v < 1 ? v : v.toFixed(1)) },
  };
}

function renderTable(id, head, rows) {
  const table = document.getElementById(id);
  const tr = (cells, tag) =>
    `<tr>${cells.map((c) => `<${tag}>${c ?? "–"}</${tag}>`).join("")}</tr>`;
  table.innerHTML =
    `<thead>${tr(head, "th")}</thead>` +
    `<tbody>${rows.map((r) => tr(r, "td")).join("")}</tbody>`;
}

function renderHistoryPanels() {
  const history = document.getElementById("history");
  const all = [...new Set(state.reports.flatMap(workloads))].sort();
  for (const w of all) {
    history.insertAdjacentHTML(
      "beforeend",
      `<section class="panel">
        <h2>${w} <small id="${w}-sub"></small></h2>
        <label class="rt"><input type="checkbox" class="table-toggle" /> table</label>
        <div class="chart" style="position: relative; height: 320px">
          <canvas id="${w}-canvas"></canvas>
        </div>
        <table id="${w}-table" hidden></table>
      </section>`,
    );
  }
  return all;
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
  for (const toggle of document.querySelectorAll(".table-toggle")) {
    const panel = toggle.closest("section");
    toggle.onchange = () => {
      panel.querySelector(".chart").hidden = toggle.checked;
      panel.querySelector("table").hidden = !toggle.checked;
    };
  }
  document.getElementById("log-toggle").onchange = (e) => {
    state.log = e.target.checked;
    render();
  };
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

/// Latest run, CPU time per benchmark.
function renderPerBench() {
  const latest = state.reports.at(-1);
  const benches = workloads(latest);
  const runtimes = RUNTIMES.filter((r) => state.enabled.has(r));

  const datasets = runtimes.map((rt) => ({
    label: rt,
    data: benches.map((w) => cpu(latest, w, rt)),
    backgroundColor: COLORS[rt],
    borderRadius: 3,
  }));

  document.getElementById("per-bench-sub").textContent =
    `${short(latest.sha)} — CPU time in ms, lower is better`;

  renderTable(
    "per-bench-table",
    ["workload", ...runtimes.map((rt) => `${rt} ms`)],
    benches.map((w) => [
      w,
      ...runtimes.map((rt) => {
        const v = cpu(latest, w, rt);
        return v == null ? null : fmt(v);
      }),
    ]),
  );

  state.charts.perBench?.destroy();
  state.charts.perBench = new Chart(document.getElementById("per-bench-canvas"), {
    type: "bar",
    data: { labels: benches, datasets },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        y: yScale("CPU time (ms)"),
        x: { grid: { display: false } },
      },
      plugins: {
        tooltip: {
          callbacks: {
            label: (c) => ` ${c.dataset.label}: ${fmt(c.parsed.y)} ms`,
          },
        },
      },
    },
  });
}

/// One benchmark across every commit.
function renderHistory(workload) {
  const runtimes = RUNTIMES.filter((r) => state.enabled.has(r));
  const series = Object.fromEntries(
    runtimes.map((rt) => [rt, state.reports.map((r) => cpu(r, workload, rt))]),
  );

  document.getElementById(`${workload}-sub`).textContent = runtimes
    .map((rt) => {
      const v = series[rt].at(-1);
      return v == null ? null : `${rt} ${fmt(v)} ms`;
    })
    .filter(Boolean)
    .join(" · ");

  renderTable(
    `${workload}-table`,
    ["commit", "date", ...runtimes.map((rt) => `${rt} ms`)],
    state.reports
      .map((r, i) => [
        `<a href="https://github.com/xnacly/purple-garden/commit/${r.sha}">${short(r.sha)}</a>`,
        r.date,
        ...runtimes.map((rt) => {
          const v = series[rt][i];
          return v == null ? null : fmt(v);
        }),
      ])
      .reverse(),
  );

  state.charts[workload]?.destroy();
  state.charts[workload] = new Chart(document.getElementById(`${workload}-canvas`), {
    type: "line",
    data: {
      labels: state.reports.map((r) => short(r.sha)),
      datasets: runtimes.map((rt) => ({
        label: rt,
        data: series[rt],
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
        y: yScale("CPU time (ms)"),
        x: { ticks: { maxTicksLimit: 12, autoSkip: true }, grid: { display: false } },
      },
      plugins: {
        tooltip: {
          callbacks: {
            title: (items) => {
              const r = state.reports[items[0].dataIndex];
              return `${short(r.sha)} · ${r.date}${r.ref ? ` · ${r.ref}` : ""}`;
            },
            label: (c) => (c.parsed.y == null ? "" : ` ${c.dataset.label}: ${fmt(c.parsed.y)} ms`),
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
  for (const w of state.history) renderHistory(w);
}

(async () => {
  try {
    await load();
    if (!state.reports.length) throw new Error("index.json is empty");
    document.getElementById("status").hidden = true;
    document.getElementById("content").hidden = false;
    state.history = renderHistoryPanels();
    renderControls();
    render();
  } catch (e) {
    document.getElementById("status").textContent = `failed to load benchmark data: ${e.message}`;
    document.getElementById("meta").textContent = "";
  }
})();
