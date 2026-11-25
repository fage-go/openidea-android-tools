const panel = document.querySelector("#android-panel");
const toolsContent = document.querySelector("#android-tools-content");
const environment = document.querySelector("#android-environment");
const settings = document.querySelector("#panel-settings");
const configureEnvironment = settings.querySelector("button");
const noProject = document.querySelector("#android-no-project");
const dialog = document.querySelector("#adb-result");
const resultStatus = dialog.querySelector("#adb-result-status");
const resultOutput = dialog.querySelector("#adb-result-output");
const copyButton = dialog.querySelector("#adb-copy");
const copyStatus = dialog.querySelector("#adb-copy-status");
const dates = new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" });
let visible = false;
let busy = false;
let loadingButton;
let environmentRequest;
let projectScope;
let chosenPlatform;
let captureRequest;
let captureTimer;
let execution;
let displayedResult;

function requestId() {
  return Array.from(crypto.getRandomValues(new Uint32Array(4)), n => n.toString(16)).join("-");
}

function setBusy(action, value = "", trigger) {
  if (busy || execution) return;
  busy = true;
  const changesPage = [
    "open", "configure-android-environment", "check-android-environment", "enter-android-tools",
  ].includes(action);
  const operation = action === "open" ? "refresh-config" : action;
  if (changesPage || operation === "refresh-config" || operation === "inspect-layout") {
    loadingButton = trigger ?? panel.querySelector('button[data-action="' + operation + '"]');
    if (loadingButton) {
      loadingButton.disabled = true;
      loadingButton.setAttribute("aria-busy", "true");
    }
    const status = panel.querySelector("#panel-status");
    if (status) status.hidden = true;
  }
  panel.setAttribute("aria-busy", "true");
  if (changesPage) {
    environmentRequest = requestId();
    window.openidea.postMessage(action + "\n" + environmentRequest + "\n");
    return;
  }
  if (action === "inspect-layout") {
    captureRequest = requestId();
    captureTimer = setTimeout(() => {
      captureRequest = undefined;
      finishAction();
      const status = panel.querySelector("#panel-status");
      status.textContent = "未收到抓取结果，请重试。";
      status.hidden = false;
    }, 15_000);
    window.openidea.postMessage(action + "\n" + captureRequest + "\n");
    return;
  }
  window.openidea.postMessage(value ? action + "\n" + value : action);
}

function finishAction() {
  busy = false;
  if (loadingButton) {
    loadingButton.removeAttribute("aria-busy");
    loadingButton.disabled = loadingButton.dataset.action === "inspect-layout" &&
      panel.querySelector("#device-state").hidden;
    loadingButton = undefined;
  }
  panel.removeAttribute("aria-busy");
}

function setSectionExpanded(toggle, expanded) {
  const id = toggle.getAttribute("aria-controls");
  toggle.closest("section").querySelector("#" + id).hidden = !expanded;
  toggle.setAttribute("aria-expanded", String(expanded));
}

function updatePanel(html, completesAction = !captureRequest && !environmentRequest) {
  if (completesAction) finishAction();
  const template = document.createElement("template");
  template.innerHTML = html;
  const next = template.content;
  for (const time of next.querySelectorAll("time[data-time]")) {
    const date = new Date(Number(time.dataset.time));
    if (!Number.isNaN(date.getTime())) {
      time.dateTime = date.toISOString();
      time.textContent = dates.format(date);
    }
  }
  if (!panel.querySelector("#android-form")) {
    if (panel.querySelector("#settings-content")?.hidden) {
      setSectionExpanded(next.querySelector('[aria-controls="settings-content"]'), false);
    }
    toolsContent.replaceChildren(next);
    return;
  }
  for (const id of ["device", "module", "variant"]) {
    const select = panel.querySelector("#" + id);
    const updated = next.querySelector("#" + id);
    if (select.innerHTML !== updated.innerHTML) select.innerHTML = updated.innerHTML;
    select.value = updated.value;
    select.disabled = updated.disabled;
  }
  panel.querySelector("#device-state").hidden = next.querySelector("#device-state").hidden;
  const capture = '[data-action="inspect-layout"]';
  panel.querySelector(capture).disabled = Boolean(captureRequest) || next.querySelector(capture).disabled;
  panel.querySelector("#adb-submit").disabled = next.querySelector("#adb-submit").disabled;
  for (const id of ["panel-status", "layout-history", "adb-history"]) {
    const current = panel.querySelector("#" + id);
    const updated = next.querySelector("#" + id);
    if (!current.isEqualNode(updated)) current.replaceWith(updated);
  }
}

function updateInstallCommand() {
  const platform = environment.querySelector("#android-install-platform");
  if (!platform) return;
  const command = platform.selectedOptions[0]?.dataset.command ?? "";
  const output = environment.querySelector("#android-install-command");
  output.textContent = command;
  output.hidden = !command;
  const action = environment.querySelector('[data-action="copy-install-command"]');
  action.disabled = !command;
  action.textContent = "复制命令";
}

function showEnvironment(html) {
  environment.innerHTML = html;
  const preview = environment.querySelector("template[data-tools-preview]");
  if (preview) {
    updatePanel(preview.innerHTML, false);
    preview.remove();
    noProject.hidden = true;
    toolsContent.hidden = false;
  }
  const platform = environment.querySelector("#android-install-platform");
  if (platform && chosenPlatform !== undefined) platform.value = chosenPlatform;
  updateInstallCommand();
  if (!environment.open) environment.showModal();
  else environment.querySelector(".environment-actions button:last-child")?.focus({ preventScroll: true });
}

function execute(command) {
  if (busy || execution || panel.querySelector("#adb-submit").disabled) return;
  const supplied = command.trim();
  if (!supplied) {
    panel.querySelector("#adb-command").focus();
    return;
  }
  const normalized = supplied.replace(/^adb\s+/, "");
  const id = requestId();
  execution = id;
  displayedResult = undefined;
  copyButton.disabled = true;
  copyButton.title = "复制结果";
  copyButton.removeAttribute("data-copied");
  copyStatus.textContent = "";
  copyStatus.removeAttribute("data-error");
  dialog.querySelector("#adb-result-command").textContent = "adb " + normalized;
  dialog.querySelector("#adb-result-device").textContent = panel.querySelector("#device").selectedOptions[0].textContent;
  resultStatus.textContent = "执行中…";
  resultStatus.dataset.state = "running";
  resultOutput.textContent = "";
  dialog.showModal();
  window.openidea.postMessage("run-adb\n" + id + "\n" + supplied);
}

function showResult(result) {
  const output = result.error ?? [result.stdout, result.stderr].filter(Boolean).join("\n");
  if (result.error) {
    resultStatus.textContent = "执行失败";
  } else {
    resultStatus.textContent = result.status === 0 ? "执行成功" :
      result.status === null ? "执行中止" : "退出码 " + result.status;
  }
  resultOutput.textContent = output || "无输出";
  copyButton.disabled = !output;
  resultStatus.dataset.state = result.status === 0 ? "success" : "failure";
  resultOutput.scrollTop = 0;
  resultOutput.scrollLeft = 0;
}

window.openidea.onMessage(message => {
  const scopeEnd = message.indexOf("\n");
  if (scopeEnd < 0) return;
  const scope = message.slice(0, scopeEnd);
  const update = message.slice(scopeEnd + 1);
  if (environmentRequest) {
    const first = update.indexOf("\n");
    const second = update.indexOf("\n", first + 1);
    const type = update.slice(0, first);
    if (second > first && update.slice(first + 1, second) === environmentRequest &&
        ["setup", "tools-ready", "unsupported-project"].includes(type)) {
      environmentRequest = undefined;
      finishAction();
      if (projectScope && projectScope !== scope) {
        toolsContent.replaceChildren();
        chosenPlatform = undefined;
      }
      projectScope = scope;
      configureEnvironment.disabled = false;
      const html = update.slice(second + 1);
      if (type === "setup") showEnvironment(html);
      else {
        environment.close();
        noProject.hidden = type !== "unsupported-project";
        toolsContent.hidden = type !== "tools-ready";
        if (type === "tools-ready") {
          updatePanel(html, false);
          panel.dispatchEvent(new Event("refresh-emulators"));
        }
      }
      return;
    }
  }
  if (scope !== projectScope) return;
  if (update.startsWith("panel\n") && !toolsContent.hidden) {
    const applied = loadingButton?.dataset.action === "refresh-config" && !environmentRequest;
    const previousDevice = panel.querySelector("#device")?.value;
    updatePanel(update.slice(6));
    if (previousDevice !== panel.querySelector("#device")?.value) {
      panel.dispatchEvent(new Event("android-device-changed"));
    }
    if (applied) panel.dispatchEvent(new Event("refresh-emulators"));
    return;
  }
  if (environment.open && (update.startsWith("setup-link\n") || update.startsWith("setup-copy\n"))) {
    const error = update.slice(update.indexOf("\n") + 1);
    const status = environment.querySelector("#android-environment-status");
    const copy = environment.querySelector('[data-action="copy-install-command"]');
    if (update.startsWith("setup-copy\n") && copy) {
      copy.textContent = error ? "复制命令" : "已复制";
    }
    status.textContent = error;
    status.hidden = !error;
    return;
  }
  if (update.startsWith("capture\n")) {
    const end = update.indexOf("\n", 8);
    if (end < 0) return;
    const matching = captureRequest === update.slice(8, end);
    if (captureRequest && !matching) return;
    if (matching) {
      clearTimeout(captureTimer);
      captureRequest = undefined;
    }
    updatePanel(update.slice(end + 1), matching);
    return;
  }
  const copyHeader = "adb-copy\n" + displayedResult + "\n";
  if (displayedResult && update.startsWith(copyHeader)) {
    const error = update.slice(copyHeader.length);
    copyButton.disabled = false;
    copyButton.title = error ? "复制结果" : "已复制";
    copyButton.toggleAttribute("data-copied", !error);
    copyStatus.toggleAttribute("data-error", !!error);
    copyStatus.textContent = error || "已复制";
    return;
  }
  const header = "adb\n" + execution + "\n";
  if (!execution || !update.startsWith(header)) return;
  displayedResult = execution;
  execution = undefined;
  try {
    showResult(JSON.parse(update.slice(header.length)));
  } catch {
    showResult({ error: "无法读取 ADB 执行结果" });
  }
});

panel.addEventListener("click", event => {
  if (!settings.contains(event.target)) settings.open = false;
  if (event.target.closest("[data-environment-close]")) {
    environment.querySelector("#android-environment-dismiss")?.click();
    return;
  }
  const toggle = event.target.closest("button.section-toggle");
  if (toggle) {
    setSectionExpanded(toggle, toggle.getAttribute("aria-expanded") === "false");
    return;
  }
  const button = event.target.closest("button[data-action]");
  if (!button || button.disabled) return;
  const { action, value } = button.dataset;
  if (settings.contains(button)) {
    settings.open = false;
    settings.querySelector("summary").focus();
  }
  if (action === "close-environment") {
    if (!environmentRequest) environment.close();
  } else if (action === "open-cli-download") window.openidea.postMessage(action);
  else if (action === "copy-install-command") {
    const platform = environment.querySelector("#android-install-platform").value;
    if (platform) window.openidea.postMessage(action + "\n" + platform);
  } else if (action === "execute-adb") execute(panel.querySelector("#adb-command").value);
  else if (action === "run-command") execute(value);
  else setBusy(action, value, button);
});

// OpenIDEA's sandbox blocks native form submission.
panel.addEventListener("keydown", event => {
  if (event.key === "Escape" && settings.open) {
    event.preventDefault();
    settings.open = false;
    settings.querySelector("summary").focus();
    return;
  }
  if (event.target.id !== "adb-command" || event.key !== "Enter" || event.isComposing) return;
  event.preventDefault();
  execute(event.target.value);
});

panel.addEventListener("change", event => {
  const field = event.target;
  if (field.id === "android-install-platform") {
    chosenPlatform = field.value;
    updateInstallCommand();
  } else if (["device", "module", "variant"].includes(field.id)) {
    window.openidea.postMessage(field.id + "\n" + field.value);
  }
});

environment.addEventListener("cancel", event => {
  event.preventDefault();
  environment.querySelector("#android-environment-dismiss")?.click();
});

dialog.querySelector("[data-close]").addEventListener("click", () => dialog.close());
copyButton.addEventListener("click", () => {
  if (copyButton.disabled || !displayedResult) return;
  copyStatus.textContent = "";
  copyStatus.removeAttribute("data-error");
  copyButton.disabled = true;
  window.openidea.postMessage("copy-adb-result\n" + displayedResult + "\n" + resultOutput.textContent);
});

// OpenIDEA may keep hidden tool windows mounted.
new IntersectionObserver(([entry]) => {
  if (visible === entry.isIntersecting) return;
  visible = entry.isIntersecting;
  if (visible && !environment.open) setBusy("open");
}).observe(document.documentElement);
