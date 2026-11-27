// What the pages of sg-lang.org do with a script: the search, the line that says what a name in a program is,
// the button for light or dark, and the filter of the index. Every page reads and works without it.
// Nothing is written into the page as HTML: text goes in with textContent.
(function () {
  'use strict';

  var root = (document.querySelector('meta[name="sg-root"]') || {}).content || './';
  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  // ---- light or dark -------------------------------------------------------------------------------
  var html = document.documentElement;
  var themeButton = document.getElementById('theme');
  function shown() {
    var set = html.getAttribute('data-theme');
    if (set) return set;
    return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  function label() { themeButton.textContent = shown() === 'dark' ? 'Light' : 'Dark'; }
  if (themeButton) {
    themeButton.hidden = false;
    label();
    themeButton.addEventListener('click', function () {
      var next = shown() === 'dark' ? 'light' : 'dark';
      html.setAttribute('data-theme', next);
      try { localStorage.setItem('sg-theme', next); } catch (e) {}
      label();
    });
  }

  // ---- what a name in a program is, where it is pointed at -------------------------------------------
  var tip = el('div');
  tip.id = 'tip';
  tip.hidden = true;
  tip.setAttribute('role', 'tooltip');
  document.body.appendChild(tip);
  function showTip(a) {
    tip.textContent = '';
    tip.appendChild(el('b', null, a.textContent));
    tip.appendChild(document.createTextNode(a.getAttribute('data-tip')));
    tip.hidden = false;
    var r = a.getBoundingClientRect();
    var x = Math.max(8, Math.min(r.left, window.innerWidth - tip.offsetWidth - 8));
    var y = r.top - tip.offsetHeight - 8;
    if (y < 8) y = r.bottom + 8;
    tip.style.left = x + 'px';
    tip.style.top = y + 'px';
  }
  function hideTip() { tip.hidden = true; }
  Array.prototype.forEach.call(document.querySelectorAll('pre a[title], a.code-link[title]'), function (a) {
    a.setAttribute('data-tip', a.title);
    a.removeAttribute('title');
    a.addEventListener('mouseenter', function () { showTip(a); });
    a.addEventListener('focus', function () { showTip(a); });
    a.addEventListener('mouseleave', hideTip);
    a.addEventListener('blur', hideTip);
  });
  window.addEventListener('scroll', hideTip, { passive: true });

  // ---- the search ----------------------------------------------------------------------------------
  var q = document.getElementById('q');
  var found = document.getElementById('found');
  var hits = [];
  var at = 0;
  var all = self.SG_SEARCH || [];
  function rank(entry, s) {
    var n = entry[0].toLowerCase();
    if (n === s) return 0;
    if (n.indexOf(s) === 0) return 1 + (entry[1].indexOf('in ') === 0 ? 2 : 0);
    if (n.indexOf(s) > 0) return 4 + (entry[1].indexOf('in ') === 0 ? 2 : 0);
    if (entry[2].toLowerCase().indexOf(s) >= 0) return 7;
    return -1;
  }
  function search() {
    var s = q.value.trim().toLowerCase();
    found.textContent = '';
    hits = [];
    if (!s) { found.hidden = true; return; }
    hits = all.map(function (e) { return { e: e, r: rank(e, s) }; })
      .filter(function (x) { return x.r >= 0; })
      .sort(function (a, b) { return a.r - b.r || a.e[0].length - b.e[0].length || a.e[0].localeCompare(b.e[0]); })
      .slice(0, 12)
      .map(function (x) { return x.e; });
    at = 0;
    if (!hits.length) found.appendChild(el('li', 'none', 'Nothing by that name.'));
    hits.forEach(function (e, i) {
      var li = el('li', i === 0 ? 'on' : null);
      var a = el('a');
      a.href = root + e[3];
      a.appendChild(el('b', null, e[0]));
      a.appendChild(el('i', null, e[1]));
      if (e[2]) a.appendChild(el('span', null, e[2]));
      li.appendChild(a);
      found.appendChild(li);
    });
    found.hidden = false;
  }
  if (q && found) {
    q.addEventListener('input', search);
    q.addEventListener('keydown', function (ev) {
      if (ev.key === 'Escape') { found.hidden = true; q.blur(); return; }
      if (!hits.length) return;
      if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
        ev.preventDefault();
        at = (at + (ev.key === 'ArrowDown' ? 1 : hits.length - 1)) % hits.length;
        Array.prototype.forEach.call(found.children, function (li, i) { li.className = i === at ? 'on' : ''; });
        found.children[at].scrollIntoView({ block: 'nearest' });
      } else if (ev.key === 'Enter') {
        ev.preventDefault();
        location.href = root + hits[at][3];
      }
    });
    document.addEventListener('click', function (ev) {
      if (!ev.target.closest('.find')) found.hidden = true;
    });
    document.addEventListener('keydown', function (ev) {
      var t = ev.target;
      var typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
      if (ev.key === '/' && !typing) { ev.preventDefault(); q.focus(); }
    });
  }

  // ---- the filter of the index ---------------------------------------------------------------------
  var filter = document.getElementById('f');
  var names = document.getElementById('names');
  if (filter && names) {
    var rows = Array.prototype.slice.call(names.querySelectorAll('tr'));
    var count = document.getElementById('fcount');
    var apply = function () {
      var s = filter.value.trim().toLowerCase();
      var shownRows = 0;
      rows.forEach(function (tr) {
        var ok = !s || tr.textContent.toLowerCase().indexOf(s) >= 0;
        tr.hidden = !ok;
        if (ok) shownRows++;
      });
      Array.prototype.forEach.call(names.querySelectorAll('h2'), function (h) {
        var box = h.nextElementSibling;
        var any = box.querySelector('tr:not([hidden])');
        h.hidden = !any;
        box.hidden = !any;
      });
      count.textContent = s ? shownRows + ' of ' + rows.length : '';
    };
    filter.addEventListener('input', apply);
    // A search sent from a page without its script comes here as ?q=.
    var asked = new URLSearchParams(location.search).get('q');
    if (asked) { filter.value = asked; apply(); }
  }
})();
