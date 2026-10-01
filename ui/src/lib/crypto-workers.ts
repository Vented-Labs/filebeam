import transferWorkerUrl from '../../../backend/resources/js/workers/filebeam-crypto.worker.ts?worker&url';
import accountWorkerUrl from '../../../backend/resources/js/workers/account-crypto.worker.ts?worker&url';

const developmentEntries = new Map<string, string>();

function createWorker(source: string): Worker {
    const url = new URL(source, window.location.href);
    if (import.meta.env.DEV && url.origin !== window.location.origin) {
        // Worker entries must be same-origin; module imports may use Vite's CORS-enabled origin.
        let entry = developmentEntries.get(url.href);
        if (!entry) {
            entry = URL.createObjectURL(
                new Blob([`import ${JSON.stringify(url.href)};`], {
                    type: 'text/javascript',
                }),
            );
            developmentEntries.set(url.href, entry);
        }
        return new Worker(entry, { type: 'module' });
    }
    return new Worker(url, { type: 'module' });
}

if (import.meta.hot) {
    import.meta.hot.dispose(() => {
        for (const entry of developmentEntries.values()) URL.revokeObjectURL(entry);
        developmentEntries.clear();
    });
}

export function createTransferCryptoWorker(): Worker {
    return createWorker(transferWorkerUrl);
}

export function createAccountCryptoWorker(): Worker {
    return createWorker(accountWorkerUrl);
}
