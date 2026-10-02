export function init(api) {
  if (api.boundModules.has("popover")) return api.popover;
  api.boundModules.add("popover");
  const byId = api.byId;
  const popover = byId("thinkingEffortPopover");
  const trigger = byId("thinkingEffort");

  function position() {
    if (!popover || !trigger || popover.hidden) return;
    const rect = trigger.getBoundingClientRect();
    const box = popover.getBoundingClientRect();
    const scale = box.width / popover.offsetWidth;
    if (!Number.isFinite(scale) || scale <= 0) throw new Error("Popover layout is unavailable");
    const top = Math.min(window.innerHeight - Math.max(190, box.height + 16), rect.bottom + 6);
    const left = Math.min(window.innerWidth - box.width - 10, Math.max(16, rect.left - 150));
    popover.style.top = top / scale + "px";
    popover.style.left = left / scale + "px";
    popover.style.bottom = "auto";
    popover.style.right = "auto";
  }

  function close() {
    if (popover) popover.hidden = true;
    trigger?.setAttribute("aria-expanded", "false");
  }

  function toggle(event) {
    event?.stopPropagation?.();
    if (!popover || !trigger) return;
    const opening = popover.hidden;
    api.beforeOpen?.();
    if (!opening) { close(); return; }
    popover.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    position();
    byId("thinkingEffortSlider")?.focus();
  }

  function renderChoice(options, index) {
    const slider = byId("thinkingEffortSlider");
    const selected = options[index];
    if (!slider || !selected) throw new Error("Thinking effort choice is unavailable");
    slider.min = "0";
    slider.max = String(options.length - 1);
    slider.value = String(index);
    slider.disabled = options.length === 1;
    slider.setAttribute("aria-valuetext", selected.label);
    slider.style.setProperty("--thinking-progress", (options.length > 1 ? index / (options.length - 1) * 100 : 0) + "%");
    byId("thinkingEffortPickerValue").textContent = selected.label;
    const marks = byId("thinkingEffortMarks");
    marks.replaceChildren(...options.map((option, markIndex) => {
      const mark = document.createElement("span");
      mark.className = "thinking-level-mark" + (markIndex === index ? " active" : "");
      mark.title = option.label;
      return mark;
    }));
  }

  api.popover = Object.freeze({ position, toggle, close, renderChoice });

  trigger?.addEventListener("click", toggle);
  const slider = api.byId("thinkingEffortSlider");
  slider?.addEventListener("input", event => api.previewThinkingEffort(event.currentTarget.value));
  slider?.addEventListener("change", event => api.selectThinkingEffort(event.currentTarget.value));
  return api.popover;
}
