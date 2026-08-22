// Guest-side preview runtime for the Tauri shell, injected into every preview
// child webview as an initialization script (after a per-tab
// `window.__T3P_TAB_ID` prelude and the Playwright InjectedScript install
// expression — see scripts/build-preview-runtime.mjs).
//
// Responsibilities, mirroring what Electron gets from CDP for free:
// - `__t3pPost`: fire JSON messages at the `t3preview://ipc/message` custom
//   protocol (the shell's WKURLSchemeHandler equivalent; the probe proved
//   this path). Every message carries the tab id.
// - eval results: the shell's `Webview::eval` is fire-and-forget, so scripts
//   the shell runs wrap themselves in `__t3pPost({kind:"result", id, ...})`.
// - navigation/title reporting (CDP Page domain equivalent).
// - console + network capture (CDP Runtime/Network equivalent; network is
//   partial by design: fetch/XHR monkeypatch only).
// - human-controller detection: trusted input events mark the tab
//   human-controlled for a short window, like Electron's before-input taps.
(() => {
  if (window.__t3pInstalled) {
    return;
  }
  window.__t3pInstalled = true;

  const tabId = window.__T3P_TAB_ID || "unknown";

  const post = (payload) => {
    try {
      const body = JSON.stringify({ tabId, ...payload });
      return fetch("t3preview://ipc/message", { method: "POST", body }).catch(() => false);
    } catch {
      return Promise.resolve(false);
    }
  };
  window.__t3pPost = (payload) => {
    try {
      JSON.stringify(payload.value);
    } catch {
      payload = {
        kind: "result",
        id: payload.id,
        ok: false,
        error: "Evaluation result is not JSON-serializable.",
      };
    }
    void post(payload);
  };

  // --- navigation + title reporting -------------------------------------
  const reportNav = (loading) => {
    void post({
      kind: "nav",
      url: location.href,
      title: document.title,
      loading,
    });
  };
  window.addEventListener("DOMContentLoaded", () => reportNav(true));
  window.addEventListener("load", () => reportNav(false));
  window.addEventListener("popstate", () => reportNav(false));
  window.addEventListener("hashchange", () => reportNav(false));
  // Back/forward restores can come from the back-forward cache without firing
  // load events; pageshow fires for every history restore.
  window.addEventListener("pageshow", () => reportNav(false));
  for (const method of ["pushState", "replaceState"]) {
    const original = history[method].bind(history);
    history[method] = (...args) => {
      const result = original(...args);
      queueMicrotask(() => reportNav(false));
      return result;
    };
  }
  const titleObserver = new MutationObserver(() => {
    void post({ kind: "title", title: document.title });
  });
  window.addEventListener("DOMContentLoaded", () => {
    const title = document.querySelector("title");
    if (title) {
      titleObserver.observe(title, { childList: true, characterData: true, subtree: true });
    }
  });

  // --- console capture ----------------------------------------------------
  const forwardConsole = (level, args) => {
    let text;
    try {
      text = args
        .map((value) => (typeof value === "string" ? value : JSON.stringify(value)))
        .join(" ");
    } catch {
      text = args.map(String).join(" ");
    }
    void post({ kind: "console", level, text: String(text).slice(0, 2000) });
  };
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      forwardConsole(level, args);
      original(...args);
    };
  }
  window.addEventListener("error", (event) => {
    forwardConsole("error", [event.message || String(event.error)]);
  });
  window.addEventListener("unhandledrejection", (event) => {
    forwardConsole("error", ["Unhandled promise rejection: " + String(event.reason)]);
  });

  // --- network capture (fetch/XHR monkeypatch; no subresource visibility) --
  const reportNet = (entry) => void post({ kind: "net", ...entry });
  const originalFetch = window.fetch.bind(window);
  window.fetch = (...args) => {
    const request = args[0];
    const url = String(request instanceof Request ? request.url : request);
    if (url.startsWith("t3preview://")) {
      return originalFetch(...args);
    }
    const method = String(
      (request instanceof Request ? request.method : args[1]?.method) || "GET",
    ).toUpperCase();
    return originalFetch(...args).then(
      (response) => {
        reportNet({ url, method, status: response.status, failed: !response.ok });
        return response;
      },
      (error) => {
        reportNet({ url, method, status: null, failed: true, errorText: String(error) });
        throw error;
      },
    );
  };
  const originalOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    this.__t3pRequest = { method: String(method).toUpperCase(), url: String(url) };
    return originalOpen.call(this, method, url, ...rest);
  };
  const originalSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function (...args) {
    const request = this.__t3pRequest;
    if (request) {
      this.addEventListener("loadend", () => {
        reportNet({
          url: request.url,
          method: request.method,
          status: this.status || null,
          failed: this.status === 0,
        });
      });
    }
    return originalSend.apply(this, args);
  };

  // --- human-controller detection ------------------------------------------
  // The shell suppresses this for a short window around synthetic agent input
  // (mirroring Electron's expectAgentInput), but synthetic events dispatched
  // from JS have isTrusted === false anyway, so trusted events are always a
  // real human unless WebKit itself synthesized them.
  let lastHumanPost = 0;
  const reportHuman = (event) => {
    if (!event.isTrusted) {
      return;
    }
    const now = Date.now();
    if (now - lastHumanPost < 250) {
      return;
    }
    lastHumanPost = now;
    void post({ kind: "human" });
  };
  for (const type of ["pointerdown", "keydown", "wheel"]) {
    window.addEventListener(type, reportHuman, { capture: true, passive: true });
  }

  reportNav(document.readyState !== "complete");
})();
