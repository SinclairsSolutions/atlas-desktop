// Deutsch/Englisch nach Systemsprache für die mitgelieferten Fenster.
(function () {
  var en = !/^de/i.test(navigator.language || "de");
  window.__en = en;
  document.documentElement.lang = en ? "en" : "de";
  document.querySelectorAll("[data-de]").forEach(function (el) {
    var t = el.getAttribute(en ? "data-en" : "data-de");
    if (t != null) el.textContent = t;
  });
})();
