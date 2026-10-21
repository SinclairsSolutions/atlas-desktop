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
      // Fähigkeiten, nach denen Atlas fragen kann.
      kann: Object.freeze({ eingang: true, notiz: true, touchId: "__PLATTFORM__" === "macos", fensterZiehen: true, importOrdner: true, ablage: true }),
      /** Gerade gespeicherte Datei im Finder zeigen (nur nach Namen). */
      imFinderZeigen: function (name) {
        return invoke("im_finder_zeigen", { name: String(name || "") });
      },
      /** Touch ID (macOS) — Promise<boolean>. */
      touchId: function (grund) {
        return invoke("touch_id_pruefen", { grund: String(grund || "") }).catch(function () {
          return false;
        });
      },
      /** Gerätegeheimnis für den Passwort-Tresor (Touch ID + Schlüsselbund) — Promise<string base64>. */
      geraetGeheimnis: "__PLATTFORM__" === "macos"
        ? function (id, anlegen) {
            return invoke("geraet_geheimnis", { id: String(id), anlegen: !!anlegen });
          }
        : null,
      /**
       * Import-Ordner: Workspaces (mit Unternehmen), in deren Eingang die Person
       * schreiben darf — nach der Anmeldung aufrufen.
       * liste: [{ id, name, unternehmen: [{ id, name }] }]
       */
      importOrdnerSetzen: function (liste) {
        var workspaces = (Array.isArray(liste) ? liste : []).map(function (w) {
          return {
            id: String((w && w.id) || ""),
            name: String((w && w.name) || ""),
            unternehmen: (w && Array.isArray(w.unternehmen) ? w.unternehmen : []).map(function (u) {
              return { id: String((u && u.id) || ""), name: String((u && u.name) || "") };
            }),
          };
        });
        return invoke("import_ordner_setzen", { workspaces: workspaces });
      },
      /**
       * Ablage im Finder/Explorer (siehe src-tauri/src/ablage): Atlas fragt den
       * Stand ab, koppelt bei Bedarf und meldet Abmeldungen.
       * ablageStatus → { gewuenscht, verbunden, geraetId, geraetName }
       */
      ablageStatus: function () {
        return invoke("ablage_status");
      },
      ablageVerbunden: function (z) {
        return invoke("ablage_verbunden", {
          token: String((z && z.token) || ""),
          geraetId: String((z && z.geraetId) || ""),
          name: String((z && z.name) || ""),
        });
      },
      ablageFehler: function (meldung) {
        return invoke("ablage_fehler", { meldung: String(meldung || "") });
      },
      ablageAbgemeldet: function () {
        return invoke("ablage_abgemeldet");
      },
      /** Bestätigung der Eingangs-Uploads (Dateien aus Dock, Ordner, Menüleiste). */
      dateienErledigt: function (erledigt, fehler, ziel) {
        return invoke("dateien_erledigt", {
          erledigt: erledigt || [],
          fehler: fehler || [],
          ziel: ziel ? String(ziel) : null,
        }).catch(function () {});
      },
    }),
    enumerable: false,
  });

  // macOS: Atlas reicht bis unter die Fensterknöpfe (Titelleiste „Overlay“).
  // Atlas lässt dafür oben Platz (Klasse) und markiert ziehbare Flächen.
  try {
    document.documentElement.classList.add("atlas-desktop");
    if ("__PLATTFORM__" === "macos") document.documentElement.classList.add("atlas-desktop-mac");
  } catch (_) {}

  // Ziehfläche über dem oberen Rand der Seitenleiste (dort sitzen die
  // Fensterknöpfe). Die Kopfleiste markiert Atlas selbst (data-tauri-drag-region).
  if ("__PLATTFORM__" === "macos") {
    var streifen = null;
    var streifenAnpassen = function () {
      var leiste = document.querySelector('[data-sidebar="sidebar"]');
      if (!document.body) return;
      if (!streifen) {
        streifen = document.createElement("div");
        streifen.setAttribute("data-tauri-drag-region", "");
        streifen.style.cssText = "position:fixed;top:0;left:0;height:28px;z-index:2147483000;";
        document.body.appendChild(streifen);
      }
      var b = leiste ? Math.round(leiste.getBoundingClientRect().width) : 0;
      streifen.style.width = Math.max(b, 78) + "px";
    };
    var beobachten = function () {
      streifenAnpassen();
      try {
        new ResizeObserver(streifenAnpassen).observe(document.body);
        new MutationObserver(streifenAnpassen).observe(document.body, { childList: true, subtree: false });
      } catch (_) {}
    };
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", beobachten);
    else beobachten();
    window.addEventListener("resize", streifenAnpassen);
    setInterval(streifenAnpassen, 1500);
  }

  // Eingang: Die App übergibt Dateien (Dock, überwachter Ordner, Import-Ordner,
  // Menüleiste, Bildschirmfoto). Atlas hört auf „atlas-desktop-dateien“, lädt mit
  // der Anmeldung der Person hoch und bestätigt über dateienErledigt.
  // Je Datei: { token, name, mime, inhalt, quelle, ziel } mit ziel
  //   { art: "persoenlich" } | { art: "workspace", workspaceId, companyId|null }
  //   | { art: "aktuell" } | { art: "fragen" }
  Object.defineProperty(window, "__atlasDesktopDateien", {
    value: function (dateien) {
      window.dispatchEvent(new CustomEvent("atlas-desktop-dateien", { detail: { dateien: dateien || [] } }));
    },
  });
  // Schnellerfassung: Text als Aufgabe oder Notiz.
  Object.defineProperty(window, "__atlasDesktopNotiz", {
    value: function (notiz) {
      window.dispatchEvent(new CustomEvent("atlas-desktop-notiz", { detail: notiz || {} }));
    },
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
  // In der App entscheidet macOS über Mitteilungen (Systemeinstellungen →
  // Mitteilungen → Sinclair Atlas); die Seite darf immer senden. Ein früher
  // gespeichertes „denied“ (ältere App-Versionen) würde sonst alle
  // Hintergrund-Mitteilungen still verschlucken.
  var erlaubnis = "__PLATTFORM__" === "macos" ? "granted" : gespeicherteErlaubnis();
  if (erlaubnis === "granted") {
    try {
      localStorage.setItem(SCHLUESSEL, "granted");
    } catch (_) {}
  }

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
    if (adresse && window.__atlasDesktopIstDownload && window.__atlasDesktopIstDownload(String(adresse))) {
      dateiSpeichern(String(adresse), "");
      return null;
    }
    var a = adresse == null || String(adresse) === "" ? "about:blank" : adresse;
    return urspruenglichOpen.call(window, a, ziel, merkmale);
  };

  // Navigation API (WebKit ab Safari 26, WebView2): erwischt auch
  // `location.href = …` und Formular-Sendungen, bevor die Seite wechselt.
  if (window.navigation && typeof window.navigation.addEventListener === "function") {
    window.navigation.addEventListener("navigate", function (e) {
      var ziel = e.destination && e.destination.url;
      // location.href = …?dl=1 → selbst speichern statt die Seite zu verlassen.
      if (ziel && e.cancelable && istDownloadAdresse(ziel)) {
        e.preventDefault();
        dateiSpeichern(ziel, "");
        return;
      }
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
  // Dateiname aus Content-Disposition (filename*=UTF-8''… oder filename="…").
  function nameAusAntwort(r, href) {
    var cd = (r && r.headers && r.headers.get("content-disposition")) || "";
    var m = /filename\*=(?:UTF-8'')?([^;]+)/i.exec(cd);
    if (m) {
      try {
        return decodeURIComponent(m[1].trim().replace(/^"|"$/g, ""));
      } catch (_) {}
    }
    m = /filename="?([^";]+)"?/i.exec(cd);
    if (m) return m[1].trim();
    try {
      var teil = new URL(href, location.href).pathname.split("/").pop();
      return teil ? decodeURIComponent(teil) : "";
    } catch (_) {
      return "";
    }
  }
  function dateiSpeichern(href, name) {
    var antwort = null;
    return fetch(href, { credentials: "same-origin" })
      .then(function (r) {
        if (!r.ok) throw new Error("HTTP " + r.status);
        antwort = r;
        return r.arrayBuffer();
      })
      .then(function (puffer) {
        var k = window.__ATLAS_KONTEXT__ || {};
        return invoke("datei_speichern", {
          name: name || nameAusAntwort(antwort, href) || "Download",
          inhalt: alsBase64(puffer),
          mandant: k.workspaceName ? String(k.workspaceName) : null,
        });
      })
      .catch(function (e) {
        console.warn("[Atlas Desktop] Download fehlgeschlagen", e);
      });
  }
  // Atlas liefert Dateien über eigene Links aus; „dl=1“ heißt „als Download“.
  // WebKit zeigt PDFs dann trotzdem an statt sie zu speichern — deshalb holt
  // die Brücke solche Dateien selbst.
  function istDownloadAdresse(href) {
    try {
      var u = new URL(href, location.href);
      return (istAtlas(u.href) || u.origin === location.origin) && /(^|&)dl=1(&|$)/.test(u.search.slice(1));
    } catch (_) {
      return false;
    }
  }
  function istSelbstDownload(a) {
    if (!a || !a.getAttribute) return false;
    var href = a.href || "";
    if (a.hasAttribute("download") && /^(blob|data):/i.test(href)) return true;
    if (a.hasAttribute("download") && (istAtlas(href) || href.indexOf(location.origin) === 0)) return true;
    return istDownloadAdresse(href);
  }
  window.__atlasDesktopIstDownload = istDownloadAdresse;
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
        var a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
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
