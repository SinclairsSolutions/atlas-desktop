(function () {
      var m = document.getElementById("meldung");
      function los() {
        m.textContent = "";
        window.__TAURI_INTERNALS__.invoke("entsperren").then(function (ok) {
          if (!ok) { m.textContent = window.__en ? "Not confirmed — try again." : "Nicht bestätigt — bitte erneut versuchen."; m.className = "meldung fehler"; }
        });
      }
      document.getElementById("los").onclick = los;
      setTimeout(los, 300);
    })();
