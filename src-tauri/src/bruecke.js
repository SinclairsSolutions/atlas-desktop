// Atlas Desktop — wird vor jedem Seitenskript in den Hauptframe eingespielt.
// Wirkt nur auf Atlas-Seiten; auf allen anderen Seiten passiert nichts.
//
// Was es tut:
//  - `window.__ATLAS_DESKTOP__` setzen, damit Atlas die App erkennen kann
//  - `Notification` → System-Mitteilungen, `navigator.setAppBadge` → Dock-Zahl,
//    `window.print` → Druckdialog (macOS)
//  - Klicks und Navigationen zu fremden Adressen in den Standardbrowser
//  - Hinweis-Leiste bei fehlender Verbindung
(function () {
  "use strict";
  var HOSTS = __ATLAS_HOSTS__;
  var ORIGINS = __ATLAS_ORIGINS__;

  function istAtlas(adresse) {
    try {
      var u = new URL(adresse, location.href);
      if (ORIGINS.indexOf(u.origin) !== -1) return true;
      return u.protocol === "https:" && HOSTS.indexOf(u.hostname) !== -1;
    } catch (_) {
      return false;
    }
  }

  if (window.__ATLAS_DESKTOP__ || !istAtlas(location.href)) return;

  function invoke(befehl, args) {
    var i = window.__TAURI_INTERNALS__;
    if (!i || typeof i.invoke !== "function") return Promise.reject(new Error("keine Brücke"));
    return i.invoke(befehl, args || {});
  }

  Object.defineProperty(window, "__ATLAS_DESKTOP__", {
    value: Object.freeze({
      version: "__VERSION__",
      plattform: "__PLATTFORM__",
      // Deep-Link-Schema für Rücksprünge aus dem Browser, z. B.
      // "__SCHEMA__://open/workspaces/<id>/communication/mail"
      schema: "__SCHEMA__",
    }),
    enumerable: false,
  });

  // ---------------------------------------------------------------------------
  // Mitteilungen
  // ---------------------------------------------------------------------------
  // Erlaubnis verhält sich wie im Browser: "default", bis Atlas fragt.
  var SCHLUESSEL = "atlas.desktop.mitteilungen";
  function gespeicherteErlaubnis() {
    try {
      var v = localStorage.getItem(SCHLUESSEL);
      return v === "granted" || v === "denied" ? v : "default";
    } catch (_) {
      return "default";
    }
  }
  var erlaubnis = gespeicherteErlaubnis();

  function ereignis(ziel, name) {
    var e = new Event(name);
    ziel.dispatchEvent(e);
    var h = ziel["on" + name];
    if (typeof h === "function") {
      try {
        h.call(ziel, e);
      } catch (err) {
        console.error(err);
      }
    }
  }

  function AtlasNotification(titel, optionen) {
    if (!(this instanceof AtlasNotification)) {
      throw new TypeError("Notification muss mit new aufgerufen werden");
    }
    var o = optionen || {};
    var ziel = new EventTarget();
    this.addEventListener = ziel.addEventListener.bind(ziel);
    this.removeEventListener = ziel.removeEventListener.bind(ziel);
    this.dispatchEvent = ziel.dispatchEvent.bind(ziel);
    this.title = String(titel);
    this.body = o.body ? String(o.body) : "";
    this.tag = o.tag ? String(o.tag) : "";
    this.icon = o.icon || "";
    this.data = o.data === undefined ? null : o.data;
    this.silent = !!o.silent;
    this.onclick = this.onshow = this.onclose = this.onerror = null;
    var self = this;
    if (erlaubnis !== "granted") {
      setTimeout(function () {
        ereignis(self, "error");
      }, 0);
      return;
    }
    invoke("mitteilung", { titel: this.title, text: this.body }).then(
      function () {
        ereignis(self, "show");
      },
      function () {
        ereignis(self, "error");
      },
    );
  }
  AtlasNotification.prototype.close = function () {
    ereignis(this, "close");
  };
  Object.defineProperty(AtlasNotification, "permission", {
    get: function () {
      return erlaubnis;
    },
  });
  Object.defineProperty(AtlasNotification, "maxActions", { value: 0 });
  AtlasNotification.requestPermission = function (rueckruf) {
    return invoke("mitteilungen_erlauben")
      .catch(function () {
        return "denied";
      })
      .then(function (r) {
        erlaubnis = r === "granted" || r === "denied" ? r : "default";
        try {
          if (erlaubnis !== "default") localStorage.setItem(SCHLUESSEL, erlaubnis);
        } catch (_) {}
        if (typeof rueckruf === "function") rueckruf(erlaubnis);
        return erlaubnis;
      });
  };
  Object.defineProperty(window, "Notification", {
    value: AtlasNotification,
    writable: true,
    configurable: true,
  });

  // ---------------------------------------------------------------------------
  // Zahl am Dock-/Taskleistensymbol (Badging API)
  // ---------------------------------------------------------------------------
  navigator.setAppBadge = function (anzahl) {
    if (anzahl === undefined) return invoke("kennzahl", { anzahl: null, punkt: true });
    var n = Math.max(0, Math.floor(Number(anzahl) || 0));
    return invoke("kennzahl", { anzahl: n, punkt: false });
  };
  navigator.clearAppBadge = function () {
    return invoke("kennzahl", { anzahl: null, punkt: false });
  };

  // ---------------------------------------------------------------------------
  // Drucken (macOS: WebKit ignoriert window.print in eingebetteten Ansichten)
  // ---------------------------------------------------------------------------
  if ("__PLATTFORM__" === "macos") {
    window.print = function () {
      invoke("drucken").catch(function (e) {
        console.warn("[atlas-desktop] Drucken fehlgeschlagen", e);
      });
    };
  }

  // ---------------------------------------------------------------------------
  // Fremde Adressen → Standardbrowser
  // ---------------------------------------------------------------------------
  function istFremdWeb(adresse) {
    return /^https?:/i.test(adresse) && !istAtlas(adresse);
  }
  function extern(adresse) {
    invoke("extern_oeffnen", { url: String(adresse) }).catch(function (e) {
      console.warn("[atlas-desktop] extern öffnen fehlgeschlagen", e);
    });
  }

  // `window.open("")` (TIFF-Ansicht, leere Fenster zum Hineinschreiben) kommt
  // bei Tauri ohne Adresse an und wird verworfen — als about:blank ist es gültig.
  var urspruenglichOpen = window.open;
  window.open = function (adresse, ziel, merkmale) {
    var a = adresse == null || String(adresse) === "" ? "about:blank" : adresse;
    return urspruenglichOpen.call(window, a, ziel, merkmale);
  };

  // Navigation API (WebKit ab Safari 26, WebView2): erwischt auch
  // `location.href = …` und Formular-Sendungen, bevor die Seite wechselt.
  if (window.navigation && typeof window.navigation.addEventListener === "function") {
    window.navigation.addEventListener("navigate", function (e) {
      var ziel = e.destination && e.destination.url;
      if (!ziel || !istFremdWeb(ziel)) return;
      if (e.downloadRequest != null || !e.cancelable) return;
      e.preventDefault();
      extern(ziel);
    });
  }

  // Downloads aus blob:/data:-Adressen (Atlas erzeugt PDFs, Excel, DATEV im
  // Browser): WebKit meldet sie nicht als Download. Die Brücke liest die Datei
  // und lässt sie von der App im Download-Ordner speichern.
  function alsBase64(puffer) {
    var bytes = new Uint8Array(puffer);
    var teile = [];
    for (var i = 0; i < bytes.length; i += 0x8000) {
      teile.push(String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000)));
    }
    return btoa(teile.join(""));
  }
  function dateiSpeichern(href, name) {
    return fetch(href)
      .then(function (r) {
        return r.arrayBuffer();
      })
      .then(function (puffer) {
        return invoke("datei_speichern", { name: name || "Download", inhalt: alsBase64(puffer) });
      })
      .catch(function (e) {
        console.warn("[Atlas Desktop] Download fehlgeschlagen", e);
      });
  }
  function istSelbstDownload(a) {
    return a && a.hasAttribute && a.hasAttribute("download") && /^(blob|data):/i.test(a.href || "");
  }
  {
    // Programmatisch: a.click() an einem (oft nicht eingehängten) Link.
    var urspruenglichKlick = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function () {
      if (istSelbstDownload(this)) {
        dateiSpeichern(this.href, this.getAttribute("download"));
        return;
      }
      return urspruenglichKlick.call(this);
    };
    // Echte Klicks auf Download-Links.
    document.addEventListener(
      "click",
      function (e) {
        var a = e.target && e.target.closest ? e.target.closest("a[download]") : null;
        if (!istSelbstDownload(a)) return;
        e.preventDefault();
        dateiSpeichern(a.href, a.getAttribute("download"));
      },
      true,
    );
  }

  // Ältere WebKit-Versionen: wenigstens Link-Klicks. Läuft nach den
  // React-Handlern (Bubble-Phase am document), respektiert deren preventDefault.
  document.addEventListener("click", function (e) {
    if (e.defaultPrevented || e.button !== 0) return;
    var a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    if (!a || a.hasAttribute("download")) return;
    var href = a.href;
    if (istFremdWeb(href) || /^(mailto|tel):/i.test(href)) {
      e.preventDefault();
      extern(href);
    }
  });

  // ---------------------------------------------------------------------------
  // Hinweis-Leiste (Offline, Anmeldung im Browser) — weich, Milchglas
  // ---------------------------------------------------------------------------
  var leiste = null;
  var zeitgeber = null;
  function leisteHolen() {
    if (leiste && document.documentElement.contains(leiste)) return leiste;
    leiste = document.createElement("div");
    leiste.setAttribute("role", "status");
    leiste.setAttribute("aria-live", "polite");
    leiste.style.cssText = [
      "position:fixed",
      "left:50%",
      "bottom:24px",
      "transform:translateX(-50%) translateY(12px)",
      "z-index:2147483647",
      "max-width:min(560px,calc(100vw - 32px))",
      "padding:12px 18px",
      "border-radius:16px",
      "font:500 13px/1.45 -apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif",
      "color:#1f2a24",
      "background:rgba(255,255,255,0.72)",
      "backdrop-filter:blur(18px) saturate(1.4)",
      "-webkit-backdrop-filter:blur(18px) saturate(1.4)",
      "box-shadow:0 12px 36px -12px rgba(20,40,30,0.35),0 0 0 1px rgba(255,255,255,0.5) inset",
      "opacity:0",
      "transition:opacity .25s ease,transform .25s ease",
      "pointer-events:none",
    ].join(";");
    (document.body || document.documentElement).appendChild(leiste);
    return leiste;
  }
  function zeigen(text, dauerMs) {
    var l = leisteHolen();
    l.textContent = text;
    requestAnimationFrame(function () {
      l.style.opacity = "1";
      l.style.transform = "translateX(-50%) translateY(0)";
    });
    clearTimeout(zeitgeber);
    if (dauerMs) zeitgeber = setTimeout(verbergen, dauerMs);
  }
  function verbergen() {
    if (!leiste) return;
    leiste.style.opacity = "0";
    leiste.style.transform = "translateX(-50%) translateY(12px)";
  }
  Object.defineProperty(window, "__atlasDesktopHinweis", {
    value: function (text) {
      zeigen(String(text), 7000);
    },
    enumerable: false,
  });

  var englisch = /^en/i.test(navigator.language || "");
  window.addEventListener("offline", function () {
    zeigen(
      englisch
        ? "No connection. Changes may not be saved until you're back online."
        : "Keine Verbindung. Änderungen werden erst gespeichert, wenn das Netz wieder da ist.",
      0,
    );
  });
  window.addEventListener("online", function () {
    zeigen(englisch ? "Back online." : "Wieder verbunden.", 2500);
  });
})();
