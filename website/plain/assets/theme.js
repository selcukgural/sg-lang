// Runs before the page is drawn, so a page chosen dark is never shown light first: the choice is kept in
// this browser only. Without it the page follows the system's setting.
(function () {
  var root = document.documentElement;
  root.setAttribute('data-js', '');
  try {
    var chosen = localStorage.getItem('sg-theme');
    if (chosen === 'light' || chosen === 'dark') root.setAttribute('data-theme', chosen);
  } catch (e) {}
})();
