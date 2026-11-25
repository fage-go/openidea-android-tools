const layout = document.querySelector("#android-layout");
const status = layout.querySelector("#layout-status");
const columns = layout.querySelector("#layout-columns");
const caption = layout.querySelector("#layout-window");
const viewport = layout.querySelector("#layout-viewport");
const screen = layout.querySelector("#layout-screen");
const image = screen.querySelector("img");
const tree = layout.querySelector("#layout-tree");
const treeRows = layout.querySelector("#layout-tree-rows");
const treeFilter = layout.querySelector("#layout-tree-filter");
const compact = layout.querySelector("#layout-tree-compact");
const treeEmpty = layout.querySelector("#layout-tree-empty");
const properties = layout.querySelector("#layout-properties");
const filter = layout.querySelector("#layout-property-filter");
const highlight = layout.querySelector("#layout-highlight");
const collapsedGroups = new Set();
const groupLabels = new Map([
  ["basic", "基本信息"],
  ["meta", "元数据"],
  ["layout", "布局"],
  ["measurement", "尺寸测量"],
  ["drawing", "绘制"],
  ["scrolling", "滚动"],
  ["text", "文本"],
  ["focus", "焦点"],
  ["accessibility", "无障碍"],
  ["theme", "主题"],
  ["misc", "其他"],
]);
let views = [];
let selection;
let revealedView;

function showSnapshot(snapshot) {
  views = [];
  selection = undefined;
  revealedView = undefined;
  caption.textContent = snapshot.window;
  caption.title = snapshot.window;

  const rows = document.createDocumentFragment();
  const pending = [{ node: snapshot.tree, depth: 0, parent: null }];
  while (pending.length) {
    const { node, depth, parent } = pending.pop();
    const indent = depth * 14;
    const entry = document.createElement("li");
    entry.style.setProperty("--tree-indent", indent + "px");
    entry.setAttribute("aria-level", String(depth + 1));
    const button = document.createElement("button");
    button.type = "button";
    const id = node.properties.find(([name]) => name === "id")?.[1];
    const suffix = id && id !== "NO_ID" ? "  @" + String(id).replace(/^@/, "") : "";
    button.title = node.class + suffix;
    button.setAttribute("aria-pressed", "false");
    const view = {
      node, button, depth, entry, parent, suffix, indent,
      fullName: node.class.toLowerCase(),
      shortName: node.class.slice(node.class.lastIndexOf(".") + 1),
    };
    views.push(view);
    button.addEventListener("click", () => selectView(view));
    entry.append(button);
    rows.append(entry);
    for (let i = node.children.length - 1; i >= 0; i--) {
      pending.push({ node: node.children[i], depth: depth + 1, parent: view });
    }
  }

  treeRows.replaceChildren(rows);
  updateTreeNames();
  filterTree();
  columns.hidden = false;
  status.hidden = true;
  image.src = "data:image/png;base64," + snapshot.png;
  selectView(views[0]);
}

function updateTreeNames() {
  for (const { button, node, shortName, suffix } of views) {
    button.textContent = (compact.checked ? shortName : node.class) + suffix;
  }
}

function filterTree() {
  const query = treeFilter.value.trim().toLowerCase();
  for (const view of views) {
    view.entry.hidden = view !== revealedView && !view.fullName.includes(query);
  }
  // Children follow their parents in views. Walk backwards to reveal each matching branch.
  for (let i = views.length - 1; i >= 0; i--) {
    const { entry, parent } = views[i];
    if (!entry.hidden && parent) parent.entry.hidden = false;
  }
  treeEmpty.hidden = !views[0]?.entry.hidden;
}

compact.addEventListener("change", updateTreeNames);
treeFilter.addEventListener("input", () => {
  revealedView = undefined;
  filterTree();
});

function selectView(view) {
  selection?.button.setAttribute("aria-pressed", "false");
  selection = view;
  view.button.setAttribute("aria-pressed", "true");
  showProperties();
  drawSelection();
}

function scrollToView(view) {
  const row = view.button.getBoundingClientRect();
  const scrollport = tree.getBoundingClientRect();
  tree.scrollTop += row.top - scrollport.top + (row.height - tree.clientHeight) / 2;
  tree.scrollLeft = view.indent;
}

function showProperties() {
  const query = filter.value.trim().toLocaleLowerCase();
  const groups = new Map();
  for (const [name, value] of selection?.node.properties ?? []) {
    const separator = name.indexOf(":");
    const group = separator > 0 ? name.slice(0, separator).toLowerCase() : "basic";
    const label = groupLabels.get(group) ?? group;
    if (query && ![name, String(value), label].some(text => text.toLocaleLowerCase().includes(query))) {
      continue;
    }
    if (!groups.has(group)) groups.set(group, []);
    groups.get(group).push([name, value]);
  }

  const content = document.createDocumentFragment();
  const orderedGroups = new Set([...groupLabels.keys(), ...groups.keys()]);
  for (const group of orderedGroups) {
    const rows = groups.get(group);
    if (!rows) continue;
    const section = document.createElement("details");
    section.open = !!query || !collapsedGroups.has(group);
    section.addEventListener("toggle", () => {
      if (!section.isConnected || filter.value.trim()) return;
      if (section.open) collapsedGroups.delete(group);
      else collapsedGroups.add(group);
    });
    const heading = document.createElement("summary");
    heading.textContent = groupLabels.get(group) ?? group;
    const list = document.createElement("dl");
    for (const [name, value] of rows) {
      const term = document.createElement("dt");
      const description = document.createElement("dd");
      term.textContent = name;
      description.textContent = value;
      list.append(term, description);
    }
    section.append(heading, list);
    content.append(section);
  }
  if (!groups.size) {
    const empty = document.createElement("p");
    empty.textContent = query ? "没有匹配的属性" : "暂无属性";
    content.append(empty);
  }
  properties.replaceChildren(content);
  properties.scrollTop = 0;
}

filter.addEventListener("input", showProperties);

function fitScreenshot() {
  const { naturalWidth: width, naturalHeight: height } = image;
  if (!width || !height) return;
  const scale = Math.min(viewport.clientWidth / width, viewport.clientHeight / height);
  screen.style.width = width * scale + "px";
  screen.style.height = height * scale + "px";
  drawSelection();
}

function drawSelection() {
  const bounds = selection?.node.bounds;
  const { naturalWidth: width, naturalHeight: height } = image;
  highlight.hidden = true;
  if (!bounds || !width || !height) return;
  const [left, top, right, bottom] = bounds;
  const x = Math.max(left, 0);
  const y = Math.max(top, 0);
  const w = Math.min(right, width) - x;
  const h = Math.min(bottom, height) - y;
  if (w <= 0 || h <= 0) return;
  highlight.style.left = 100 * x / width + "%";
  highlight.style.top = 100 * y / height + "%";
  highlight.style.width = 100 * w / width + "%";
  highlight.style.height = 100 * h / height + "%";
  highlight.hidden = false;
}

image.addEventListener("load", fitScreenshot);
new ResizeObserver(fitScreenshot).observe(viewport);

image.addEventListener("click", event => {
  const box = image.getBoundingClientRect();
  if (!box.width || !box.height || !image.naturalWidth || !image.naturalHeight) return;
  const x = (event.clientX - box.left) * image.naturalWidth / box.width;
  const y = (event.clientY - box.top) * image.naturalHeight / box.height;
  let best;
  let bestArea = Infinity;
  for (const view of views) {
    if (!view.node.bounds) continue;
    const [left, top, right, bottom] = view.node.bounds;
    if (x < left || x >= right || y < top || y >= bottom) continue;
    const area = (right - left) * (bottom - top);
    if (area < bestArea || (area === bestArea && view.depth >= best.depth)) {
      best = view;
      bestArea = area;
    }
  }
  if (best) {
    if (treeFilter.value.trim()) {
      revealedView = best;
      filterTree();
    }
    selectView(best);
    scrollToView(best);
  }
});

function showError(message) {
  columns.hidden = true;
  status.hidden = false;
  status.textContent = message;
}

const request = Array.from(crypto.getRandomValues(new Uint32Array(4)), n => n.toString(16)).join("-");
const prefix = "layout\n" + request + "\n";
let chunks = [];
window.openidea.onMessage(message => {
  if (!chunks || !message.startsWith(prefix)) return;
  const data = message.slice(prefix.length);
  if (data.startsWith("chunk\n")) {
    chunks.push(data.slice(6));
    return;
  }
  if (data === "done\n") {
    try {
      showSnapshot(JSON.parse(chunks.join("")));
    } catch {
      showError("无法打开布局快照，请在 Android 面板重新抓取布局。");
    }
  } else if (data.startsWith("error\n")) {
    showError(data.slice(6));
  }
  chunks = undefined;
});
requestAnimationFrame(() => window.openidea.postMessage("read-snapshot\n" + request + "\n"));
