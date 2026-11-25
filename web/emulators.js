(() => {
  const editor = document.querySelector("#emulator-create");
  const deleteDialog = document.querySelector("#emulator-delete");
  const deleteName = deleteDialog.querySelector("#emulator-delete-name");
  const deleteImage = deleteDialog.querySelector("#emulator-delete-image");
  const deleteImagePath = deleteDialog.querySelector("#emulator-delete-image-path");
  const deleteHint = deleteDialog.querySelector("#emulator-delete-hint");
  const deleteError = deleteDialog.querySelector("#emulator-delete-error");
  const deleteConfirm = deleteDialog.querySelector("#emulator-delete-confirm");
  const deleteCancel = deleteDialog.querySelectorAll("[data-emulator-delete-cancel]");
  const trashIcon = document.querySelector("#emulator-trash-icon");
  const form = editor.querySelector("#emulator-form");
  const profile = editor.querySelector("#emulator-profile");
  const api = editor.querySelector("#emulator-api");
  const image = editor.querySelector("#emulator-image");
  const name = editor.querySelector("#emulator-name");
  const orientation = editor.querySelector("#emulator-orientation");
  const boot = editor.querySelector("#emulator-boot");
  const progress = editor.querySelector("#emulator-progress");
  const progressStatus = editor.querySelector("#emulator-progress-status");
  const progressBar = editor.querySelector("#emulator-progress-bar");
  const next = editor.querySelector("#emulator-next");
  const back = editor.querySelector("#emulator-back");
  const dialogError = editor.querySelector("#emulator-create-error");
  const warning = editor.querySelector("#emulator-catalog-warning");
  const forms = {
    phone: "手机", tablet: "平板", tv: "电视",
    wear: "Wear OS", automotive: "车载", desktop: "桌面",
  };
  const states = { running: "运行中", stopped: "已停止", starting: "启动中", stopping: "停止中" };
  const pending = new Map();
  const apiSort = new Intl.Collator(undefined, { numeric: true });
  let managerScope;
  let rows = [];
  let step = 0;
  let catalog;
  let listRequest;
  let listTimer;
  let operation = false;
  let creating = false;
  let removal;

  const find = selector => panel.querySelector(selector);

  function call(...fields) {
    const id = requestId();
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        pending.delete(id);
        reject(new Error("未收到模拟器操作结果，请重试。"));
      }, fields[0] === "catalog" ? 90_000 : 25_000);
      pending.set(id, { resolve, reject, timeout, scope: projectScope });
      window.openidea.postMessage(["emulator", id, ...fields].join("\n"));
    });
  }

  window.openidea.onMessage(message => {
    const first = message.indexOf("\n");
    if (first === -1 || !message.startsWith("emulator\n", first + 1)) return;
    const second = message.indexOf("\n", first + 10);
    const id = message.slice(first + 10, second);
    const request = pending.get(id);
    if (!request || request.scope !== message.slice(0, first)) return;
    pending.delete(id);
    clearTimeout(request.timeout);
    try {
      const data = JSON.parse(message.slice(second + 1));
      if (data.error) request.reject(new Error(data.error));
      else request.resolve(data);
    } catch {
      request.reject(new Error("无法读取模拟器操作结果。"));
    }
  });

  function feedback(message) {
    const status = find("#emulator-status");
    if (!status) return;
    status.textContent = message ?? "";
    status.hidden = !message;
  }

  function action(label, task, avdName, disabled = false) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.dataset.emulatorAction = task;
    button.dataset.name = avdName;
    button.disabled = disabled;
    return button;
  }

  function drawRows(list) {
    const focused = list.contains(document.activeElement) ? document.activeElement.dataset : null;
    const scroll = list.firstElementChild?.scrollTop ?? 0;
    const selected = find("#device")?.value;
    if (!rows.length) {
      const empty = document.createElement("p");
      empty.textContent = "暂无模拟器";
      list.replaceChildren(empty);
      return;
    }
    const records = document.createElement("ul");
    records.className = "emulator-records";
    records.setAttribute("aria-labelledby", "emulator-title");
    for (const avd of rows) {
      const li = document.createElement("li");
      li.dataset.state = avd.state;
      const label = document.createElement("div");
      label.className = "emulator-record-label";
      const title = document.createElement("strong");
      title.textContent = title.title = avd.name;
      const status = document.createElement("p");
      const subtitle = [avd.api && "API " + avd.api, states[avd.state], avd.serial].filter(Boolean).join(" · ");
      status.textContent = status.title = subtitle;
      label.append(title, status);
      const controls = document.createElement("div");
      controls.className = "record-actions";
      if (avd.state === "running") {
        const active = avd.serial === selected;
        controls.append(action(active ? "当前设备" : "设为设备", "select", avd.name, active));
      }
      if (avd.state === "stopped") {
        const remove = action("", "delete", avd.name);
        remove.className = "icon-button delete";
        remove.title = "删除模拟器";
        remove.setAttribute("aria-label", "删除模拟器 " + avd.name);
        remove.append(trashIcon.content.cloneNode(true));
        controls.append(action("启动", "start", avd.name), remove);
      } else {
        controls.append(action("停止", "stop", avd.name, avd.state === "stopping"));
      }
      li.append(label, controls);
      records.append(li);
    }
    list.replaceChildren(records);
    records.scrollTop = scroll;
    if (focused) {
      [...records.querySelectorAll("button")].find(button =>
        button.dataset.emulatorAction === focused.emulatorAction &&
        button.dataset.name === focused.name)?.focus({ preventScroll: true });
    }
  }

  function refresh() {
    clearTimeout(listTimer);
    const list = find("#emulator-list");
    if (!projectScope || toolsContent.hidden || !list) return Promise.resolve();
    if (managerScope !== projectScope) {
      managerScope = projectScope;
      rows = [];
      catalog = undefined;
    }
    if (listRequest) return listRequest;
    const scope = projectScope;
    listRequest = call("list").then(data => {
      if (scope !== projectScope) return;
      const changed = JSON.stringify(rows) !== JSON.stringify(data.rows);
      rows = data.rows;
      if (changed || !list.firstElementChild) drawRows(list);
      find("#emulator-new").disabled = false;
      if (data.notice) feedback(data.notice);
      const transitioning = rows.some(row => row.state === "starting" || row.state === "stopping");
      const running = rows.some(row => row.state === "running");
      if (visible && !find("#emulator-content").hidden && (transitioning || running)) {
        listTimer = setTimeout(refresh, transitioning ? 2200 : 8000);
      }
    }).catch(error => {
      if (scope === projectScope) {
        feedback(error.message);
        // Let the creation dialog report missing SDK components as well.
        find("#emulator-new").disabled = false;
      }
    }).finally(() => {
      listRequest = undefined;
      if (scope !== projectScope) refresh();
    });
    return listRequest;
  }

  function options(select, entries, preferred = select.value) {
    select.replaceChildren(...entries.map(([value, label]) => new Option(label, value)));
    if (entries.some(([value]) => value === preferred)) select.value = preferred;
  }

  function profileOptions() {
    options(profile, catalog.profiles.filter(item => item.form === form.value).map(item => [item.id, item.name]));
    if (!profile.value) next.disabled = true;
    else next.disabled = false;
    const portrait = orientation.querySelector('[value="portrait"]');
    portrait.disabled = form.value === "tv";
    if (portrait.disabled && orientation.value === "portrait") orientation.value = "default";
  }

  function candidates() {
    const kind = form.value === "tablet" ? "phone" : form.value;
    return catalog.images.filter(item => item.form === kind &&
      (!item.tag.includes("distantdisplay") || profile.value.includes("distant_display")));
  }

  function imageLabel(item) {
    const tags = { default: "AOSP", google_apis: "Google APIs", google_apis_playstore: "Google Play",
      "google-tv": "Google TV", "android-tv": "Android TV" };
    const tag = tags[item.tag] ?? item.tag;
    return tag + " · " + item.abi + (item.installed ? " · 已安装" : " · 待下载");
  }

  function imageOptions() {
    const matches = candidates().filter(item => item.api === api.value)
      .sort((a, b) => Number(b.installed) - Number(a.installed) || a.tag.localeCompare(b.tag));
    options(image, matches.map(item => [item.path, imageLabel(item)]));
    if (!image.value) image.add(new Option("没有兼容的系统镜像", ""));
    image.disabled = !image.value;
    next.disabled = !image.value;
  }

  function showStep(value) {
    step = value;
    for (const section of editor.querySelectorAll("[data-emulator-step]")) {
      section.hidden = Number(section.dataset.emulatorStep) !== step;
    }
    dialogError.hidden = true;
    back.hidden = !step;
    if (step === 0) {
      profileOptions();
    } else if (step === 1) {
      const images = candidates();
      const apis = [...new Set(images.map(item => item.api))].sort((a, b) => apiSort.compare(b, a));
      const installed = apis.find(version => images.some(item => item.api === version && item.installed));
      options(api, apis.map(version => [version, "API " + version]),
        apis.includes(api.value) ? api.value : installed);
      if (!api.value) api.add(new Option("没有兼容的 Android 版本", ""));
      api.disabled = !api.value;
      warning.textContent = catalog.warning ?? "";
      warning.hidden = !catalog.warning;
      imageOptions();
    } else {
      const selected = catalog.images.find(item => item.path === image.value);
      const prefix = profile.value.replace(/[^A-Za-z0-9._-]+/g, "_").replace(/^[^A-Za-z0-9]+/, "");
      const base = ((prefix || "Android") + "_API_" + selected.api).slice(0, 72);
      let proposed = base;
      for (let n = 2; rows.some(row => row.name.toLowerCase() === proposed.toLowerCase()); n++) proposed = base + "_" + n;
      name.value ||= proposed;
      editor.querySelector("#emulator-summary").textContent =
        profile.selectedOptions[0].textContent + " · API " + selected.api + " · " + imageLabel(selected);
      next.disabled = false;
    }
    next.textContent = step < 2 ? "下一步" :
      catalog.images.find(item => item.path === image.value).installed ? "创建" : "下载并创建";
  }

  function setCreating(value) {
    creating = value;
    next.disabled = value;
    back.disabled = value;
    if (value) next.setAttribute("aria-busy", "true");
    else next.removeAttribute("aria-busy");
    for (const control of editor.querySelectorAll("[data-emulator-close], #emulator-name, #emulator-orientation, #emulator-boot")) {
      control.disabled = value;
    }
  }

  function updateCreationProgress(phase, percent) {
    const label = {
      download: "正在下载系统镜像",
      unpack: "正在解压系统镜像",
      create: "正在创建模拟器",
    }[phase];
    if (!label) return;
    const hasPercent = Number.isInteger(percent) && percent >= 0 && percent <= 100;
    if (hasPercent) progressBar.value = percent;
    else progressBar.removeAttribute("value");
    const status = hasPercent ? label + " · " + percent + "%" : label;
    if (progressStatus.textContent !== status) progressStatus.textContent = status;
    if (progress.hidden) {
      progress.hidden = false;
      progress.scrollIntoView({ block: "nearest" });
    }
  }

  function showError(error) {
    dialogError.textContent = error.message;
    dialogError.hidden = false;
  }

  async function openWizard(button) {
    button.disabled = true;
    button.setAttribute("aria-busy", "true");
    try {
      catalog = await call("catalog");
      options(form, Object.entries(forms).filter(([kind]) => catalog.profiles.some(item => item.form === kind)));
      name.value = "";
      orientation.value = "default";
      boot.checked = false;
      api.replaceChildren();
      image.replaceChildren();
      showStep(0);
      editor.showModal();
    } catch (error) {
      feedback(error.message);
    } finally {
      button.removeAttribute("aria-busy");
      button.disabled = false;
    }
  }

  async function create() {
    name.value = name.value.trim();
    if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name.value)) {
      showError(new Error("名称须以字母或数字开头，可使用点、下划线和连字符。"));
      name.focus();
      return;
    }
    setCreating(true);
    dialogError.hidden = true;
    const downloading = !catalog.images.find(item => item.path === image.value).installed;
    if (downloading) updateCreationProgress("download");
    try {
      let state = await call("create", name.value, profile.value, image.value, orientation.value, boot.checked ? "1" : "0");
      while (state.working) {
        if (state.phase === "download" || state.phase === "unpack" || !progress.hidden) {
          updateCreationProgress(state.phase, state.percent);
        }
        await new Promise(resolve => setTimeout(resolve, 1700));
        state = await call("creation-status");
      }
      editor.close();
      feedback(state.notice);
      await refresh();
    } catch (error) {
      showError(error);
    } finally {
      progress.hidden = true;
      setCreating(false);
    }
  }

  async function openDelete(name) {
    const request = { name, image: null };
    removal = request;
    deleteName.textContent = name;
    deleteImage.checked = false;
    deleteImage.disabled = true;
    deleteImagePath.hidden = true;
    deleteConfirm.disabled = true;
    deleteError.hidden = true;
    deleteHint.hidden = true;
    deleteDialog.showModal();
    try {
      const result = await call("delete-preview", name);
      if (!deleteDialog.open || removal !== request) return;
      request.image = result.image;
      deleteImagePath.textContent = result.image ?? "";
      deleteImagePath.hidden = !result.image;
      deleteImage.disabled = !result.image || result.shared;
      const hint = result.shared ? "此镜像还被其他模拟器使用。" :
        result.image ? "" : "未找到可随模拟器删除的 System Image。";
      deleteHint.textContent = hint;
      deleteHint.hidden = !hint;
      deleteConfirm.disabled = false;
    } catch (error) {
      if (deleteDialog.open && removal === request) {
        deleteError.textContent = error.message;
        deleteError.hidden = false;
      }
    }
  }

  deleteConfirm.addEventListener("click", async () => {
    if (!removal || deleteConfirm.disabled) return;
    const { name, image: packagePath } = removal;
    const selectedImage = deleteImage.checked && !deleteImage.disabled ? packagePath : "";
    const imageDisabled = deleteImage.disabled;
    deleteImage.disabled = true;
    deleteConfirm.disabled = true;
    deleteConfirm.setAttribute("aria-busy", "true");
    deleteError.hidden = true;
    for (const button of deleteCancel) button.disabled = true;
    try {
      const result = await call("delete", name, selectedImage);
      deleteDialog.close();
      feedback(result.notice);
      await refresh();
    } catch (error) {
      deleteError.textContent = error.message;
      deleteError.hidden = false;
    } finally {
      deleteConfirm.removeAttribute("aria-busy");
      deleteConfirm.disabled = false;
      deleteImage.disabled = imageDisabled;
      for (const button of deleteCancel) button.disabled = false;
    }
  });

  deleteDialog.addEventListener("cancel", event => {
    if (deleteConfirm.getAttribute("aria-busy") === "true") event.preventDefault();
  });
  for (const button of deleteCancel) {
    button.addEventListener("click", () => deleteDialog.close());
  }

  panel.addEventListener("refresh-emulators", refresh);
  panel.addEventListener("android-device-changed", () => {
    if (rows.length) drawRows(find("#emulator-list"));
  });
  panel.addEventListener("click", async event => {
    const toggle = event.target.closest('[aria-controls="emulator-content"]');
    if (toggle?.getAttribute("aria-expanded") === "true") refresh();
    const newButton = event.target.closest("#emulator-new");
    if (newButton && !newButton.disabled && !busy) {
      await openWizard(newButton);
      return;
    }
    const button = event.target.closest("[data-emulator-action]");
    if (!button || button.disabled || operation || busy) return;
    if (button.dataset.emulatorAction === "delete") {
      await openDelete(button.dataset.name);
      return;
    }
    operation = true;
    button.disabled = true;
    button.setAttribute("aria-busy", "true");
    feedback();
    try {
      await call(button.dataset.emulatorAction, button.dataset.name);
      await refresh();
    } catch (error) {
      feedback(error.message);
    } finally {
      button.removeAttribute("aria-busy");
      button.disabled = false;
      operation = false;
    }
  });

  panel.addEventListener("change", event => {
    if (event.target.id === "device" && rows.length) drawRows(find("#emulator-list"));
  });
  form.addEventListener("change", () => {
    name.value = "";
    profileOptions();
  });
  profile.addEventListener("change", () => { name.value = ""; });
  api.addEventListener("change", () => {
    name.value = "";
    imageOptions();
  });
  editor.addEventListener("cancel", event => {
    if (creating) event.preventDefault();
  });
  for (const close of editor.querySelectorAll("[data-emulator-close]")) {
    close.addEventListener("click", () => editor.close());
  }
  back.addEventListener("click", () => showStep(step - 1));
  next.addEventListener("click", () => {
    if (step === 2) create();
    else showStep(step + 1);
  });
})();
