(() => {
    window.filebeamAppearance?.dispose?.();
    const root = document.documentElement;
    const system = matchMedia('(prefers-color-scheme: dark)');
    const admin = root.classList.contains('fi');
    const storageKey = 'filebeam.guestAppearance';
    const cookieName = 'filebeam_appearance';
    let data = JSON.parse(document.getElementById('filebeam-appearance')?.textContent || 'null');
    const validMode = (value) => ['light', 'dark', 'system'].includes(value);
    const normalize = (value) => ({
        mode: validMode(value?.mode) ? value.mode : 'system',
        preset: data?.catalog?.some((entry) => entry.id === value?.preset)
            ? value.preset
            : 'instance',
    });
    function guest() {
        try {
            const cookie = document.cookie
                .split('; ')
                .find((entry) => entry.startsWith(`${cookieName}=`));
            if (cookie)
                return normalize(
                    JSON.parse(decodeURIComponent(cookie.slice(cookieName.length + 1))),
                );
        } catch {}
        try {
            const saved = localStorage.getItem(storageKey);
            if (saved) return normalize(JSON.parse(saved));
            return normalize({ mode: localStorage.getItem('filebeam.appearance') });
        } catch {
            return normalize(null);
        }
    }
    let preference = data?.account && !data.needs_adoption ? normalize(data.preference) : guest();
    let status = 'idle';
    let error = '';
    let revision = 0;
    let epoch = 0;
    let pending = null;
    let saving = false;
    let retryAdopt = false;
    let disposed = false;
    let requestController;
    const resolvedMode = () =>
        admin
            ? root.classList.contains('dark')
                ? 'dark'
                : 'light'
            : preference.mode === 'system'
              ? system.matches
                  ? 'dark'
                  : 'light'
              : preference.mode;
    const palette = () =>
        data?.catalog?.find(
            (entry) => entry.id === (data.custom_colors ? preference.preset : 'instance'),
        ) ?? null;
    function apply() {
        const mode = resolvedMode();
        const selected = palette();
        root.dataset.fbTheme = mode;
        root.dataset.fbAppearance = preference.mode;
        root.dataset.fbPreset = selected?.id ?? 'instance';
        if (!admin) root.classList.toggle('dark', mode === 'dark');
        const style = document.getElementById('filebeam-theme');
        if (selected && style) style.dataset.primary = selected.primary;
        const chrome = document.querySelector('meta[name="theme-color"]');
        const color = selected?.chrome?.[mode] ?? chrome?.getAttribute(`data-fb-${mode}`);
        if (chrome && color) chrome.setAttribute('content', color);
        for (const element of document.querySelectorAll('[data-fb-favicon]')) {
            const url = selected?.favicons?.[element.dataset.fbFavicon];
            if (url) element.setAttribute('href', url);
        }
        for (const element of document.querySelectorAll('[data-fb-brand]')) {
            const url = selected?.branding?.[element.dataset.fbBrand];
            if (url) element.setAttribute('src', url);
        }
        dispatchEvent(new CustomEvent('filebeam:appearance'));
    }
    function rememberGuest() {
        const serialized = JSON.stringify(preference);
        try {
            document.cookie = `${cookieName}=${encodeURIComponent(serialized)}; Path=/; Max-Age=31536000; SameSite=Lax${location.protocol === 'https:' ? '; Secure' : ''}`;
            localStorage.setItem(storageKey, serialized);
            localStorage.setItem('filebeam.appearance', preference.mode);
        } catch {}
    }
    function enqueue(adopt = false) {
        retryAdopt = adopt;
        pending = {
            ...preference,
            adopt,
            account: data.account,
            revision,
            epoch,
            url: data.save_url,
            csrf: data.csrf,
        };
        status = 'saving';
        error = '';
        apply();
        void drain();
    }
    async function drain() {
        if (saving || !pending || disposed) return;
        const job = pending;
        pending = null;
        saving = true;
        requestController = new AbortController();
        try {
            const response = await fetch(job.url, {
                method: 'PATCH',
                credentials: 'same-origin',
                signal: requestController.signal,
                headers: {
                    Accept: 'application/json',
                    'Content-Type': 'application/json',
                    'X-CSRF-TOKEN': job.csrf ?? '',
                },
                body: JSON.stringify({
                    account: job.account,
                    mode: job.mode,
                    preset: job.preset,
                    adopt: job.adopt,
                }),
            });
            if (!response.ok) throw new Error('Appearance could not be saved. Try again.');
            const saved = await response.json();
            if (disposed || epoch !== job.epoch || data?.account !== saved.account) return;
            data.needs_adoption = false;
            if (revision === job.revision) {
                preference = normalize(saved.preference);
                status = 'saved';
                apply();
            }
        } catch (failure) {
            if (!disposed && epoch === job.epoch && revision === job.revision) {
                status = 'error';
                error =
                    failure instanceof Error && failure.name !== 'AbortError'
                        ? 'Appearance could not be saved. Try again.'
                        : '';
                apply();
            }
        } finally {
            saving = false;
            if (!disposed && pending) void drain();
        }
    }
    function choose(next) {
        preference = normalize({ ...preference, ...next });
        revision++;
        if (data?.account) enqueue();
        else {
            status = 'idle';
            error = '';
            rememberGuest();
            apply();
        }
    }
    function hydrate(next) {
        if (!next || admin) return;
        const changedAccount = data?.account !== next.account;
        if (changedAccount) {
            epoch++;
            revision++;
            pending = null;
            requestController?.abort();
            status = 'idle';
            error = '';
        }
        data = next;
        if (changedAccount || !['saving', 'error'].includes(status)) {
            preference =
                next.account && !next.needs_adoption ? normalize(next.preference) : guest();
        }
        apply();
        if (next.account && next.needs_adoption && !pending && status !== 'saving') enqueue(true);
    }
    window.filebeamAppearance = {
        get: () => preference.mode,
        set: (mode) => {
            if (validMode(mode)) choose({ mode });
        },
        setPreset: (preset) => {
            if (data?.custom_colors || preset === 'instance') choose({ preset });
        },
        reset: () => choose({ mode: 'system', preset: 'instance' }),
        retry: () => {
            if (data?.account) enqueue(retryAdopt);
        },
        hydrate,
        snapshot: () => ({
            preference: { ...preference },
            mode: resolvedMode(),
            account: data?.account ?? null,
            catalog: data?.catalog ?? [],
            palette: palette(),
            customColors: data?.custom_colors ?? false,
            status,
            error,
        }),
        refresh: apply,
        dispose: () => {
            disposed = true;
            requestController?.abort();
            system.removeEventListener('change', apply);
            removeEventListener('storage', storageChanged);
            observer?.disconnect();
            document.removeEventListener('DOMContentLoaded', mounted);
        },
    };
    function storageChanged(event) {
        if (!data?.account && [storageKey, 'filebeam.appearance', null].includes(event.key)) {
            preference = guest();
            apply();
        }
    }
    function mounted() {
        apply();
        if (data?.account && data.needs_adoption && status !== 'saving' && !pending) enqueue(true);
    }
    system.addEventListener('change', apply);
    addEventListener('storage', storageChanged);
    const observer = admin ? new MutationObserver(apply) : null;
    observer?.observe(root, { attributes: true, attributeFilter: ['class'] });
    document.addEventListener('DOMContentLoaded', mounted, { once: true });
    apply();
})();
