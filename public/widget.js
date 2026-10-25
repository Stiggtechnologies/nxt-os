/*!
 * Referral Suite widget. Embed on any storefront:
 *
 *   <script src="https://YOUR-DEPLOYMENT/widget.js"
 *           data-email="customer@example.com" data-name="Jane Doe"
 *           data-signature="optional-hmac" async></script>
 *
 * Without data-email the widget asks for the customer's email first.
 * Open it programmatically with window.ReferralSuite.open().
 */
(function () {
  "use strict";
  var script = document.currentScript;
  var base = script ? new URL(script.src).origin : location.origin;
  var cfg = {
    email: script && script.dataset.email,
    name: (script && script.dataset.name) || "",
    signature: script && script.dataset.signature,
    launcher: !(script && script.dataset.launcher === "false"),
  };
  var state = { offer: null, advocate: null };

  function api(path, body) {
    return fetch(base + path, body ? {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    } : undefined).then(function (r) {
      return r.json().then(function (j) { if (!r.ok) throw new Error(j.error || r.status); return j; });
    });
  }

  var css = "\
.rsw-launch{position:fixed;right:20px;bottom:20px;z-index:2147483000;background:#111;color:#fff;border:0;border-radius:999px;padding:12px 18px;font:600 14px/1 system-ui,sans-serif;box-shadow:0 6px 24px rgba(0,0,0,.25);cursor:pointer}\
.rsw-launch:hover{transform:translateY(-1px)}\
.rsw-back{position:fixed;inset:0;z-index:2147483001;background:rgba(10,10,20,.55);display:flex;align-items:center;justify-content:center;padding:16px}\
.rsw-card{background:#fff;color:#111;width:100%;max-width:420px;border-radius:16px;padding:28px 24px 22px;font:15px/1.45 system-ui,sans-serif;position:relative;box-shadow:0 20px 60px rgba(0,0,0,.3)}\
.rsw-card h2{margin:0 0 6px;font-size:24px;letter-spacing:-.01em}\
.rsw-card p{margin:0 0 16px;color:#444}\
.rsw-x{position:absolute;top:10px;right:12px;background:none;border:0;font-size:22px;cursor:pointer;color:#666}\
.rsw-row{display:flex;gap:8px}\
.rsw-in{flex:1;min-width:0;border:1px solid #ccc;border-radius:10px;padding:10px 12px;font:inherit}\
.rsw-btn{background:#111;color:#fff;border:0;border-radius:10px;padding:10px 14px;font:600 14px system-ui,sans-serif;cursor:pointer;white-space:nowrap}\
.rsw-ch{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-top:12px}\
.rsw-ch a,.rsw-ch button{display:block;text-align:center;border:1px solid #ddd;border-radius:10px;padding:9px 4px;color:#111;text-decoration:none;font:500 13px system-ui,sans-serif;background:#fafafa;cursor:pointer}\
.rsw-ch a:hover,.rsw-ch button:hover{background:#f0f0f0}\
.rsw-stats{display:flex;justify-content:space-between;margin-top:16px;padding-top:12px;border-top:1px solid #eee;font-size:13px;color:#555}\
.rsw-stats b{display:block;font-size:18px;color:#111}\
.rsw-err{color:#b00020;font-size:13px;margin-top:8px}";

  function el(tag, attrs, html) {
    var e = document.createElement(tag);
    for (var k in attrs || {}) e.setAttribute(k, attrs[k]);
    if (html != null) e.innerHTML = html;
    return e;
  }
  function esc(s) { return String(s).replace(/[&<>"']/g, function (c) { return "&#" + c.charCodeAt(0) + ";"; }); }

  function close() { var b = document.querySelector(".rsw-back"); if (b) b.remove(); }

  function render(card) {
    var o = state.offer, a = state.advocate;
    if (!a) {
      card.innerHTML = '<button class="rsw-x" aria-label="Close">&times;</button>' +
        "<h2>" + esc(o.headline) + "</h2><p>" + esc(o.advocate_copy) + "</p>" +
        '<form class="rsw-row"><input class="rsw-in" type="email" required placeholder="Your email" aria-label="Your email">' +
        '<button class="rsw-btn">Get my link</button></form><div class="rsw-err"></div>';
      card.querySelector("form").onsubmit = function (ev) {
        ev.preventDefault();
        cfg.email = card.querySelector("input").value;
        enroll(card);
      };
    } else {
      var url = a.share_url, msg = o.friend_copy + " " + url, enc = encodeURIComponent;
      card.innerHTML = '<button class="rsw-x" aria-label="Close">&times;</button>' +
        "<h2>" + esc(o.headline) + "</h2><p>" + esc(o.advocate_copy) + "</p>" +
        '<div class="rsw-row"><input class="rsw-in" readonly value="' + esc(url) + '" aria-label="Your referral link">' +
        '<button class="rsw-btn" data-ch="copy">Copy</button></div>' +
        '<div class="rsw-ch">' +
        '<a data-ch="email" href="mailto:?subject=' + enc("A gift from " + o.brand_name) + "&body=" + enc(msg) + '">Email</a>' +
        '<a data-ch="sms" href="sms:?&body=' + enc(msg) + '">Text</a>' +
        '<a data-ch="whatsapp" target="_blank" rel="noopener" href="https://wa.me/?text=' + enc(msg) + '">WhatsApp</a>' +
        '<a data-ch="facebook" target="_blank" rel="noopener" href="https://www.facebook.com/sharer/sharer.php?u=' + enc(url) + '">Facebook</a>' +
        '<a data-ch="x" target="_blank" rel="noopener" href="https://x.com/intent/post?text=' + enc(msg) + '">X</a>' +
        '<a data-ch="linkedin" target="_blank" rel="noopener" href="https://www.linkedin.com/sharing/share-offsite/?url=' + enc(url) + '">LinkedIn</a>' +
        "</div>" +
        '<div class="rsw-stats"><span><b>' + a.stats.clicks + "</b>clicks</span><span><b>" + a.stats.referrals +
        "</b>friends joined</span><span><b>" + a.stats.rewards_earned + "</b>rewards</span></div>";
      card.querySelectorAll("[data-ch]").forEach(function (b) {
        b.addEventListener("click", function () {
          var ch = b.getAttribute("data-ch");
          if (ch === "copy") {
            (navigator.clipboard ? navigator.clipboard.writeText(url) : Promise.reject()).catch(function () {
              card.querySelector(".rsw-in").select(); document.execCommand("copy");
            });
            b.textContent = "Copied!";
            setTimeout(function () { b.textContent = "Copy"; }, 1500);
          }
          api("/api/public/share", { code: a.code, channel: ch }).catch(function () {});
        });
      });
    }
    card.querySelector(".rsw-x").onclick = close;
  }

  function enroll(card) {
    api("/api/public/advocate", { email: cfg.email, name: cfg.name, signature: cfg.signature })
      .then(function (a) { state.advocate = a; render(card); })
      .catch(function (e) {
        var err = card.querySelector(".rsw-err");
        if (err) err.textContent = "Couldn't create your link: " + e.message;
      });
  }

  function open() {
    if (document.querySelector(".rsw-back")) return;
    var back = el("div", { class: "rsw-back", role: "dialog", "aria-modal": "true" });
    var card = el("div", { class: "rsw-card" }, "<p>Loading…</p>");
    back.appendChild(card);
    back.addEventListener("click", function (e) { if (e.target === back) close(); });
    document.addEventListener("keydown", function k(e) { if (e.key === "Escape") { close(); document.removeEventListener("keydown", k); } });
    document.body.appendChild(back);
    var ready = state.offer ? Promise.resolve() : api("/api/public/offer").then(function (o) { state.offer = o; });
    ready.then(function () {
      if (cfg.email && !state.advocate) { render(card); enroll(card); } else render(card);
    }).catch(function (e) { card.innerHTML = '<p class="rsw-err">Referral program unavailable (' + esc(e.message) + ")</p>"; });
  }

  function init() {
    document.head.appendChild(el("style", {}, css));
    if (cfg.launcher) {
      api("/api/public/offer").then(function (o) {
        state.offer = o;
        var b = el("button", { class: "rsw-launch", type: "button" }, esc(o.headline));
        b.onclick = open;
        document.body.appendChild(b);
      }).catch(function () {});
    }
  }

  window.ReferralSuite = { open: open, identify: function (email, name, sig) { cfg.email = email; cfg.name = name || ""; cfg.signature = sig; state.advocate = null; } };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init); else init();
})();
