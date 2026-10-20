(function () {
      function inv(b, a) { return window.__TAURI_INTERNALS__.invoke(b, a || {}); }
      var t = document.getElementById("text");
      var m = document.getElementById("meldung");
      function senden(art) {
        inv("notiz_senden", { text: t.value, art: art }).catch(function (e) { m.textContent = String(e); m.className = "meldung fehler"; });
      }
      document.getElementById("aufgabe").onclick = function () { senden("aufgabe"); };
      document.getElementById("notiz").onclick = function () { senden("notiz"); };
      document.getElementById("foto").onclick = function () { inv("bildschirmfoto"); };
      document.getElementById("beleg").onclick = function () { inv("beleg_auswaehlen"); };
      t.addEventListener("keydown", function (e) {
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); senden("aufgabe"); }
        if (e.key === "Escape") { window.__TAURI_INTERNALS__.invoke("plugin:window|close", { label: "notiz" }).catch(function () { window.close(); }); }
      });
      setTimeout(function () { t.focus(); }, 50);
    })();
