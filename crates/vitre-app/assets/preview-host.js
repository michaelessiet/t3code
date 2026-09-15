// Native host adapter around the pinned Playwright injected selector engine.
(() => {
  if (globalThis.__vitreHost) return;
  const logs = [],
    network = [],
    actions = [];
  const cap = (list) => {
    while (list.length > 100) list.shift();
  };
  for (const level of ["log", "info", "warn", "error"]) {
    const original = console[level];
    console[level] = function (...args) {
      logs.push({
        level,
        text: args
          .map((x) => {
            try {
              return typeof x === "string" ? x : JSON.stringify(x);
            } catch {
              return String(x);
            }
          })
          .join(" ")
          .slice(0, 2000),
        timestamp: new Date().toISOString(),
      });
      cap(logs);
      return original.apply(this, args);
    };
  }
  const fetch = globalThis.fetch;
  globalThis.fetch = async function (...args) {
    const entry = {
      url: String(args[0]?.url ?? args[0]),
      method: args[1]?.method ?? args[0]?.method ?? "GET",
      status: null,
      failed: false,
      timestamp: new Date().toISOString(),
    };
    try {
      const response = await fetch.apply(this, args);
      entry.status = response.status;
      return response;
    } catch (error) {
      entry.failed = true;
      entry.errorText = String(error);
      throw error;
    } finally {
      network.push(entry);
      cap(network);
    }
  };
  const selector = (element) => {
    if (element.id) return "#" + CSS.escape(element.id);
    const parts = [];
    while (element && parts.length < 12) {
      const name = element.localName;
      const siblings = element.parentElement
        ? [...element.parentElement.children].filter((e) => e.localName === name)
        : [element];
      parts.unshift(
        name + (siblings.length > 1 ? `:nth-of-type(${siblings.indexOf(element) + 1})` : ""),
      );
      element = element.parentElement;
    }
    return parts.join(" > ");
  };
  const visible = (e) => {
    const r = e.getBoundingClientRect();
    const s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
  };
  const target = (input) => {
    const locator = input.locator ?? input.selector;
    return locator
      ? __t3PlaywrightInjected.querySelector(
          __t3PlaywrightInjected.parseSelector(locator),
          document,
          true,
        )
      : document.activeElement;
  };
  const cursor = (e) => {
    let pointer = document.querySelector("[data-vitre-cursor]");
    if (!pointer) {
      pointer = document.createElement("div");
      pointer.dataset.vitreCursor = "";
      pointer.style.cssText =
        "position:fixed;width:16px;height:16px;border:2px solid white;border-radius:50%;background:#3973ffb0;z-index:2147483647;pointer-events:none;transition:left .12s,top .12s";
      document.documentElement.append(pointer);
    }
    const r = e.getBoundingClientRect();
    pointer.style.left = r.x + r.width / 2 + "px";
    pointer.style.top = r.y + r.height / 2 + "px";
    setTimeout(() => pointer.remove(), 1200);
  };
  const snapshot = () => ({
    url: location.href,
    title: document.title,
    loading: document.readyState !== "complete",
    visibleText: (document.body?.innerText ?? "").slice(0, 40000),
    interactiveElements: [
      ...document.querySelectorAll("a[href],button,input,textarea,select,[role],[tabindex]"),
    ]
      .filter(visible)
      .slice(0, 200)
      .map((e) => {
        const r = e.getBoundingClientRect();
        return {
          tag: e.localName,
          role: e.getAttribute("role"),
          name: (e.getAttribute("aria-label") ?? e.innerText ?? e.getAttribute("name") ?? "").slice(
            0,
            250,
          ),
          selector: selector(e),
          x: r.x,
          y: r.y,
          width: r.width,
          height: r.height,
        };
      }),
    accessibilityTree: __t3PlaywrightInjected.ariaSnapshot(document.body, { mode: "ai" }),
    consoleEntries: logs,
    networkEntries: network,
    actionTimeline: actions,
  });
  const execute = async (operation, input) => {
    if (operation === "snapshot") return snapshot();
    if (operation === "evaluate") return await (0, eval)(input.expression);
    if (operation === "waitFor") {
      const deadline = Date.now() + (input.timeoutMs ?? 15000);
      do {
        if (
          (!(input.locator ?? input.selector) || target(input)) &&
          (!input.text || (document.body?.innerText ?? "").includes(input.text)) &&
          (!input.urlIncludes || location.href.includes(input.urlIncludes))
        )
          return null;
        await new Promise((resolve) => setTimeout(resolve, 50));
      } while (Date.now() < deadline);
      throw new Error("Wait condition timed out");
    }
    const element =
      Number.isFinite(input.x) && Number.isFinite(input.y)
        ? document.elementFromPoint(input.x, input.y)
        : target(input);
    if (operation === "scroll") {
      ((input.locator ?? input.selector) ? element : window)?.scrollBy({
        left: input.deltaX ?? 0,
        top: input.deltaY ?? 0,
        behavior: "instant",
      });
      return null;
    }
    if (!element) throw new Error("Target not found");
    element.scrollIntoView({ block: "nearest", inline: "nearest" });
    cursor(element);
    if (operation === "click") {
      if (element.disabled) throw new Error("Target is disabled");
      element.focus();
      element.click();
      return null;
    }
    if (operation === "type") {
      element.focus();
      if (element.disabled || element.readOnly) throw new Error("Target is not editable");
      if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
        const prototype =
          element instanceof HTMLInputElement
            ? HTMLInputElement.prototype
            : HTMLTextAreaElement.prototype;
        const old = element.value,
          start = input.clear ? 0 : (element.selectionStart ?? old.length),
          end = input.clear ? old.length : (element.selectionEnd ?? old.length);
        Object.getOwnPropertyDescriptor(prototype, "value").set.call(
          element,
          old.slice(0, start) + input.text + old.slice(end),
        );
        element.setSelectionRange?.(start + input.text.length, start + input.text.length);
      } else if (element.isContentEditable) {
        if (input.clear) element.textContent = "";
        document.execCommand("insertText", false, input.text);
      } else throw new Error("Target is not editable");
      element.dispatchEvent(
        new InputEvent("input", { bubbles: true, inputType: "insertText", data: input.text }),
      );
      element.dispatchEvent(new Event("change", { bubbles: true }));
      return null;
    }
    if (operation === "press") {
      const modifiers = input.modifiers ?? [];
      const event = {
        key: input.key,
        bubbles: true,
        cancelable: true,
        altKey: modifiers.includes("Alt"),
        ctrlKey: modifiers.includes("Control"),
        metaKey: modifiers.includes("Meta"),
        shiftKey: modifiers.includes("Shift"),
      };
      if (element.dispatchEvent(new KeyboardEvent("keydown", event))) {
        if (input.key === "Enter" && element instanceof HTMLInputElement)
          element.form?.requestSubmit();
        if (input.key === "Tab") {
          const all = [
            ...document.querySelectorAll("a[href],button,input,textarea,select,[tabindex]"),
          ].filter((e) => visible(e) && !e.disabled && e.tabIndex >= 0);
          const i = all.indexOf(element);
          all[(i + (event.shiftKey ? -1 : 1) + all.length) % all.length]?.focus();
        }
      }
      element.dispatchEvent(new KeyboardEvent("keyup", event));
      return null;
    }
    throw new Error("Unsupported operation: " + operation);
  };
  let picker, pickerMove, pickerKey, highlight;
  const stopPicking = () => {
    document.removeEventListener("click", picker, true);
    document.removeEventListener("pointermove", pickerMove, true);
    document.removeEventListener("keydown", pickerKey, true);
    highlight?.remove();
    picker = pickerMove = pickerKey = highlight = null;
  };
  globalThis.__vitreHost = {
    async run(id, operation, input) {
      const action = {
        id,
        action: operation,
        status: "running",
        startedAt: new Date().toISOString(),
      };
      actions.push(action);
      cap(actions);
      try {
        const result = await execute(operation, input);
        action.status = "succeeded";
        window.ipc.postMessage(
          JSON.stringify({ kind: "automation", id, ok: true, result: result ?? null }),
        );
      } catch (error) {
        action.status = "failed";
        action.error = String(error);
        window.ipc.postMessage(
          JSON.stringify({ kind: "automation", id, ok: false, error: String(error) }),
        );
      } finally {
        action.completedAt = new Date().toISOString();
      }
    },
    pick() {
      if (picker) {
        stopPicking();
        return;
      }
      highlight = document.createElement("div");
      highlight.style.cssText =
        "position:fixed;pointer-events:none;z-index:2147483647;box-sizing:border-box;border:2px solid #568cff;background:#568cff25";
      document.documentElement.append(highlight);
      pickerMove = (event) => {
        const r = event.target.getBoundingClientRect();
        Object.assign(highlight.style, {
          left: r.x + "px",
          top: r.y + "px",
          width: r.width + "px",
          height: r.height + "px",
        });
      };
      pickerKey = (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopImmediatePropagation();
          stopPicking();
        }
      };
      picker = (event) => {
        event.preventDefault();
        event.stopImmediatePropagation();
        const element = event.target;
        const styles = getComputedStyle(element);
        stopPicking();
        window.ipc.postMessage(
          JSON.stringify({
            kind: "picked",
            url: location.href,
            title: document.title,
            selector: selector(element),
            text: (element.innerText ?? "").slice(0, 2000),
            html: element.outerHTML.slice(0, 6000),
            styles: {
              color: styles.color,
              backgroundColor: styles.backgroundColor,
              fontSize: styles.fontSize,
              display: styles.display,
            },
          }),
        );
      };
      document.addEventListener("click", picker, true);
      document.addEventListener("pointermove", pickerMove, true);
      document.addEventListener("keydown", pickerKey, true);
    },
  };
})();
