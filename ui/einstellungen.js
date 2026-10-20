(function () {
  function tt(de, en) { return window.__en ? en : de; }
  function inv(b, a) { return window.__TAURI_INTERNALS__.invoke(b, a || {}); }
  var $ = function (id) { return document.getElementById(id); };
  var stand = null;
  function melden(t, fehler) { var m = $("meldung"); m.textContent = t || ""; m.className = "meldung" + (fehler ? " fehler" : ""); }

  function zeigen() {
    var e = stand.einstellungen;
    $("version").textContent = "Sinclair Atlas " + stand.version;
    $("dockZahl").checked = e.dockZahl;
    $("menueleiste").checked = e.menueleiste;
    $("passwortSymbol").checked = e.passwortSymbol;
    $("kuerzel").checked = e.schnellKuerzel;
    $("jeMandant").checked = e.exportJeMandant;
    $("touch").checked = e.touchIdSperre;
    $("touch").disabled = !stand.touchId;
    if (!stand.touchId) $("touchHinweis").textContent = tt("Auf diesem Gerät nicht verfügbar", "Not available on this device");
    $("minuten").value = String(e.sperreNachMinuten);
    $("minuten").disabled = !e.touchIdSperre;
    $("autostart").checked = stand.autostart;
    $("ordnerPfad").textContent = e.ueberwachterOrdner || tt("nicht eingerichtet", "not set");
    $("ordnerZeigen").disabled = !e.ueberwachterOrdner;
    $("ordnerWeg").disabled = !e.ueberwachterOrdner;
    $("exportPfad").textContent = e.exportOrdner || tt("Download-Ordner", "Downloads folder");
    $("exportWeg").disabled = !e.exportOrdner;
    $("wartend").textContent = stand.wartend
      ? tt(stand.wartend + " Datei(en) warten", stand.wartend + " file(s) waiting")
      : tt("Alles gesendet", "Everything sent");
    $("senden").disabled = !stand.wartend;
    document.querySelectorAll("#symbol button").forEach(function (b) {
      b.classList.toggle("an", b.getAttribute("data-wert") === e.symbol);
    });
  }
  function laden() { return inv("einstellungen_lesen").then(function (r) { stand = r; zeigen(); }); }
  function setzen(patch) {
    var neu = Object.assign({}, stand.einstellungen, patch);
    return inv("einstellungen_setzen", { neu: neu })
      .then(function () { melden(tt("Gespeichert", "Saved")); return laden(); })
      .catch(function (e) { melden(String(e), true); return laden(); });
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
  $("ordnerWaehlen").onclick = function () { inv("ordner_waehlen").then(function (p) { if (p) setzen({ ueberwachterOrdner: p }); }); };
  $("ordnerWeg").onclick = function () { setzen({ ueberwachterOrdner: null }); };
  $("ordnerZeigen").onclick = function () { inv("ordner_zeigen", { pfad: stand.einstellungen.ueberwachterOrdner }); };
  $("exportWaehlen").onclick = function () { inv("ordner_waehlen").then(function (p) { if (p) setzen({ exportOrdner: p }); }); };
  $("exportWeg").onclick = function () { setzen({ exportOrdner: null }); };
  $("senden").onclick = function () { inv("warteschlange_senden").then(function () { setTimeout(laden, 1500); }); };
  $("erlauben").onclick = function () {
    inv("mitteilungen_erlauben").then(function (r) {
      melden(r === "granted" ? tt("Mitteilungen erlaubt", "Notifications allowed") : tt("Bitte in den Systemeinstellungen → Mitteilungen erlauben", "Please allow in System Settings → Notifications"), r !== "granted");
    });
  };
  $("testen").onclick = function () { inv("mitteilung_testen").catch(function (e) { melden(String(e), true); }); };
  laden();
  setInterval(function () { if (stand) inv("einstellungen_lesen").then(function (r) { stand.wartend = r.wartend; zeigen(); }); }, 5000);
})();
