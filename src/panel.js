const panel = document.querySelector("#android-panel");
const prefix = panel.dataset.project + "\n";
let visible = false;
let scanning = false;

function scan(action) {
  if (scanning) return;
  scanning = true;
  const status = panel.querySelector('[role="status"]');
  status.hidden = false;
  status.textContent = "正在刷新配置…";
  const refresh = panel.querySelector('[data-action="refresh-config"]');
  if (refresh) refresh.disabled = true;
  window.openidea.postMessage(action);
}

window.openidea.onMessage(message => {
  if (!message.startsWith(prefix)) return;
  scanning = false;
  const next = document.createElement("template");
  next.innerHTML = message.slice(prefix.length);
  const currentForm = panel.querySelector("form");
  if (!currentForm) {
    panel.replaceChildren(next.content);
    return;
  }
  for (const id of ["device", "module", "variant"]) {
    const select = panel.querySelector("#" + id);
    const updated = next.content.querySelector("#" + id);
    if (select.innerHTML !== updated.innerHTML) select.innerHTML = updated.innerHTML;
    select.value = updated.value;
    select.disabled = updated.disabled;
  }
  panel.querySelector('[data-action="refresh-config"]').disabled = false;
  panel.querySelector('[role="status"]').replaceWith(next.content.querySelector('[role="status"]'));
});

panel.addEventListener("click", event => {
  const button = event.target.closest("[data-action]");
  if (!button || button.disabled) return;
  scan(button.dataset.action);
});

panel.addEventListener("change", event => {
  const field = event.target;
  if (["device", "module", "variant"].includes(field.id)) {
    window.openidea.postMessage(field.id + "\n" + field.value);
  }
});

// OpenIDEA may keep hidden tool windows mounted.
new IntersectionObserver(([entry]) => {
  if (visible === entry.isIntersecting) return;
  visible = entry.isIntersecting;
  if (visible) scan("open");
  else scanning = false;
}).observe(document.documentElement);
