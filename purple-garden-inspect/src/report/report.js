// Draws the report's views from the json the compiler embedded, nothing is computed here that
// the compiler didn't: see purple-garden-inspect/src/report.rs for the data.
"use strict";

const D = JSON.parse(document.getElementById("data").textContent);

const TABS = [
  ["ir", "IR"],
  ["cfg", "CFG"],
  ["registers", "Registers"],
  ["bytecode", "Bytecode"],
  ["x86", "x86-64"],
  ["ast", "AST"],
  ["types", "Types"],
  ["memory", "Memory"],
  ["output", "Output"],
];
const PER_FUNCTION = new Set(["ir", "cfg", "registers", "bytecode", "x86"]);
const LAST_PASS = Math.max(D.passes.length - 1, 0);

// what the views show, mirrored in the url's hash so links and reloads keep it
const S = {
  tab: "ir",
  func: null, // a function id, or "all"
  pass: LAST_PASS,
  target: "bytecode",
  line: null, // the selected source line
  hover: null, // "func:row"
  pinned: null, // a row a link points at, "func:row"
  zoom: 1,
  pan: [24, 52],
  fitted: null, // what the graph was last fitted to
};

// ---- helpers

function el(tag, attrs, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") e.className = v;
    else if (k === "style") e.style.cssText = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c instanceof Node ? c : String(c));
  }
  return e;
}

const SVG = "http://www.w3.org/2000/svg";
function svg(tag, attrs) {
  const e = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  return e;
}

/// Maps byte offsets of `text`, as the compiler counts, to string indices.
function byteIndex(text) {
  if (!/[^\x00-\x7f]/.test(text)) return (b) => b;
  const map = [];
  let b = 0;
  for (let i = 0; i < text.length; i++) {
    const code = text.codePointAt(i);
    const len = code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    for (let k = 0; k < len; k++) map[b + k] = i;
    b += len;
    if (code >= 0x10000) i++;
  }
  map[b] = text.length;
  return (byte) => map[Math.min(byte, b)] ?? text.length;
}

/// `text` with `highlights`, `[start, end, class]` byte ranges, as spans. A class is a name, or
/// an index into `D.classes`.
function highlighted(text, highlights) {
  const at = byteIndex(text);
  const out = [];
  let i = 0;
  for (const [start, end, cls] of highlights) {
    const s = at(start), e = at(end);
    if (s < i) continue;
    if (s > i) out.push(text.slice(i, s));
    const name = typeof cls === "number" ? D.classes[cls] : cls.replace(".", "-");
    out.push(el("span", { class: name }, text.slice(s, e)));
    i = e;
  }
  if (i < text.length) out.push(text.slice(i));
  return out;
}

const func = (id) => D.functions.find((f) => f.id === id);
const TONE = Object.fromEntries(D.tones.map((t, i) => [t, i]));
const changed = (f, pass) => pass === 0 || f.changes[pass] !== null;

/// The IR of `f` after `pass` as listing lines, the rows the pass removed and added marked.
function irLines(f, pass) {
  const rows = D.snapshots[f.rows[pass]];
  const changes = f.changes[pass];
  if (!changes) return rows;
  const old = D.snapshots[f.rows[pass - 1]];
  const out = [];
  for (const [kind, start, len] of changes) {
    for (const [gutter, text, highlights, row, tone] of (kind === 1 ? old : rows).slice(start, start + len)) {
      // kept rows as they are, removed ones link nowhere anymore
      if (kind === 0) out.push([gutter, text, highlights, row, tone]);
      else if (kind === 1) out.push([gutter, text, highlights, null, TONE.Removed]);
      else out.push([gutter, text, highlights, row, TONE.Added]);
    }
  }
  return out;
}

/// A [`CompactLayout`] as nodes, edges, titles and size, moved right by `dx`.
function layoutOf([nodes, edges, titles, size], dx = 0) {
  return {
    nodes: nodes.map(([func, block, x, y, w, h, r0, r1]) => ({ func, block, origin: [x + dx, y], size: [w, h], rows: [r0, r1] })),
    edges: edges.map(edge),
    titles: titles.map(([text, x, y]) => ({ text, origin: [x + dx, y] })),
    size,
  };
  function edge([kind, ...flat]) {
    const xy = [];
    for (let i = 0; i < flat.length; i += 2) xy.push([flat[i] + dx, flat[i + 1]]);
    return { back: kind === 1, call: kind === 2, points: xy.slice(0, -1), tip: xy[xy.length - 1] };
  }
}

/// Every function's graph after `pass`, side by side.
function programLayout(pass) {
  const p = D.programs[D.program[pass]];
  const out = { nodes: [], edges: [], titles: [], size: p.size };
  for (const [index, dx] of p.parts) {
    const part = layoutOf(D.layouts[index], dx);
    out.nodes.push(...part.nodes);
    out.edges.push(...part.edges);
    out.titles.push(...part.titles);
  }
  out.edges.push(...layoutOf([[], p.calls, [], p.size]).edges);
  return out;
}
const shown = () => (S.func === "all" ? D.functions : [func(S.func)].filter(Boolean));
const key = (f, r) => `${f}:${r}`;

// ---- source lines

// byte offsets: the compiler's spans count bytes
const SOURCE = new TextEncoder().encode(D.source);
const lineStarts = [0];
SOURCE.forEach((b, i) => {
  if (b === 0x0a) lineStarts.push(i + 1);
});
function lineOf(offset) {
  let lo = 0, hi = lineStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (lineStarts[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/// The source line `func:row` was lowered from.
function sourceLine(rowKey) {
  if (!rowKey) return null;
  const [f, r] = rowKey.split(":").map(Number);
  const span = func(f)?.spans.find(([row]) => row === r);
  return span ? lineOf(span[1]) : null;
}

/// Rows lowered from the selected source line.
function activeRows() {
  const out = new Set();
  if (S.line === null) return out;
  for (const f of shown()) {
    for (const [row, span] of f.spans) if (lineOf(span) === S.line) out.add(key(f.id, row));
  }
  return out;
}

/// The function whose rows came from source line `line`, the entry's are top level code.
function functionAt(line) {
  const hits = D.functions.filter((f) => f.name !== "entry" && f.spans.some(([, s]) => lineOf(s) === line));
  return (hits[0] ?? D.functions.find((f) => f.name === "entry"))?.id ?? null;
}

function renderSource() {
  const root = document.getElementById("source");
  root.replaceChildren();
  const bytes = SOURCE;
  const decoder = new TextDecoder();
  let token = 0;
  lineStarts.forEach((start, n) => {
    const end = n + 1 < lineStarts.length ? lineStarts[n + 1] - 1 : bytes.length;
    const text = decoder.decode(bytes.slice(start, end));
    const highlights = [];
    while (token < D.tokens.length && D.tokens[token][0] < end) {
      const [s, e, name] = D.tokens[token];
      if (e > start) highlights.push([Math.max(s, start) - start, Math.min(e, end) - start, name]);
      if (e > end) break;
      token++;
    }
    root.append(
      el(
        "div",
        { class: "src", "data-line": n, onclick: () => selectLine(n) },
        el("span", { class: "n" }, n + 1),
        el("span", {}, highlighted(text, highlights)),
      ),
    );
  });
}

function selectLine(n) {
  S.line = S.line === n ? null : n;
  if (S.line !== null && S.func !== "all") S.func = functionAt(n) ?? S.func;
  render();
}

// ---- hover linking

function setHover(rowKey) {
  rowKey = rowKey ?? S.pinned;
  if (S.hover === rowKey) return;
  for (const e of document.querySelectorAll(".hov")) e.classList.remove("hov");
  S.hover = rowKey;
  if (rowKey) {
    for (const e of document.querySelectorAll(`[data-r="${rowKey}"]`)) e.classList.add("hov");
    const line = sourceLine(rowKey);
    if (line !== null) document.querySelector(`.src[data-line="${line}"]`)?.classList.add("hov");
  }
  renderStatus();
}

document.addEventListener("mouseover", (e) => {
  const hit = e.target.closest?.("[data-r]");
  setHover(hit ? hit.dataset.r : null);
});

// ---- listings

/// `lines` as `[gutter, text, highlights, row, tone]` tuples.
function listing(lines) {
  const active = activeRows();
  const width = Math.max(0, ...lines.map((l) => l[0].length));
  return el(
    "div",
    { class: "scroll" },
    el(
      "div",
      { class: "listing" },
      lines.map(([gutter, text, highlights, row, tone]) => {
        const r = row ? key(row[0], row[1]) : null;
        const cls = ["ln", `tone-${D.tones[tone]}`, r && active.has(r) && "on", r && r === S.hover && "hov"];
        return el(
          "div",
          { class: cls.filter(Boolean).join(" "), "data-r": r },
          el("span", { class: "g", style: `width:${width}ch` }, gutter),
          el("span", { class: "t" }, highlighted(text, highlights)),
        );
      }),
    ),
  );
}

/// The listings of every shown function, a blank line between two.
function joined(pick) {
  const out = [];
  for (const f of shown()) {
    if (out.length) out.push(["", "", [], null, TONE.Normal]);
    out.push(...pick(f));
  }
  return out;
}

const empty = (text) => el("div", { class: "empty" }, text);

// ---- views

function viewIr() {
  if (!shown().length) return empty("No function to show");
  const anyChanged = (i) => shown().some((f) => changed(f, i));
  const passes = el(
    "div",
    { class: "passes" },
    D.passes.map((name, i) =>
      el(
        "div",
        {
          class: ["item", i === S.pass && "on", anyChanged(i) && "changed"].filter(Boolean).join(" "),
          onclick: () => {
            S.pass = i;
            render();
          },
        },
        el("span", {}, name),
        el("span", {}, anyChanged(i) && i > 0 ? "+" : ""),
      ),
    ),
  );
  return [passes, listing(joined((f) => irLines(f, S.pass)))];
}

function viewCode(pick, note) {
  if (!shown().length) return empty("No function to show");
  const lines = joined(pick);
  const list = lines.length ? listing(lines) : empty("Nothing to show");
  return note ? el("div", { class: "scroll", style: "display:flex;flex-direction:column" }, el("div", { class: "bar" }, note), list) : list;
}

function x86Note() {
  const fs = shown();
  const native = fs.filter((f) => f.native).length;
  const host = D.host_runs_x86 ? "" : " · compiled on a host that can't run it";
  return `${native} of ${fs.length} functions compiled by the x86-64 backend${host}`;
}

/// Font size of the graph's monospace font making a character `char_width` wide.
let graphFont = null;
function graphFontSize() {
  if (graphFont) return graphFont;
  const probe = el("span", { style: "font:100px var(--mono);position:absolute;visibility:hidden" }, "m".repeat(10));
  document.body.append(probe);
  const width = probe.getBoundingClientRect().width / 10;
  probe.remove();
  graphFont = width > 0 ? (100 * D.char_width) / width : 11;
  return graphFont;
}

function viewCfg() {
  if (!shown().length) return empty("No function to show");
  const layout = S.func === "all" ? programLayout(S.pass) : layoutOf(D.layouts[shown()[0].cfg[S.pass]]);
  const rowsOf = (f) => D.snapshots[func(f).rows[S.pass]];
  const active = activeRows();
  const world = el("div", { class: "world" });
  const lines = svg("svg", { width: layout.size[0] + 200, height: layout.size[1] + 200 });
  for (const e of layout.edges) {
    const cls = e.call ? "call" : e.back ? "back" : "";
    const [first, ...rest] = e.points;
    let d = `M${first[0]},${first[1]}`;
    for (let i = 0; i + 2 < rest.length; i += 3) d += `C${rest[i]},${rest[i + 1]},${rest[i + 2]}`;
    lines.append(svg("path", { d, class: cls }));
    // the arrow head, from the end of the curve to the tip
    const end = e.points[e.points.length - 1];
    let dx = e.tip[0] - end[0], dy = e.tip[1] - end[1];
    const len = Math.hypot(dx, dy) || 1;
    dx /= len;
    dy /= len;
    const bx = e.tip[0] - dx * 8, by = e.tip[1] - dy * 8;
    lines.append(
      svg("polygon", {
        points: `${e.tip[0]},${e.tip[1]} ${bx - dy * 4},${by + dx * 4} ${bx + dy * 4},${by - dx * 4}`,
        class: cls,
      }),
    );
  }
  world.append(lines);
  for (const t of layout.titles) {
    world.append(el("div", { class: "title", style: `left:${t.origin[0]}px;top:${t.origin[1]}px` }, t.text));
  }
  const font = graphFontSize();
  for (const n of layout.nodes) {
    const rows = rowsOf(n.func).slice(n.rows[0], n.rows[1] + 1);
    world.append(
      el(
        "div",
        {
          class: "node",
          style: `left:${n.origin[0]}px;top:${n.origin[1]}px;width:${n.size[0]}px;height:${n.size[1]}px;` +
            `padding:${D.pad}px 0;font-size:${font}px;line-height:${D.line}px;--pad:${D.pad}px`,
        },
        rows.map(([, text, highlights, row]) => {
          // a block's rows sit one indent in, the label at none
          const cut = text.startsWith("  ") ? 2 : 0;
          const shifted = highlights.map(([s, e, c]) => [s - cut, e - cut, c]);
          const r = row ? key(row[0], row[1]) : null;
          return el(
            "div",
            { "data-r": r, class: [r && active.has(r) && "on", r && r === S.hover && "hov"].filter(Boolean).join(" ") },
            highlighted(text.slice(cut), shifted),
          );
        }),
      ),
    );
  }

  const view = el("div", { class: "cfg" }, world);
  const place = () => {
    world.style.transform = `translate(${S.pan[0]}px,${S.pan[1]}px) scale(${S.zoom})`;
    zoomLabel.textContent = `${Math.round(S.zoom * 100)}%`;
  };
  const fit = () => {
    const w = view.clientWidth, h = view.clientHeight;
    if (!w || !layout.size[0]) return;
    S.zoom = Math.max(0.2, Math.min(1, (w - 48) / layout.size[0], (h - 100) / layout.size[1]));
    S.pan = [Math.max(24, (w - layout.size[0] * S.zoom) / 2), 52];
    place();
  };
  const zoomBy = (factor, at) => {
    const z = Math.max(0.2, Math.min(4, S.zoom * factor));
    // keep the point under `at` where it is
    S.pan = [at[0] - ((at[0] - S.pan[0]) * z) / S.zoom, at[1] - ((at[1] - S.pan[1]) * z) / S.zoom];
    S.zoom = z;
    place();
  };
  const zoomLabel = el("span", { class: "zoom" });
  const center = () => [view.clientWidth / 2, view.clientHeight / 2];
  view.append(
    el(
      "div",
      { class: "tools", onmousedown: (e) => e.stopPropagation() },
      `after ${D.passes[S.pass]}`,
      el("button", { onclick: () => zoomBy(1 / 1.25, center()) }, "−"),
      zoomLabel,
      el("button", { onclick: () => zoomBy(1.25, center()) }, "+"),
      el("button", { onclick: () => { S.zoom = 1; S.pan = [24, 52]; place(); } }, "100%"),
      el("button", { onclick: fit }, "fit"),
    ),
  );
  view.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const box = view.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) zoomBy(Math.exp(-e.deltaY / 300), [e.clientX - box.left, e.clientY - box.top]);
      else {
        S.pan = [S.pan[0] - e.deltaX, S.pan[1] - e.deltaY];
        place();
      }
    },
    { passive: false },
  );
  view.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    const start = [e.clientX, e.clientY, ...S.pan];
    view.classList.add("drag");
    const move = (m) => {
      S.pan = [start[2] + m.clientX - start[0], start[3] + m.clientY - start[1]];
      place();
    };
    const up = () => {
      view.classList.remove("drag");
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  });
  // fit when another function is shown, keep pan and zoom while stepping through passes
  requestAnimationFrame(() => {
    if (S.fitted !== S.func) {
      S.fitted = S.func;
      fit();
    }
    place();
  });
  place();
  return view;
}

const LANE = 22, ROW = 16, LABELS = 64, CHART = 36;
const BARS = getComputedStyle(document.documentElement).getPropertyValue("--bars").split(",").map((c) => c.trim());

function timeline(f, lanes) {
  const rows = f.pressure.length;
  const top = CHART + 18;
  const max = Math.max(1, ...f.pressure);
  const t = el("div", {
    class: "timeline",
    style: `width:${LABELS + ROW * rows + 24}px;height:${top + LANE * lanes.length + 12}px`,
  });
  const at = (x, y, w, h) => `left:${x}px;top:${y}px;width:${w}px;height:${h}px`;
  f.pressure.forEach((live, row) => {
    const h = (CHART * live) / max;
    t.append(el("div", { class: "live", style: at(LABELS + ROW * row + 1, CHART - h, ROW - 2, h) }));
  });
  for (const [row, block] of f.blocks) {
    t.append(el("div", { class: "tick", style: `left:${LABELS + ROW * row}px;top:${CHART + 2}px` }, `b${block}`));
    t.append(el("div", { class: "rule", style: at(LABELS + ROW * row, top, 1, LANE * lanes.length) }));
  }
  lanes.forEach(([location, bars], lane) => {
    const y = top + LANE * lane;
    t.append(el("div", { class: "lane", style: `left:8px;top:${y + 3}px` }, location));
    for (const { value, rows: [def, last] } of bars) {
      // from the middle of the defining row to the middle of the last use: a register freed by a
      // value's last use is taken by the value defined on that very row
      const [x, w] = def === last ? [ROW * def + ROW / 4, ROW / 2] : [ROW * def + ROW / 2, ROW * (last - def)];
      const color = BARS[value % BARS.length];
      t.append(
        el(
          "div",
          {
            class: "bar",
            "data-r": key(f.id, def),
            title: `%v${value} ${location}, rows ${def}..${last}`,
            style: `${at(LABELS + x + 1, y + 2, w - 2, LANE - 4)};border-color:${color};` +
              `background:color-mix(in srgb, ${color} 25%, transparent)`,
          },
          `%v${value}`,
        ),
      );
    }
  });
  return t;
}

function viewRegisters() {
  if (!shown().length) return empty("No function to show");
  const toggle = el(
    "span",
    { style: "margin-left:auto;display:flex;height:100%" },
    ["bytecode", "x86"].map((target) =>
      el(
        "span",
        {
          class: ["item", S.target === target && "on"].filter(Boolean).join(" "),
          style: "color:var(--fg)",
          onclick: () => {
            S.target = target;
            render();
          },
        },
        target === "x86" ? "x86-64" : "bytecode",
      ),
    ),
  );
  const body = el("div", { class: "regs" });
  for (const f of shown()) {
    body.append(el("h3", {}, `f${f.id} <${f.name}>`));
    const lanes = f.registers[S.target];
    body.append(lanes.Ok ? timeline(f, lanes.Ok) : el("div", { class: "why" }, lanes.Err));
  }
  return el(
    "div",
    { style: "flex:1;display:flex;flex-direction:column;min-width:0" },
    el(
      "div",
      { class: "bar", style: "padding-right:0" },
      "a lane per register and stack slot, a bar from a value's definition to its last use, live values on top",
      toggle,
    ),
    body,
  );
}

function duration(us) {
  return us >= 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us} µs`;
}
function bytes(n) {
  return n < 1024 ? `${n} B` : n < 1 << 20 ? `${(n / 1024).toFixed(1)} KiB` : `${(n / (1 << 20)).toFixed(1)} MiB`;
}

function viewMemory() {
  if (!D.phases.length) return empty("Nothing compiled");
  const maxTime = Math.max(1, ...D.phases.map((p) => p.micros));
  const maxPeak = Math.max(1, ...D.phases.map((p) => p.peak));
  const meter = (fraction, cls) =>
    el("span", { class: `meter ${cls}` }, el("span", { style: `width:${(fraction * 100).toFixed(1)}%` }));
  const total = D.phases.reduce((t, p) => t + p.micros, 0);
  return el(
    "div",
    { class: "page" },
    el(
      "table",
      {},
      el("tr", {}, ["phase", "wall time", "peak live", "allocs", "frees", "allocated"].map((h) => el("th", {}, h))),
      D.phases.map((p) =>
        el(
          "tr",
          {},
          el("td", {}, p.name),
          el("td", {}, meter(p.micros / maxTime, ""), duration(p.micros)),
          el("td", {}, meter(p.peak / maxPeak, "peak"), bytes(p.peak)),
          el("td", {}, p.allocs),
          el("td", {}, p.frees),
          el("td", {}, bytes(p.bytes)),
        ),
      ),
    ),
    el("h4", {}, `compiled in ${duration(total)}, when the report was made`),
  );
}

function viewOutput() {
  const page = el("div", { class: "page" });
  if (D.error) page.append(el("h4", {}, "compiler"), el("pre", { class: "error" }, D.error));
  if (D.diagnostics.length) {
    page.append(el("h4", {}, `${D.diagnostics.length} errors`));
    for (const d of D.diagnostics) {
      page.append(
        el(
          "pre",
          {
            class: "error",
            onclick: () => {
              S.line = lineOf(d.start);
              render();
              document.querySelector(`.src[data-line="${S.line}"]`)?.scrollIntoView({ block: "center" });
            },
          },
          d.rendered.trimEnd(),
        ),
      );
    }
  }
  if (!D.error && !D.diagnostics.length) page.append(el("h4", {}, "no errors"));
  page.append(
    el("h4", {}, "run"),
    el("pre", {}, "A report isn't run, run the program with `purple-garden " + D.file + "`."),
  );
  return page;
}

// ---- chrome

function renderHeader() {
  const errors = D.diagnostics.length;
  const total = D.phases.reduce((t, p) => t + p.micros, 0);
  const note = D.error
    ? el("span", { class: "note bad" }, D.error.split("\n")[0])
    : errors
      ? el("span", { class: "note bad" }, `${errors} error${errors === 1 ? "" : "s"}`)
      : el("span", { class: "note" }, `compiled in ${duration(total)}`);
  document.getElementById("header").replaceChildren(
    el("span", { class: "file" }, D.file),
    note,
    el("span", { class: "note", style: "margin-left:auto" }, "purple garden report"),
  );
}

function renderTabs() {
  document.getElementById("tabs").replaceChildren(
    ...TABS.map(([id, label]) =>
      el(
        "a",
        {
          href: "#",
          class: S.tab === id ? "on" : null,
          onclick: (e) => {
            e.preventDefault();
            S.tab = id;
            render();
          },
        },
        label,
        id === "output" && D.diagnostics.length ? el("span", { class: "count" }, `(${D.diagnostics.length})`) : null,
      ),
    ),
  );
}

function renderFunctions() {
  const bar = document.getElementById("functions");
  bar.hidden = !PER_FUNCTION.has(S.tab) || !D.functions.length;
  const pick = (id) => () => {
    S.func = id;
    S.hover = null;
    render();
  };
  bar.replaceChildren(
    el("span", { class: ["item", S.func === "all" && "on"].filter(Boolean).join(" "), onclick: pick("all") }, "all"),
    el("span", { class: "sep" }),
    ...D.functions.map((f) =>
      el(
        "span",
        {
          class: ["item", S.func === f.id && "on", f.unused && "unused"].filter(Boolean).join(" "),
          title: f.native ? "compiled to x86-64" : f.reason ? `stays bytecode: ${f.reason}` : null,
          onclick: pick(f.id),
        },
        el("span", { class: ["marker", f.native && "native", f.unused && "unused"].filter(Boolean).join(" ") }),
        f.name,
      ),
    ),
    el(
      "span",
      { class: "legend" },
      el("span", { class: "marker native" }),
      "x86-64",
      el("span", { class: "marker", style: "margin-left:8px" }),
      "bytecode",
    ),
  );
}

function renderStatus() {
  const parts = [];
  if (S.line !== null) parts.push(`line ${S.line + 1}`);
  if (PER_FUNCTION.has(S.tab) && D.functions.length) {
    const f = func(S.func);
    parts.push(S.func === "all" ? `all ${D.functions.length} functions` : f ? `f${f.id} ${f.name}` : "");
    if (D.passes.length) parts.push(`after ${D.passes[S.pass]} (${S.pass + 1}/${D.passes.length})`);
    if (S.hover) parts.push(`f${S.hover.replace(":", " row ")}`);
  }
  document.getElementById("status").replaceChildren(...parts.map((p) => el("span", {}, p)));
}

function renderContent() {
  const lines = (pick) => () => viewCode(pick);
  const views = {
    ir: viewIr,
    cfg: viewCfg,
    registers: viewRegisters,
    bytecode: lines((f) => f.bytecode),
    x86: () => viewCode((f) => f.x86, x86Note()),
    ast: () => (D.ast.length ? listing(D.ast) : empty("No AST")),
    types: () => (D.types.length ? listing(D.types) : empty("No types")),
    memory: viewMemory,
    output: viewOutput,
  };
  const content = document.getElementById("content");
  content.replaceChildren(...[views[S.tab]()].flat());
}

function render() {
  for (const e of document.querySelectorAll(".src.on")) e.classList.remove("on");
  if (S.line !== null) document.querySelector(`.src[data-line="${S.line}"]`)?.classList.add("on");
  renderTabs();
  renderFunctions();
  renderContent();
  renderStatus();
  saveHash();
}

// ---- state in the url

function saveHash() {
  const h = new URLSearchParams({ tab: S.tab, f: S.func ?? "", pass: S.pass, target: S.target });
  if (S.line !== null) h.set("line", S.line + 1);
  if (S.pinned) h.set("row", S.pinned);
  history.replaceState(null, "", `#${h}`);
}

function loadHash() {
  const h = new URLSearchParams(location.hash.slice(1));
  if (TABS.some(([id]) => id === h.get("tab"))) S.tab = h.get("tab");
  const f = h.get("f");
  if (f === "all") S.func = "all";
  else if (f && func(Number(f))) S.func = Number(f);
  const pass = Number(h.get("pass") ?? NaN);
  if (Number.isInteger(pass) && pass >= 0 && pass <= LAST_PASS) S.pass = pass;
  if (h.get("target") === "x86") S.target = "x86";
  const line = Number(h.get("line") ?? NaN);
  if (Number.isInteger(line) && line >= 1 && line <= lineStarts.length) S.line = line - 1;
  if (/^\d+:\d+$/.test(h.get("row") ?? "")) S.pinned = h.get("row");
}

// ---- keys: [ and ] step through the passes, 1 to 9 pick a tab

document.addEventListener("keydown", (e) => {
  if (e.metaKey || e.ctrlKey || e.altKey || e.target.closest?.("input, textarea")) return;
  if (e.key === "[" || e.key === "]") {
    S.pass = Math.max(0, Math.min(LAST_PASS, S.pass + (e.key === "]" ? 1 : -1)));
    render();
  } else if (/^[1-9]$/.test(e.key) && TABS[Number(e.key) - 1]) {
    S.tab = TABS[Number(e.key) - 1][0];
    render();
  }
});

// the first function that isn't the entry
S.func = (D.functions.find((f) => !f.unused && f.name !== "entry") ?? D.functions[0])?.id ?? null;
loadHash();
renderHeader();
renderSource();
render();
setHover(S.pinned);
