// Operate the same controls as the signed-in person. No selectors, scripts or API URLs
// are accepted from the assistant; actions use current, visible DOM controls.
(function (root) {
  "use strict";
  const clean = (v) => String(v || "").replace(/\s+/g, " ").trim();
  const norm = (v) => clean(v).normalize("NFKD").replace(/\p{M}+/gu, "").toLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim();
  const CONTROL = 'button,a[href],input,select,textarea,summary,[contenteditable="true"],[role="textbox"],[role="button"],[role="tab"],[role="menuitem"],[role="option"],[role="checkbox"],[role="switch"],[role="combobox"],[data-client-row],[data-top-route],[data-go],[data-nav],[data-action]';
  const EXCLUDE = '#drawer,.nxl,.nxl-panel,[data-nxt-workspace-picker],.signin,[inert]';
  function create(opts = {}) {
    const doc = root.document;
    let handles = new Map(), ids = new WeakMap(), sequence = 0, context = "", picker = null, picked = null;
    const ready = () => !!opts.allowed?.();
    function frameLoading(frame) {
      if (!frame.dataset.src) return false;
      try {
        const expected = new URL(frame.dataset.src, root.location.href), current = new URL(frame.contentWindow.location.href);
        return expected.pathname + expected.search !== current.pathname + current.search || frame.contentDocument?.readyState === "loading";
      } catch { return true; }
    }
    function visible(el) {
      if (!el?.isConnected || el.closest('[hidden],[aria-hidden="true"],template') || el.closest(EXCLUDE)) return false;
      const style = el.ownerDocument.defaultView.getComputedStyle(el);
      return style.visibility !== "hidden" && style.display !== "none" && !!el.getClientRects().length;
    }
    function documents() {
      const result = [{ doc, scope: "workspace" }];
      function visit(d, depth) {
        if (depth > 2) return;
        for (const frame of d.querySelectorAll("iframe")) {
          if (!visible(frame)) continue;
          if (frameLoading(frame)) continue;
          try { if (frame.contentDocument?.body && frame.contentWindow.location.origin === root.location.origin) {
            result.push({ doc: frame.contentDocument, scope: clean(frame.title) || "current app" });
            visit(frame.contentDocument, depth + 1);
          } } catch { /* external embedded content retains browser isolation */ }
        }
      }
      visit(doc, 0);
      return result;
    }
    function label(el) {
      const refs = clean(el.getAttribute("aria-labelledby")).split(" ").filter(Boolean).map(id => el.ownerDocument.getElementById(id)?.textContent).join(" ");
      const labels = [...(el.labels || [])].map(l => {
        const copy = l.cloneNode(true);
        copy.querySelectorAll("input,select,textarea,button").forEach(c => c.remove());
        return copy.textContent;
      }).join(" ");
      return clean(el.getAttribute("aria-label") || refs || labels || el.getAttribute("title") ||
        el.getAttribute("placeholder") || (el.matches('input[type="submit"],input[type="button"]') ? el.value : "") ||
        (!el.matches("input,textarea,select") ? el.textContent : "") || el.name || el.id).slice(0, 180);
    }
    function collect() {
      const docs = documents();
      const key = root.location.href + docs.map(d => d.doc.URL).join("|");
      if (key !== context) { context = key; handles = new Map(); ids = new WeakMap(); closePicker(); }
      const rows = [];
      for (const entry of docs) {
       const modal = [...entry.doc.querySelectorAll('[aria-modal="true"],dialog[open]')].filter(visible).at(-1);
       for (const el of (modal || entry.doc).querySelectorAll(CONTROL)) {
        if (!visible(el) || el.matches('input[type="hidden"],input[type="password"]') || /password|secret|api.?key|access.?token/i.test([el.name, el.id, el.autocomplete].join(" "))) continue;
        const name = label(el);
        if (!name) continue;
        let id = ids.get(el);
        if (!id) { id = "control-" + ++sequence; ids.set(el, id); }
        const row = { id, label: name, scope: entry.scope, kind: el.getAttribute("role") || (el.matches("input") ? el.type : el.tagName.toLowerCase()), disabled: el.matches(":disabled") || el.getAttribute("aria-disabled") === "true" };
        const surrounding = el.closest('tr,[role="row"],[data-client-row],fieldset');
        if (surrounding && surrounding !== el) row.context = clean(surrounding.textContent).slice(0, 240);
        if (el.matches("select")) row.selected = clean(el.selectedOptions[0]?.textContent);
        if (el.matches('input[type="checkbox"],input[type="radio"]')) row.checked = el.checked;
        if (el.hasAttribute("aria-expanded")) row.expanded = el.getAttribute("aria-expanded") === "true";
        const record = { el, row, context, aliases: [name, row.context && name + " " + row.context, el.id, el.name, el.title, el.dataset.go, el.dataset.nav, el.dataset.action, el.dataset.topRoute].filter(Boolean) };
        handles.set(id, record); rows.push(record);
       }
      }
      return rows;
    }
    const fail = (error, extra = {}) => ({ ok: false, error, ...extra });
    const success = (say, extra = {}) => ({ ok: true, say, route: root.location.hash.slice(1) || "/home", ...extra });
    function loading() {
      return [...doc.querySelectorAll("iframe")].some(f => visible(f) && frameLoading(f)) || documents().some(d => [...d.doc.querySelectorAll('[aria-busy="true"]')].some(visible));
    }
    async function wait() {
      const until = Date.now() + 5000;
      do {
        await new Promise(resolve => root.setTimeout(resolve, 50));
        if (!ready()) return fail("The workspace session ended.");
        if (!loading()) return success("The current view is ready.");
      } while (Date.now() < until);
      return success("The view is still loading. Inspect or wait again.", { loading: true });
    }
    function matches(rows, text, aliases = r => r.aliases) {
      const q = norm(text).replace(/^(?:the )/, "").replace(/ (?:button|dropdown|drop down|menu|tab|field)$/, "");
      if (!q) return [];
      const exact = rows.filter(r => aliases(r).some(a => norm(a) === q));
      if (exact.length) return exact;
      const words = q.split(" ");
      return rows.filter(r => aliases(r).some(a => words.every(w => norm(a).split(" ").includes(w))));
    }
    function find(target) {
      const rows = collect();
      if (handles.has(target)) {
        const r = handles.get(target);
        return r.context === context && r.el.isConnected && visible(r.el) && rows.some(x => x.el === r.el) ? r : fail("That control changed. Inspect the workspace again.");
      }
      if (/^control-\d+$/.test(target || "")) return fail("That control belongs to an earlier screen. Inspect again.");
      const found = matches(rows, target);
      return found.length === 1 ? found[0] : fail(found.length ? "Which control? More than one matches." : "No visible control matches that name.", { choices: found.slice(0, 15).map(r => r.row) });
    }
    function options(el, query = "", offset = 0) {
      const all = [...el.options].filter(o => !o.disabled && !o.hidden && (!query || norm(o.textContent + " " + o.value).includes(norm(query))));
      return { total_options: all.length, options: all.slice(offset, offset + 30).map(o => ({ label: clean(o.textContent).slice(0, 180), value: o.value, selected: o.selected })), next_offset: offset + 30 < all.length ? offset + 30 : null };
    }
    function closePicker() { picker?.remove(); picker = null; picked = null; }
    function choose(el, value) {
      const all = [...el.options].filter(o => !o.disabled && !o.hidden);
      const exact = all.filter(o => o.value === value || norm(o.textContent) === norm(value));
      const found = exact.length ? exact : all.filter(o => norm(o.textContent).includes(norm(value)));
      if (found.length !== 1) return fail(found.length ? "Which option? More than one matches." : "That option is not available.", options(el, value));
      el.value = found[0].value;
      const Event = el.ownerDocument.defaultView.Event;
      el.dispatchEvent(new Event("input", { bubbles: true })); el.dispatchEvent(new Event("change", { bubbles: true }));
      closePicker();
      return success("Selected " + clean(found[0].textContent) + ".");
    }
    function showPicker(el) {
      closePicker(); picked = { el, context };
      const panel = doc.createElement("section"); picker = panel;
      panel.dataset.nxtWorkspacePicker = "1"; panel.setAttribute("role", "dialog"); panel.setAttribute("aria-label", label(el) + " options");
      panel.style.cssText = "position:fixed;top:72px;left:90px;z-index:70;width:min(400px,80vw);max-height:70vh;overflow:auto;padding:16px;border:1px solid var(--line);border-radius:12px;background:var(--panel);color:var(--text);box-shadow:var(--shadow)";
      const title = doc.createElement("strong"); title.textContent = label(el) + " options";
      const close = doc.createElement("button"); close.textContent = "Close"; close.className = "btn sm"; close.onclick = closePicker;
      const search = doc.createElement("input"); search.type = "search"; search.placeholder = "Filter options"; search.setAttribute("aria-label", "Filter " + label(el) + " options"); search.style.cssText = "display:block;width:100%;margin:12px 0";
      const list = doc.createElement("div");
      const paint = () => {
        list.replaceChildren();
        for (const option of options(el, search.value).options) {
          const b = doc.createElement("button"); b.className = "btn"; b.style.cssText = "display:block;width:100%;text-align:left;margin:4px 0";
          b.textContent = option.label; b.onclick = () => { if (ready() && visible(el) && picked?.context === context) choose(el, option.value); else closePicker(); };
          list.appendChild(b);
        }
      };
      search.oninput = paint; panel.append(title, close, search, list); doc.body.appendChild(panel); paint();
    }
    function inspect(args = {}) {
      if (!ready()) return fail("Sign in with access to this workspace first.");
      const rows = collect(), q = norm(args.query), offset = Math.max(0, Math.min(10000, Number(args.offset) || 0));
      const selected = q ? rows.filter(r => norm([r.row.label, r.row.scope, ...r.aliases].join(" ")).includes(q)) : rows;
      const destinations = (opts.destinations?.() || []).filter(d => !q || norm(d.label).includes(q)).slice(0, 60).map(d => d.label);
      if (args.target) { const r = find(args.target); if (!r.el) return r; return success(r.row.label, { control: r.row, ...(r.el.matches("select") ? options(r.el, args.query, offset) : {}) }); }
      return success("Found " + selected.length + " controls.", { loading: loading(), controls: selected.slice(offset, offset + 40).map(r => r.row), total_controls: selected.length, next_offset: offset + 40 < selected.length ? offset + 40 : null, destinations });
    }
    async function act(args = {}) {
      if (!ready()) return fail("Sign in with access to this workspace first.");
      const action = args.action;
      if (action === "wait") return wait();
      if (action === "navigate") {
        const destinations = opts.destinations?.() || [];
        const found = matches(destinations, args.target, d => [d.label]);
        if (found.length !== 1) return fail(found.length ? "Which destination?" : "Inspect the workspace for available destinations, or open its navigation control.", { choices: found.slice(0, 15).map(d => d.label) });
        found[0].run(); closePicker();
        const state = await wait();
        return state.ok ? success("Opened " + found[0].label + ".", { loading: !!state.loading }) : state;
      }
      if (action === "dismiss") { closePicker(); return success("Closed the option list."); }
      if (action === "scroll") {
        const dir = args.value || "down", delta = { down: [0, 0.7], up: [0, -0.7], right: [0.7, 0], left: [-0.7, 0] }[dir];
        if (!delta) return fail("Scroll direction must be up, down, left or right.");
        const d = documents().at(-1).doc;
        const r = args.target ? find(args.target) : null;
        if (r && !r.el) return r;
        let el = r?.el || d.activeElement;
        while (el && el !== d.body && !(el.scrollHeight > el.clientHeight + 2 && /auto|scroll/.test(d.defaultView.getComputedStyle(el).overflowY))) el = el.parentElement;
        el = el && el !== d.body ? el : [...d.querySelectorAll('main,[role="main"],.home')].find(e => visible(e) && e.scrollHeight > e.clientHeight) || d.scrollingElement;
        el.scrollBy({ left: delta[0] * (el.clientWidth || 800), top: delta[1] * (el.clientHeight || 600), behavior: "instant" });
        return success("Scrolled " + dir + ".");
      }
      const r = find(args.target);
      if (!r.el) return r;
      const el = r.el;
      if (r.row.disabled) return fail("That control is disabled. " + r.row.label);
      el.scrollIntoView({ block: "nearest", inline: "nearest" });
      el.focus();
      if (action === "select") {
        if (!el.matches("select")) return fail("That is not a select. Inspect its menu and click the requested option.");
        return choose(el, String(args.value ?? ""));
      }
      if (action === "fill") {
        if (!el.matches('textarea,[contenteditable="true"],input:not([type="file"]):not([type="checkbox"]):not([type="radio"]):not([type="button"]):not([type="submit"])') || el.readOnly) return fail("That field cannot be filled by the workspace controller.");
        if (String(args.value ?? "").length > 10000) return fail("The value is too long.");
        const win = el.ownerDocument.defaultView, proto = el.matches("textarea") ? win.HTMLTextAreaElement.prototype : win.HTMLInputElement.prototype;
        if (el.isContentEditable) el.textContent = String(args.value ?? "");
        else Object.getOwnPropertyDescriptor(proto, "value").set.call(el, String(args.value ?? ""));
        el.dispatchEvent(new win.Event("input", { bubbles: true })); el.dispatchEvent(new win.Event("change", { bubbles: true }));
        return success("Filled " + r.row.label + ".");
      }
      if (action === "check") {
        if (!el.matches('input[type="checkbox"],input[type="radio"]')) return fail("That is not a checkbox or radio button.");
        const checked = args.value === true || args.value === "true" || args.value === "on";
        if (el.checked !== checked) el.click();
        return success((el.checked ? "Checked " : "Unchecked ") + r.row.label + ".");
      }
      if (action === "press") {
        const parts = String(args.value || "").split("+"), key = parts.pop();
        const modifiers = { ctrlKey: parts.includes("Ctrl"), metaKey: parts.includes("Meta"), altKey: parts.includes("Alt"), shiftKey: parts.includes("Shift") };
        if (!parts.every(p => ["Ctrl", "Meta", "Alt", "Shift"].includes(p)) || !(["Enter", "Escape", "Tab", "ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", " "].includes(key) || /^(?:F[1-9]|F1[0-2])$/.test(key) || (parts.length && /^[a-z]$/i.test(key)))) return fail("Unsupported application shortcut.");
        el.focus();
        const win = el.ownerDocument.defaultView;
        const proceed = el.dispatchEvent(new win.KeyboardEvent("keydown", { key, ...modifiers, bubbles: true, cancelable: true }));
        el.dispatchEvent(new win.KeyboardEvent("keyup", { key, ...modifiers, bubbles: true }));
        if (proceed && ((modifiers.ctrlKey || modifiers.metaKey || modifiers.altKey) || /^F\d+$/.test(key)) && !((modifiers.ctrlKey || modifiers.metaKey) && key.toLowerCase() === "a" && el.select)) return fail("That shortcut was not handled by this app.");
        if (proceed && (modifiers.ctrlKey || modifiers.metaKey) && key.toLowerCase() === "a") el.select();
        if (proceed && key === "Tab") {
          const rows = collect().filter(c => !c.row.disabled);
          rows[(rows.findIndex(c => c.el === el) + (modifiers.shiftKey ? rows.length - 1 : 1)) % rows.length]?.el.focus();
        }
        if (proceed && el.matches("select") && ["ArrowUp", "ArrowDown"].includes(key)) {
          const all = [...el.options].filter(o => !o.disabled && !o.hidden);
          const i = all.findIndex(o => o.selected), next = Math.max(0, Math.min(all.length - 1, i + (key === "ArrowUp" ? -1 : 1)));
          if (all[next]) return choose(el, all[next].value);
        }
        if (proceed && key === " " && el.matches('input[type="checkbox"],input[type="radio"],button,summary')) el.click();
        if (proceed && key === "Enter") {
          if (el.matches("button,a,summary")) el.click();
          else if (el.form && el.matches("input")) el.form.requestSubmit();
        }
        return success("Pressed " + args.value + " on " + r.row.label + ".");
      }
      if (action === "focus" || (action === "click" && el.matches("select"))) {
        el.focus(); if (el.matches("select")) showPicker(el);
        return success("Opened " + r.row.label + ".", el.matches("select") ? options(el) : {});
      }
      if (action !== "click") return fail("Unknown workspace action.");
      if (el.matches("a[href]")) {
        const url = new URL(el.href, el.ownerDocument.URL);
        if (url.origin !== root.location.origin || !/^https?:$/.test(url.protocol)) return fail("That link opens outside NXT OS. Use the visible link directly.");
      }
      el.click(); return success("Activated " + r.row.label + ".");
    }
    // The same deterministic UI commands work with browser speech and typed messages.
    // Unrecognised requests continue through Nixon's finance and Ask NXT tools.
    async function command(text) {
      if (!ready()) return null;
      const t = clean(text).replace(/^(?:hey |okay |ok )?nixon[, ]*/i, "").replace(/^please /i, "").replace(/[.!?]+$/, "");
      let m;
      if (/^(?:list|show|inspect|what are)(?: the| available)? (?:controls|buttons|menus|task ?bar|toolbar|navigation|dropdowns|drop downs)$/i.test(t)) return inspect();
      if ((m = t.match(/^(?:switch|change|set)(?: the)? (?:company|workspace) to (.+)$/i))) return act({ action: "select", target: "Company", value: m[1] });
      if ((m = t.match(/^(?:select|choose) (.+?) (?:from|in)(?: the)? (.+)$/i))) return act({ action: "select", target: m[2], value: m[1] });
      if ((m = t.match(/^(?:select|choose)(?: option)? (.+)$/i)) && picked) {
        collect();
        if (!picked || !visible(picked.el)) return fail("That dropdown changed. Open it again.");
        const ordinals = { first: 1, one: 1, second: 2, two: 2, third: 3, three: 3, fourth: 4, four: 4, fifth: 5, five: 5 };
        const index = ordinals[norm(m[1])] || (/^\d+$/.test(m[1]) ? Number(m[1]) : null);
        const option = index ? options(picked.el).options[index - 1] : null;
        return choose(picked.el, option ? option.value : m[1]);
      }
      if ((m = t.match(/^(?:fill|type|enter|set) (.+?) (?:into|in)(?: the)? (.+)$/i))) return act({ action: "fill", target: m[2], value: m[1] });
      if ((m = t.match(/^(check|uncheck)(?: the)? (.+)$/i))) return act({ action: "check", target: m[2], value: m[1].toLowerCase() === "check" });
      if ((m = t.match(/^scroll (up|down|left|right)$/i))) return act({ action: "scroll", value: m[1].toLowerCase() });
      if ((m = t.match(/^(?:click|tap|press)(?: the)? (.+)$/i))) {
        if (/^(enter|escape|tab|arrow (?:up|down|left|right))$/i.test(m[1])) {
          const active = documents().at(-1).doc.activeElement;
          const row = collect().find(r => r.el === active);
          if (!row) return fail("Choose a field or control first.");
          const key = m[1].toLowerCase().startsWith("arrow ") ? "Arrow" + m[1].slice(6,7).toUpperCase() + m[1].slice(7).toLowerCase() : m[1][0].toUpperCase() + m[1].slice(1).toLowerCase();
          return act({ action: "press", target: row.row.id, value: key });
        }
        return act({ action: "click", target: m[1] });
      }
      if ((m = t.match(/^(?:open|show|go to|navigate to)(?: the)? (.+)$/i))) {
        const rows = collect();
        const found = matches(rows, m[1]);
        if (found.length) return act({ action: "click", target: found.length === 1 ? found[0].row.id : m[1] });
        const dest = matches(opts.destinations?.() || [], m[1], d => [d.label]);
        if (dest.length) return act({ action: "navigate", target: m[1] });
      }
      return null;
    }
    return { inspect, act, command, reset() { closePicker(); handles.clear(); ids = new WeakMap(); context = ""; } };
  }
  root.NXTWorkspace = { create };
})(typeof window !== "undefined" ? window : globalThis);
