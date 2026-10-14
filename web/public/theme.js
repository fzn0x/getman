// Applies the saved theme before the page paints, so it never flashes the
// wrong colors. Kept outside the app bundle because the page's
// Content-Security-Policy allows no inline scripts.
(function () {
  var theme = "system";
  try {
    theme = localStorage.getItem("getman.theme") || "system";
  } catch (e) {}
  if (theme === "light" || theme === "dark") {
    document.documentElement.dataset.theme = theme;
  }
})();
