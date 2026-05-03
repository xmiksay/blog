// miksanik.net — site JS
// 1. Mobile sidebar toggle
// 2. Transclude (::page) collapse on header click
// 3. Page-mount scanline animation

(function () {
    // ---- mobile sidebar ----
    var menuToggle = document.getElementById('menu-toggle');
    var sidebar = document.getElementById('sidebar');
    if (menuToggle && sidebar) {
        menuToggle.addEventListener('change', function () {
            sidebar.classList.toggle('open', this.checked);
        });
    }

    // ---- transclude collapse ----
    document.addEventListener('click', function (e) {
        var head = e.target.closest('.transclude .tx-head');
        if (!head) return;
        var box = head.parentElement;
        var collapsed = box.classList.toggle('collapsed');
        var caret = head.querySelector('.caret');
        var toggle = head.querySelector('.toggle');
        if (caret) caret.textContent = collapsed ? '▸' : '▾';
        if (toggle) toggle.textContent = collapsed ? 'expand' : 'collapse';
    });

    // ---- scanline ----
    var scanline = document.querySelector('.scanline');
    if (scanline) {
        scanline.classList.add('active');
        scanline.addEventListener('animationend', function () {
            scanline.classList.remove('active');
        });
    }
})();
