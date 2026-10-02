(() => {
    window.filebeamAppearance?.dispose?.();
    const root = document.documentElement;
    const system = matchMedia('(prefers-color-scheme: dark)');
    const admin = root.classList.contains('fi');
    const key = 'filebeam.appearance';
    let preference = 'system';
    const valid = (value) => ['light', 'dark', 'system'].includes(value);
    try {
        const saved = localStorage.getItem(key);
        if (valid(saved)) preference = saved;
    } catch {}
    function apply() {
        const mode = admin
            ? root.classList.contains('dark')
                ? 'dark'
                : 'light'
            : preference === 'system'
              ? system.matches
                  ? 'dark'
                  : 'light'
              : preference;
        root.dataset.fbTheme = mode;
        root.dataset.fbAppearance = preference;
        if (!admin) root.classList.toggle('dark', mode === 'dark');
        for (const element of document.querySelectorAll('[data-fb-dark-src]')) {
            const value = element.getAttribute(`data-fb-${mode}-src`);
            if (value) element.setAttribute(element.tagName === 'LINK' ? 'href' : 'src', value);
        }
        const chrome = document.querySelector('meta[name="theme-color"]');
        const color = chrome?.getAttribute(`data-fb-${mode}`);
        if (color) chrome.setAttribute('content', color);
        dispatchEvent(new CustomEvent('filebeam:appearance'));
    }
    window.filebeamAppearance = {
        get: () => preference,
        set: (value) => {
            if (!valid(value)) return;
            preference = value;
            try {
                localStorage.setItem(key, value);
            } catch {}
            apply();
        },
        refresh: apply,
        dispose: () => {
            system.removeEventListener('change', apply);
            removeEventListener('storage', storageChanged);
            observer?.disconnect();
            document.removeEventListener('DOMContentLoaded', apply);
        },
    };
    system.addEventListener('change', apply);
    function storageChanged(event) {
        if (event.key === key || event.key === null) {
            preference = valid(event.newValue) ? event.newValue : 'system';
            apply();
        }
    }
    addEventListener('storage', storageChanged);
    const observer = admin ? new MutationObserver(apply) : null;
    observer?.observe(root, {
        attributes: true,
        attributeFilter: ['class'],
    });
    document.addEventListener('DOMContentLoaded', apply, { once: true });
    apply();
})();
