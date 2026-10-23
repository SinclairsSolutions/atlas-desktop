(function () {
  function tt(de, en) { return window.__en ? en : de; }
  function inv(b, a) { return window.__TAURI_INTERNALS__.invoke(b, a || {}); }
  var $ = function (id) { return document.getElementById(id); };
  var stand = null;
  function melden(t, fehler) { var m = $("meldung"); m.textContent = t || ""; m.className = "meldung" + (fehler ? " fehler" : ""); }
    // Eingerichteter Ordner gut sichtbar: grüner Hinweis „Aktiv“, Ordnername
  // fett, Pfad darunter. Ohne Ordner nur der leise Hinweistext.
  function ordnerZeigen(el, pfad, leer, aktivText) {
    while (el.firstChild) el.removeChild(el.firstChild);
    el.classList.toggle("ordner-aktiv", !!pfad);
    if (!pfad) { el.textContent = leer; return; }
    var teile = String(pfad).replace(/\/+$/, "").split("/");
    var name = teile.pop() || pfad;
    var oben = document.createElement("span");
    oben.className = "ordner-kopf";
    var pille = document.createElement("span");
    pille.className = "ordner-pille";
    pille.textContent = aktivText || tt("Aktiv", "Active");
    var symbol = document.createElement("span");
    symbol.className = "ordner-symbol";
    symbol.textContent = "📁";
    var n = document.createElement("b");
    n.className = "ordner-name";
    n.textContent = name;
    oben.appendChild(pille); oben.appendChild(symbol); oben.appendChild(n);
    var weg = document.createElement("span");
    weg.className = "ordner-weg";
    weg.textContent = teile.join("/").replace(/^\/Users\/[^/]+/, "~") || "/";
    el.appendChild(oben); el.appendChild(weg);
  }

  function sichtbar(el, an) { el.style.display = an ? "" : "none"; }

  // ---- Reiter (zuletzt offener wird gemerkt) ----
  var MERKER = "atlas.einstellungen.reiter";
  function reiterZeigen(name) {
    var gibt = document.querySelector('.seite[data-seite="' + name + '"]');
    if (!gibt) name = "allgemein";
    document.querySelectorAll("#reiter button[data-reiter]").forEach(function (b) {
      b.classList.toggle("an", b.getAttribute("data-reiter") === name);
    });
    document.querySelectorAll(".seite").forEach(function (s) {
      s.classList.toggle("an", s.getAttribute("data-seite") === name);
    });
    try { localStorage.setItem(MERKER, name); } catch (_) {}
  }
  document.querySelectorAll("#reiter button[data-reiter]").forEach(function (b) {
    b.onclick = function () { reiterZeigen(b.getAttribute("data-reiter")); };
  });
  var gemerkt = null;
  try { gemerkt = localStorage.getItem(MERKER); } catch (_) {}
  reiterZeigen(gemerkt || "allgemein");

  function plattformName(p) {
    return p === "macos" ? "macOS" : p === "windows" ? "Windows" : p === "linux" ? "Linux" : p;
  }

  function importListe() {
    var box = $("importListe");
    box.textContent = "";
    var imp = stand.import || { bekannt: false, workspaces: [], inaktiv: [] };
    var an = !!stand.einstellungen.importOrdner;
    function hinweis(text) {
      var d = document.createElement("div");
      d.className = "hinweis";
      d.textContent = text;
      box.appendChild(d);
    }
    if (!imp.bekannt) {
      hinweis(tt(
        "Noch keine Workspaces bekannt. Melde dich in Atlas an — dann legt die App die Ordner an.",
        "No workspaces known yet. Sign in to Atlas — the app then creates the folders."
      ));
      return;
    }
    if (!imp.workspaces.length) {
      hinweis(tt(
        "Atlas hat keine Workspaces gemeldet, in denen du Belege einreichen darfst. Alles im Import-Ordner geht in deinen persönlichen Eingang.",
        "Atlas reported no workspaces where you may submit receipts. Everything in the import folder goes to your personal inbox."
      ));
    }
    imp.workspaces.forEach(function (w) {
      var d = document.createElement("div");
      d.className = "ws";
      var n = document.createElement("b");
      n.textContent = w.name;
      d.appendChild(n);
      var o = document.createElement("div");
      o.className = "ordner";
      o.textContent = an
        ? (w.ordner ? tt("Ordner: ", "Folder: ") + w.ordner : tt("Ordner wird angelegt …", "Folder is being created …"))
        : tt("Ordner entsteht, sobald der Import-Ordner an ist", "Folder is created once the import folder is on");
      d.appendChild(o);
      if (stand.einstellungen.importJeUnternehmen) {
        (w.unternehmen || []).forEach(function (u) {
          var z = document.createElement("div");
          z.className = "ws-u";
          z.textContent = "↳ " + u.name + (an && u.ordner ? "  ·  " + u.ordner : "");
          d.appendChild(z);
        });
      }
      box.appendChild(d);
    });
    if (an && imp.inaktiv && imp.inaktiv.length) {
      var d = document.createElement("div");
      d.className = "hinweis warn";
      d.textContent = tt(
        "Nicht mehr aktiv (Dateien darin gehen in deinen persönlichen Eingang): ",
        "No longer active (files there go to your personal inbox): "
      ) + imp.inaktiv.join(", ");
      box.appendChild(d);
    }
  }

  function zeigen() {
    var e = stand.einstellungen;
    $("version").textContent = "Sinclair Atlas " + stand.version;
    $("ueberVersion").textContent = stand.version;
    $("ueberPlattform").textContent = plattformName(stand.plattform);
    if (stand.plattform === "windows") $("importZeigen").textContent = tt("Im Explorer zeigen", "Show in Explorer");
    if (stand.plattform !== "macos") $("kuerzelTitel").textContent = tt("Tastenkürzel Strg+Alt+A", "Shortcut Ctrl+Alt+A");
    $("dockZahl").checked = e.dockZahl;
    $("menueleiste").checked = e.menueleiste;
    $("passwortSymbol").checked = e.passwortSymbol;
    $("kuerzel").checked = e.schnellKuerzel;
    $("jeMandant").checked = e.exportJeMandant;
    $("touch").checked = e.touchIdSperre;
    $("touch").disabled = !stand.touchId;
    if (!stand.touchId) $("touchHinweis").textContent = tt("Auf diesem Gerät nicht verfügbar.", "Not available on this device.");
    $("minuten").value = String(e.sperreNachMinuten);
    $("minuten").disabled = !e.touchIdSperre;
    $("autostart").checked = stand.autostart;
    ordnerZeigen($("ordnerPfad"), e.ueberwachterOrdner, tt("nicht eingerichtet", "not set"), tt("Wird überwacht", "Watched"));
    $("ordnerZeigen").disabled = !e.ueberwachterOrdner;
    $("ordnerWeg").disabled = !e.ueberwachterOrdner;
    ordnerZeigen($("exportPfad"), e.exportOrdner, tt("Download-Ordner (Standard)", "Downloads folder (default)"), tt("Aktiv", "Active"));
    $("exportWeg").disabled = !e.exportOrdner;
    $("exportZeigen").disabled = !e.exportOrdner;
    $("wartend").textContent = stand.wartend
      ? tt(stand.wartend + " Datei(en) warten", stand.wartend + " file(s) waiting")
      : tt("Alles gesendet", "Everything sent");
    $("senden").disabled = !stand.wartend;
    document.querySelectorAll("#symbol button").forEach(function (b) {
      b.classList.toggle("an", b.getAttribute("data-wert") === e.symbol);
    });
    var ziel = e.eingangZiel || "fragen";
    document.querySelectorAll("#eingangZiel .option").forEach(function (b) {
      b.classList.toggle("an", b.getAttribute("data-wert") === ziel);
    });
    // Import-Ordner
    var an = !!e.importOrdner;
    $("importAn").checked = an;
    sichtbar($("importVorschlagZeile"), !an);
    sichtbar($("importPfadZeile"), an);
    $("importVorschlag").textContent = stand.importVorschlag || "~/Atlas Import";
    ordnerZeigen($("importPfad"), e.importOrdner, "", tt("Aktiv", "Active"));
    $("importJe").checked = e.importJeUnternehmen;
    importListe();
  }
  // ---- Passwörter in Safari (Erweiterung steckt in der Mac-App) ----
  var SAFARI_TEXT = {
    an: ["Eingeschaltet. Klick in Safari auf das Atlas-Symbol, um dich zu verbinden.", "Turned on. Click the Atlas icon in Safari to connect."],
    aus: ["Noch aus. Einmal in Safari einschalten — dann steht das Atlas-Symbol in der Symbolleiste.", "Still off. Turn it on once in Safari — then the Atlas icon appears in the toolbar."],
    unbekannt: ["Safari kennt die Erweiterung noch nicht. Liegt Sinclair Atlas unter „Programme“? Danach Safari einmal neu starten.", "Safari doesn’t know the extension yet. Is Sinclair Atlas in “Applications”? Then restart Safari once."],
    fehlt: ["In dieser Version nicht enthalten. Die aktuelle Mac-App gibt es auf sinclair-atlas.com.", "Not included in this version. Get the current Mac app from sinclair-atlas.com."]
  };
  function safariStand() {
    return inv("safari_erweiterung_status").then(function (z) {
      var t = SAFARI_TEXT[z] || SAFARI_TEXT.unbekannt;
      var el = $("safariStand");
      el.textContent = tt(t[0], t[1]);
      el.style.color = z === "an" ? "var(--akzent)" : "";
      $("safariOeffnen").disabled = z === "fehlt";
      $("safariOeffnen").textContent = z === "an"
        ? tt("Safari-Einstellungen öffnen", "Open Safari settings")
        : tt("In Safari einschalten", "Turn on in Safari");
    }).catch(function () {});
  }
  $("safariOeffnen").onclick = function () {
    inv("safari_erweiterung_oeffnen")
      .then(function () { melden(tt("Safari-Einstellungen geöffnet", "Safari settings opened")); })
      .catch(function (e) { melden(String(e), true); })
      .then(function () { setTimeout(safariStand, 1500); });
  };
  $("chromeOeffnen").onclick = function () {
    inv("chrome_erweiterung_oeffnen")
      .then(function (browser) {
        melden(browser
          ? tt("Chrome Web Store in " + browser + " geöffnet", "Chrome Web Store opened in " + browser)
          : tt("Chrome Web Store im Standardbrowser geöffnet", "Chrome Web Store opened in your default browser"));
      })
      .catch(function (e) { melden(String(e), true); });
  };
  // Nach dem Einschalten in Safari zurück in die App: Stand neu holen.
  window.addEventListener("focus", function () { if (stand && stand.plattform === "macos") safariStand(); });


  // ---- Ablage im Finder ----
  var ablageInfo = null;
  function ablageZeigenStand() {
    return inv("ablage_info").then(function (i) {
      ablageInfo = i;
      if (i.plattform === "windows") {
        var titel = tt("Ablage im Explorer", "Files in Explorer");
        ["reiterAblageText", "ablageTitel", "ablageSchalterTitel"].forEach(function (id) { $(id).textContent = titel; });
        $("ablageZeigen").textContent = tt("Im Explorer zeigen", "Show in Explorer");
        sichtbar($("ablageWindows"), true);
      }
      $("ablageAn").checked = !!i.gewuenscht;
      $("ablageAn").disabled = !i.eingebaut;
      var text;
      if (!i.eingebaut) text = tt("In dieser App-Version nicht enthalten.", "Not included in this app version.");
      else if (i.verbunden) text = tt("Verbunden", "Connected") + (i.geraetName ? " · " + i.geraetName : "");
      else if (i.gewuenscht) text = tt("Wartet auf Anmeldung in Atlas …", "Waiting for sign-in to Atlas …");
      else text = tt("Aus", "Off");
      $("ablageStand").textContent = text;
      sichtbar($("ablageZeigenZeile"), !!i.verbunden);
      var f = $("ablageFehler");
      f.textContent = i.fehler || "";
      sichtbar(f, !!i.fehler);
      var v = $("ablageVerschluesselung");
      if (i.verschluesselung === false) {
        v.textContent = i.plattform === "windows"
          ? tt("BitLocker ist auf diesem PC aus. Geöffnete Dateien liegen dann unverschlüsselt auf der Festplatte. Empfehlung: BitLocker einschalten.", "BitLocker is off on this PC. Opened files are then stored unencrypted on disk. Recommendation: turn on BitLocker.")
          : tt("FileVault ist auf diesem Mac aus. Geöffnete Dateien liegen dann unverschlüsselt auf der Festplatte. Empfehlung: Systemeinstellungen → Datenschutz & Sicherheit → FileVault einschalten.", "FileVault is off on this Mac. Opened files are then stored unencrypted on disk. Recommendation: turn on FileVault in System Settings → Privacy & Security.");
        sichtbar(v, true);
      } else sichtbar(v, false);
    }).catch(function () {});
  }
  $("ablageAn").onchange = function () {
    var an = this.checked;
    inv("ablage_schalten", { an: an })
      .then(function (m) { melden(m || ""); })
      .catch(function (e) { melden(String(e), true); })
      .then(ablageZeigenStand);
  };
  $("ablageZeigen").onclick = function () {
    inv("ablage_zeigen").catch(function (e) { melden(String(e), true); });
  };
  // Stand (verbunden, Fehler) beim Fokus und alle 4 Sekunden neu lesen.
  window.addEventListener("focus", ablageZeigenStand);
  setInterval(function () { if (document.visibilityState === "visible") ablageZeigenStand(); }, 4000);

  function laden() { return inv("einstellungen_lesen").then(function (r) { stand = r; zeigen(); }); }
  function setzen(patch) {
    var neu = Object.assign({}, stand.einstellungen, patch);
    return inv("einstellungen_setzen", { neu: neu })
      .then(function () { melden(tt("Gespeichert", "Saved")); return laden(); })
      .catch(function (e) { melden(String(e), true); return laden(); });
  }
  function importAnlegen() {
    return inv("import_ordner_anlegen")
      .then(function () { melden(tt("Import-Ordner angelegt", "Import folder created")); return laden(); })
      .catch(function (e) { melden(String(e), true); return laden(); });
  }
  function importWaehlen() {
    return inv("ordner_waehlen").then(function (p) { if (p) return setzen({ importOrdner: p }); laden(); });
  }

  $("dockZahl").onchange = function () { setzen({ dockZahl: this.checked }); };
  $("menueleiste").onchange = function () { setzen({ menueleiste: this.checked }); };
  $("passwortSymbol").onchange = function () { setzen({ passwortSymbol: this.checked }); };
  $("kuerzel").onchange = function () { setzen({ schnellKuerzel: this.checked }); };
  $("jeMandant").onchange = function () { setzen({ exportJeMandant: this.checked }); };
  $("touch").onchange = function () { setzen({ touchIdSperre: this.checked }); };
  $("minuten").onchange = function () { setzen({ sperreNachMinuten: Number(this.value) }); };
  $("autostart").onchange = function () {
    inv("autostart_setzen", { an: this.checked }).then(laden).catch(function (e) { melden(String(e), true); laden(); });
  };
  document.querySelectorAll("#symbol button").forEach(function (b) {
    b.onclick = function () { setzen({ symbol: b.getAttribute("data-wert") }); };
  });
  document.querySelectorAll("#eingangZiel .option").forEach(function (b) {
    b.onclick = function () { setzen({ eingangZiel: b.getAttribute("data-wert") }); };
  });
  $("importAn").onchange = function () {
    if (this.checked) importAnlegen();
    else setzen({ importOrdner: null });
  };
  $("importAnlegen").onclick = importAnlegen;
  $("importWaehlen").onclick = importWaehlen;
  $("importAendern").onclick = importWaehlen;
  $("importZeigen").onclick = function () { inv("ordner_zeigen", { pfad: stand.einstellungen.importOrdner }); };
  $("importJe").onchange = function () { setzen({ importJeUnternehmen: this.checked }); };
  $("ordnerWaehlen").onclick = function () { inv("ordner_waehlen").then(function (p) { if (p) setzen({ ueberwachterOrdner: p }); }); };
  $("ordnerWeg").onclick = function () { setzen({ ueberwachterOrdner: null }); };
  $("ordnerZeigen").onclick = function () { inv("ordner_zeigen", { pfad: stand.einstellungen.ueberwachterOrdner }); };
  $("exportWaehlen").onclick = function () { inv("ordner_waehlen").then(function (p) { if (p) setzen({ exportOrdner: p }); }); };
  $("exportWeg").onclick = function () { setzen({ exportOrdner: null }); };
  $("exportZeigen").onclick = function () { inv("ordner_zeigen", { pfad: stand.einstellungen.exportOrdner }); };
  $("senden").onclick = function () { inv("warteschlange_senden").then(function () { setTimeout(laden, 1500); }); };
  function erlaubtZeigen(r) {
    var b = $("erlauben");
    if (r === "granted") { b.textContent = tt("Erlaubt ✓", "Allowed ✓"); b.classList.add("erlaubt"); }
  }
  $("erlauben").onclick = function () {
    inv("mitteilungen_erlauben").then(function (r) {
      erlaubtZeigen(r);
      melden(r === "granted" ? tt("Mitteilungen erlaubt", "Notifications allowed") : tt("Bitte in den Systemeinstellungen → Mitteilungen erlauben", "Please allow in System Settings → Notifications"), r !== "granted");
    });
  };
  $("testen").onclick = function () {
    inv("mitteilung_testen")
      .then(function (r) { melden(r || tt("Probe-Mitteilung gesendet", "Test notification sent")); })
      .catch(function (e) { melden(String(e), true); });
  };
  $("updates").onclick = function () {
    inv("nach_updates_suchen").then(function () { melden(tt("Suche nach Updates …", "Checking for updates …")); });
  };
  if (!/Mac/i.test(navigator.platform || "")) {
    $("mitteilungHinweis").textContent = tt(
      "Es erscheint nichts? Öffne Einstellungen → System → Benachrichtigungen und erlaube Sinclair Atlas. Prüfe auch, ob „Nicht stören“ aktiv ist.",
      "Nothing shows up? Open Settings → System → Notifications and allow Sinclair Atlas. Also check whether Do Not Disturb is on."
    );
  }
  laden().then(function () {
    ablageZeigenStand();
    // Die Safari-Erweiterung gibt es nur auf dem Mac; Chrome/Edge überall.
    if (stand.plattform === "macos") safariStand();
    else sichtbar($("karteSafari"), false);
  });
  // Warteschlange und Workspace-Liste aktuell halten (Atlas meldet sie nach der Anmeldung).
  setInterval(function () {
    if (stand) inv("einstellungen_lesen").then(function (r) { stand.wartend = r.wartend; stand.import = r.import; zeigen(); });
  }, 5000);
})();
