<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { Icon } from '@filebeam/ui';
import InstallationField from '@/components/InstallationField.vue';
import {
    bootstrap as bootstrapAction,
    cache as cacheAction,
    complete as completeAction,
    configuration as configurationAction,
    database as databaseAction,
    probe as probeAction,
    storage as storageAction,
} from '@/actions/App/Http/Controllers/InstallationController';
import { computed, nextTick, reactive, ref, watch } from 'vue';

type Database = {
    driver: string;
    transport: 'tcp' | 'socket';
    socket: string;
    host: string;
    port: string | number;
    database: string;
    username: string;
    password: string;
    sslmode: string;
};

type Store = {
    name: string;
    driver: string;
    root: string;
    bucket: string;
    key: string;
    secret: string;
    region: string;
    endpoint: string;
    use_path_style_endpoint: boolean;
};

type Cache = {
    driver: 'file' | 'redis';
    transport: 'tcp' | 'tls' | 'unix';
    host: string;
    port: string | number;
    username: string;
    password: string;
    database: number;
    prefix: string;
};

type Defaults = {
    database: Database;
    cache: Cache;
    instance: {
        name: string;
        url: string;
        username_domain: string;
        visibility: string;
        auto_updates_enabled: boolean;
    };
    storage: Store[];
    placement_mode: 'replicate' | 'distribute';
};

type Chunks = {
    maximum: number;
    minimum: number;
    recommended: number;
    post_max_size: string;
    post_max_bytes: number | null;
    upload_max_filesize: string;
    overhead: number;
};

type Managed = {
    container: boolean;
    variant: string | null;
    database: boolean;
    cache: boolean;
    instance: boolean;
    auto_updates: boolean;
};

type ApiFailure = { message?: string; errors?: Record<string, string[]> };

const props = defineProps<{
    bootstrapRequired: boolean;
    challenge: string | null;
    unavailableReason: string | null;
}>();

const bootstrapRequired = ref(props.bootstrapRequired);
const installationToken = ref('');
const step = ref(1);
const furthestStep = ref(1);
const completedSteps = ref<number[]>([]);
const loading = ref(false);
const notice = ref('');
const requestError = ref('');
const errors = ref<Record<string, string[]>>({});
const databaseDrivers = ref<string[]>([]);
const redisAvailable = ref(false);
const prerequisites = ref<{ label: string; passed: boolean }[]>([]);
const chunks = ref<Chunks | null>(null);
const databaseTested = ref(false);
const cacheTested = ref(false);
const testedStores = reactive<Record<number, boolean>>({});
const testedChunkSize = ref<number | null>(null);
const rejectedChunkSize = ref<number | null>(null);
const probeStatus = ref<'idle' | 'testing' | 'passed' | 'rejected' | 'inconclusive'>('idle');
const probeMessage = ref('');
const probeAttempts = ref(0);
const probeBytes = ref(0);
const probeInFlight = ref(false);
const probeGeneration = ref(0);
const sqliteDatabaseDefault = ref('');
const managed = ref<Managed>({
    container: false,
    variant: null,
    database: false,
    cache: false,
    instance: false,
    auto_updates: false,
});

const form = reactive({
    database: {
        driver: 'mysql',
        transport: 'tcp',
        socket: '',
        host: '127.0.0.1',
        port: '3306',
        database: '',
        username: '',
        password: '',
        sslmode: 'prefer',
    } as Database,
    cache: {
        driver: 'file',
        transport: 'tcp',
        host: '127.0.0.1',
        port: 6379,
        username: '',
        password: '',
        database: 0,
        prefix: 'filebeam:',
    } as Cache,
    instance: {
        name: 'Filebeam',
        url: '',
        username_domain: '',
        visibility: 'private',
        auto_updates_enabled: false,
    },
    storage: [newStore()] as Store[],
    placement_mode: 'replicate' as 'replicate' | 'distribute',
    admin: {
        name: '',
        username: '',
        email: '',
        password: '',
        password_confirmation: '',
        email_ownership_confirmed: false,
    },
    chunk_max_size: 1_048_576,
    chunk_warning_acknowledged: false,
});

const steps = ['Access', 'Database & cache', 'Instance', 'Storage', 'Admin', 'Review'];
const fieldLabels: Record<string, string> = {
    token: 'Installation token',
    database: 'Database connection',
    cache: 'Cache connection',
    instance: 'Instance configuration',
    storage: 'Storage configuration',
    admin: 'Administrator',
    placement_mode: 'Storage placement',
    chunk_max_size: 'Maximum encrypted body bytes',
    chunk_warning_acknowledged: 'Chunk size warning acknowledgement',
    'instance.name': 'Instance name',
    'instance.url': 'Public URL',
    'instance.username_domain': 'Username domain',
    'instance.visibility': 'Visibility',
    'instance.auto_updates_enabled': 'Automatic updates',
    driver: 'Driver',
    transport: 'Connection type',
    socket: 'Socket path',
    host: 'Host',
    port: 'Port',
    username: 'Username',
    password: 'Password',
    sslmode: 'SSL mode',
    prefix: 'Prefix',
    name: 'Name',
    root: 'Relative storage path',
    bucket: 'Bucket',
    key: 'Access key',
    secret: 'Secret key',
    region: 'Region',
    endpoint: 'HTTPS endpoint',
    use_path_style_endpoint: 'Path-style endpoint',
    email: 'Email',
    password_confirmation: 'Confirm password',
    email_ownership_confirmed: 'Email ownership confirmation',
    'database.database': 'Database name or path',
    'cache.database': 'Cache database index',
};
const errorEntries = computed(() =>
    Object.entries(errors.value)
        .map(([path, messages]) => ({
            path,
            messages,
            label: errorLabel(path),
            step: errorStep(path),
        }))
        .sort((a, b) => a.step - b.step),
);
const ready = computed(() => chunks.value !== null);
const chunkMiB = computed(() => (form.chunk_max_size / 1_048_576).toFixed(2));
const finitePostLimit = computed(() => chunks.value?.post_max_bytes ?? null);
const chunkWarning = computed(() => {
    return (
        (finitePostLimit.value !== null && form.chunk_max_size > finitePostLimit.value) ||
        (rejectedChunkSize.value !== null && form.chunk_max_size >= rejectedChunkSize.value)
    );
});
const chunkStale = computed(() => testedChunkSize.value !== form.chunk_max_size);
const prerequisitesPassed = computed(() => prerequisites.value.every(({ passed }) => passed));
const canProbe = computed(() => {
    return (
        ready.value &&
        !probeInFlight.value &&
        Number.isSafeInteger(form.chunk_max_size) &&
        form.chunk_max_size >= (chunks.value?.minimum ?? 17) &&
        form.chunk_max_size <= (chunks.value?.maximum ?? 25_000_000) &&
        probeAttempts.value < 5 &&
        probeBytes.value + form.chunk_max_size <= 75_000_000
    );
});

function newStore(index = 1): Store {
    return {
        name: 'Local storage',
        driver: 'local',
        root: index === 1 ? 'primary' : `primary-${index}`,
        bucket: '',
        key: '',
        secret: '',
        region: '',
        endpoint: '',
        use_path_style_endpoint: false,
    };
}

function fieldErrors(path: string): string[] {
    return errors.value[path] ?? [];
}

function clearError(path: string): void {
    delete errors.value[path];
    requestError.value = '';
    notice.value = '';
    completedSteps.value = completedSteps.value.filter((value) => value !== errorStep(path));
    const store = /^storage\.(\d+)/.exec(path);
    if (store) {
        delete testedStores[Number(store[1])];
    }
    if (path === 'admin.password') {
        delete errors.value['admin.password_confirmation'];
    }
    const scope = store ? `storage.${store[1]}` : path.split('.')[0];
    if (scope === 'database' || scope === 'cache' || store) delete errors.value[scope];
    if (path.endsWith('.driver') || path.endsWith('.transport')) {
        void nextTick(() => {
            for (const errorPath of Object.keys(errors.value)) {
                if (
                    errorPath.startsWith(`${scope}.`) &&
                    !document.getElementById(`install-${errorPath.replaceAll('.', '-')}`)
                ) {
                    delete errors.value[errorPath];
                }
            }
        });
    }
}

function errorStep(path: string): number {
    if (path === 'token' || path === 'challenge') return 1;
    if (/^(database|cache)(\.|$)/.test(path)) return 2;
    if (/^instance(\.|$)/.test(path)) return 3;
    if (/^storage(\.|$)/.test(path) || path.startsWith('chunk_') || path === 'placement_mode')
        return 4;
    if (/^admin(\.|$)/.test(path)) return 5;
    return 6;
}

function errorLabel(path: string): string {
    if (fieldLabels[path]) return fieldLabels[path];
    const parts = path.split('.');
    const field = parts.at(-1) ?? path;
    if (parts[0] === 'storage' && /^\d+$/.test(parts[1] ?? '')) {
        return `Store ${Number(parts[1]) + 1}${parts.length > 2 ? `: ${fieldLabels[field] ?? field}` : ''}`;
    }
    const section =
        parts[0] === 'admin' ? 'Administrator' : parts[0] === 'cache' ? 'Cache' : 'Database';
    return parts.length > 1
        ? `${section}: ${fieldLabels[field] ?? field}`
        : path.replaceAll('_', ' ');
}

function stepHasErrors(value: number): boolean {
    return errorEntries.value.some((entry) => entry.step === value);
}

async function focusError(path: string): Promise<void> {
    step.value = errorStep(path);
    furthestStep.value = Math.max(furthestStep.value, step.value);
    await nextTick();
    const id = `install-${path.replaceAll('.', '-')}`;
    const control = document.getElementById(id);
    // Managed or conditionally hidden settings still need a reachable error explanation.
    const target =
        control && !control.matches(':disabled')
            ? control
            : (document.getElementById(`${id}-error`) ??
              document.getElementById('install-error-summary'));
    if (target) {
        if (target !== control) target.tabIndex = -1;
        target.focus();
    }
}

function canNavigateStep(target: number): boolean {
    return (
        !loading.value &&
        (target === 1 ||
            (ready.value &&
                prerequisitesPassed.value &&
                target <= Math.max(furthestStep.value, step.value + 1)))
    );
}

async function navigateStep(target: number, validate = false): Promise<void> {
    if (!canNavigateStep(target)) return;
    if (validate && target > step.value) {
        const controls = document.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
            '#install-step-panel input[name], #install-step-panel select[name]',
        );
        for (const control of controls) {
            if (!control.willValidate) continue;
            if (!control.checkValidity()) {
                const message =
                    control.name === 'instance.username_domain'
                        ? 'Enter a hostname such as example.com, without https:// or a path.'
                        : control.name === 'admin.username' && control.validity.patternMismatch
                          ? 'Use 3 to 24 lowercase letters, numbers, or underscores.'
                          : control.validationMessage;
                errors.value[control.name] = [message];
            }
        }
        if (step.value === 5 && form.admin.password !== form.admin.password_confirmation) {
            errors.value['admin.password_confirmation'] = ['The passwords do not match.'];
        }
        const first = errorEntries.value.find((entry) => entry.step === step.value);
        if (first) {
            notice.value = '';
            await focusError(first.path);
            return;
        }
        if (!completedSteps.value.includes(step.value)) completedSteps.value.push(step.value);
    }
    step.value = target;
    furthestStep.value = Math.max(furthestStep.value, target);
    notice.value = '';
    await nextTick();
    document.getElementById('install-step-title')?.focus();
}

function applyDefaults(defaults: Defaults): void {
    Object.assign(form.database, defaults.database);
    Object.assign(form.cache, defaults.cache);
    sqliteDatabaseDefault.value = defaults.database.database;
    Object.assign(form.instance, {
        ...defaults.instance,
        name: defaults.instance.name || 'Filebeam',
    });
    form.storage.splice(
        0,
        form.storage.length,
        ...(defaults.storage.length ? defaults.storage : [newStore()]),
    );
    form.placement_mode = defaults.placement_mode;
}

function applyDatabaseDriverDefaults(): void {
    databaseTested.value = false;
    form.database.transport = 'tcp';
    form.database.socket = '';

    if (form.database.driver === 'sqlite') {
        form.database.port = '';
        form.database.database = sqliteDatabaseDefault.value;
        return;
    }

    form.database.port = form.database.driver === 'pgsql' ? '5432' : '3306';
    form.database.database = 'filebeam';
}

function headers(withToken = true): HeadersInit {
    return {
        Accept: 'application/json',
        'Content-Type': 'application/json',
        ...(withToken && installationToken.value
            ? { 'X-Installation-Token': installationToken.value }
            : {}),
    };
}

async function json<T>(url: string, body: unknown, withToken = true): Promise<T> {
    const response = await fetch(url, {
        method: 'POST',
        headers: headers(withToken),
        body: JSON.stringify(body),
    });

    const payload = (await response.json().catch(() => ({}))) as T & ApiFailure;

    if (!response.ok) {
        throw Object.assign(new Error(payload.message || `Request failed (${response.status}).`), {
            errors:
                payload.errors ??
                (response.status === 401
                    ? { token: [payload.message || 'Enter a valid installation token.'] }
                    : {}),
        });
    }

    return payload;
}

function showFailure(error: unknown, errorPrefix?: string): void {
    const failure = error as Error & { errors?: Record<string, string[]> };
    const incoming = Object.fromEntries(
        Object.entries(failure.errors ?? {}).map(([path, messages]) => [
            errorPrefix && (path === 'storage' || path.startsWith('storage.'))
                ? `${errorPrefix}${path.slice('storage'.length)}`
                : path,
            messages,
        ]),
    );
    Object.assign(errors.value, incoming);
    completedSteps.value = completedSteps.value.filter((value) => !stepHasErrors(value));
    notice.value = '';
    requestError.value = Object.keys(incoming).length
        ? ''
        : failure.message || 'The server could not process this request.';
    const first = errorEntries.value.find((entry) => entry.path in incoming);
    if (first) {
        void focusError(first.path);
    } else {
        void nextTick(() => document.getElementById('install-error-summary')?.focus());
    }
}

async function bootstrap(): Promise<void> {
    loading.value = true;
    requestError.value = '';

    try {
        const response = await fetch(bootstrapAction.url(), {
            method: 'POST',
            headers: {
                ...headers(false),
                'X-Installation-Challenge': props.challenge ?? '',
            },
            body: JSON.stringify({}),
        });
        const result = (await response.json().catch(() => ({}))) as {
            message?: string;
        } & ApiFailure;

        if (!response.ok) {
            throw Object.assign(
                new Error(result.message || `Request failed (${response.status}).`),
                {
                    errors: result.errors ?? {},
                },
            );
        }
        bootstrapRequired.value = false;
        notice.value =
            result.message ||
            'Bootstrap is ready. Add the installation token to your server environment.';
    } catch (error) {
        showFailure(error);
    } finally {
        loading.value = false;
    }
}

async function loadConfiguration(): Promise<void> {
    loading.value = true;
    requestError.value = '';

    try {
        const result = await json<{
            databaseDrivers: string[];
            redisAvailable: boolean;
            defaults: Defaults;
            prerequisites: { label: string; passed: boolean }[];
            chunks: Chunks;
            managed: Managed;
        }>(configurationAction.url(), {});
        databaseDrivers.value = result.databaseDrivers;
        redisAvailable.value = result.redisAvailable;
        prerequisites.value = result.prerequisites;
        chunks.value = result.chunks;
        managed.value = result.managed;
        applyDefaults(result.defaults);
        form.chunk_max_size = result.chunks.recommended;
        step.value = 1;
        furthestStep.value = 1;
        errors.value = {};
        completedSteps.value = [];
        notice.value = 'Configuration loaded. Verify each section before completing installation.';
        void automaticProbe(result.chunks.recommended, probeGeneration.value);
    } catch (error) {
        showFailure(error);
    } finally {
        loading.value = false;
    }
}

async function testDatabase(): Promise<void> {
    databaseTested.value = false;
    await runCheck(databaseAction.url(), { database: form.database }, () => {
        databaseTested.value = true;
    });
}

async function testCache(): Promise<void> {
    cacheTested.value = false;
    await runCheck(cacheAction.url(), { cache: form.cache }, () => {
        cacheTested.value = true;
    });
}

async function testStore(index: number): Promise<void> {
    testedStores[index] = false;
    await runCheck(
        storageAction.url(),
        { storage: form.storage[index] },
        () => {
            testedStores[index] = true;
        },
        `storage.${index}`,
    );
}

async function runCheck(
    url: string,
    body: unknown,
    success: () => void,
    errorPrefix?: string,
): Promise<void> {
    loading.value = true;
    requestError.value = '';
    notice.value = '';
    const scope = errorPrefix ?? Object.keys(body as Record<string, unknown>)[0];
    Object.keys(errors.value)
        .filter((path) => path === scope || path.startsWith(`${scope}.`))
        .forEach((path) => delete errors.value[path]);

    try {
        const result = await json<{ message: string }>(url, body);
        success();
        notice.value = result.message || 'Connection verified.';
    } catch (error) {
        showFailure(error, errorPrefix);
    } finally {
        loading.value = false;
    }
}

async function probe(
    size: number,
    generation: number,
): Promise<'passed' | 'rejected' | 'inconclusive'> {
    if (!Number.isSafeInteger(size) || size < 17 || size > 25_000_000) {
        if (generation === probeGeneration.value) {
            probeStatus.value = 'inconclusive';
            probeMessage.value = 'Choose a whole-byte size within the supported range.';
        }
        return 'inconclusive';
    }

    if (probeInFlight.value || probeAttempts.value >= 5 || probeBytes.value + size > 75_000_000) {
        if (generation === probeGeneration.value) {
            probeStatus.value = 'inconclusive';
            probeMessage.value = 'Probe limit reached. Continue with a conservative chunk size.';
        }
        return 'inconclusive';
    }

    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 30_000);
    probeInFlight.value = true;
    probeAttempts.value += 1;
    probeBytes.value += size;
    if (generation === probeGeneration.value) {
        testedChunkSize.value = null;
        probeStatus.value = 'testing';
        probeMessage.value = `Testing ${formatBytes(size)} raw encrypted body allowance...`;
    }

    try {
        const response = await fetch(probeAction.url(), {
            method: 'PUT',
            headers: {
                Accept: 'application/json',
                'Content-Type': 'application/octet-stream',
                'X-Installation-Token': installationToken.value,
            },
            body: new Uint8Array(size),
            signal: controller.signal,
        });

        if (response.status === 413) {
            rejectedChunkSize.value = Math.min(rejectedChunkSize.value ?? size, size);
            if (generation === probeGeneration.value) {
                probeStatus.value = 'rejected';
                probeMessage.value = `${formatBytes(size)} was rejected with HTTP 413.`;
            }
            return 'rejected';
        }

        if (!response.ok) {
            if (generation === probeGeneration.value) {
                probeStatus.value = 'inconclusive';
                probeMessage.value = `Probe was inconclusive (HTTP ${response.status}).`;
            }
            return 'inconclusive';
        }

        const result = (await response.json()) as { bytes?: number };
        if (result.bytes !== size) {
            if (generation === probeGeneration.value) {
                probeStatus.value = 'inconclusive';
                probeMessage.value = 'Probe response did not confirm the complete requested body.';
            }
            return 'inconclusive';
        }
        if (generation === probeGeneration.value) {
            testedChunkSize.value = size;
            probeStatus.value = 'passed';
            probeMessage.value = `${formatBytes(size)} accepted by the readiness endpoint.`;
        }
        return 'passed';
    } catch (error) {
        if (generation === probeGeneration.value) {
            probeStatus.value = 'inconclusive';
            probeMessage.value =
                error instanceof DOMException && error.name === 'AbortError'
                    ? 'Probe timed out after 30 seconds.'
                    : 'Probe could not reach the server.';
        }
        return 'inconclusive';
    } finally {
        window.clearTimeout(timeout);
        probeInFlight.value = false;
    }
}

async function automaticProbe(size: number, generation: number): Promise<void> {
    let candidate = size;

    while (candidate >= (chunks.value?.minimum ?? 17) && generation === probeGeneration.value) {
        const result = await probe(candidate, generation);
        if (result === 'passed' && generation === probeGeneration.value) {
            form.chunk_max_size = candidate;
            return;
        }
        if (result !== 'rejected') {
            return;
        }
        candidate = Math.max(chunks.value?.minimum ?? 17, Math.floor(candidate / 2));
        if (candidate === rejectedChunkSize.value) {
            return;
        }
    }
}

async function manualProbe(): Promise<void> {
    if (!canProbe.value) {
        probeStatus.value = 'inconclusive';
        probeMessage.value = 'Choose a size within the allowed range and remaining probe budget.';
        return;
    }
    await probe(form.chunk_max_size, probeGeneration.value);
}

async function complete(): Promise<void> {
    loading.value = true;
    requestError.value = '';
    notice.value = '';
    errors.value = {};

    try {
        const result = await json<{ message: string; redirect: string }>(
            completeAction.url(),
            form,
        );
        const redirect = new URL(result.redirect, window.location.origin);

        const configuredOrigin = new URL(form.instance.url).origin;
        if (redirect.origin !== configuredOrigin || redirect.pathname !== '/admin/login') {
            throw new Error('The installation endpoint returned an unsafe redirect.');
        }

        installationToken.value = '';
        form.database.password = '';
        form.cache.password = '';
        form.storage.forEach((store) => {
            store.key = '';
            store.secret = '';
        });
        form.admin.password = '';
        form.admin.password_confirmation = '';
        window.location.assign(redirect.href);
    } catch (error) {
        showFailure(error);
    } finally {
        loading.value = false;
    }
}

function addStore(): void {
    if (form.storage.length < 8) {
        form.storage.push(newStore(form.storage.length + 1));
        completedSteps.value = completedSteps.value.filter((value) => value !== 4);
    }
}

function removeStore(index: number): void {
    if (form.storage.length > 1) {
        form.storage.splice(index, 1);
        Object.keys(testedStores).forEach((key) => delete testedStores[Number(key)]);
        // Server error paths use array indexes, so move surviving errors with their store.
        errors.value = Object.fromEntries(
            Object.entries(errors.value).flatMap(([path, messages]) => {
                const match = /^storage\.(\d+)(.*)$/.exec(path);
                if (!match) return [[path, messages]];
                const storeIndex = Number(match[1]);
                if (storeIndex === index) return [];
                return [
                    [
                        `storage.${storeIndex > index ? storeIndex - 1 : storeIndex}${match[2]}`,
                        messages,
                    ],
                ];
            }),
        );
        completedSteps.value = completedSteps.value.filter((value) => value !== 4);
    }
}

function formatBytes(bytes: number): string {
    return `${(bytes / 1_048_576).toFixed(bytes >= 1_048_576 ? 2 : 4)} MiB (${bytes.toLocaleString()} bytes)`;
}

watch(
    () => form.chunk_max_size,
    () => {
        form.chunk_warning_acknowledged = false;
        probeGeneration.value += 1;
    },
    { flush: 'sync' },
);

watch(() => form.database.driver, applyDatabaseDriverDefaults);
watch(form.database, () => {
    databaseTested.value = false;
});
watch(
    form.cache,
    () => {
        cacheTested.value = false;
    },
    { deep: true },
);
</script>

<template>
    <div class="fb-shell installer">
        <Head title="Install Filebeam" />

        <header class="fb-header">
            <div class="fb-brand">
                <img
                    src="/brand/filebeam-logo-header.svg"
                    alt="Filebeam"
                    class="fb-brand__lockup"
                />
            </div>
        </header>

        <main class="installer-content">
            <h1 class="installer-heading">Set up Filebeam</h1>

            <div
                v-if="unavailableReason"
                class="installer-message installer-message--warning"
                role="alert"
            >
                <p class="font-semibold">Installation is unavailable</p>
                <p class="mt-1 text-[var(--fb-warning)]">{{ unavailableReason }}</p>
            </div>

            <template v-else>
                <nav class="installer-progress" aria-label="Installation progress">
                    <div class="installer-progress__mobile">
                        <label for="install-step-picker"
                            >Step {{ step }} of {{ steps.length }}</label
                        >
                        <select
                            id="install-step-picker"
                            class="fb-input"
                            :value="step"
                            :disabled="loading"
                            @change="
                                navigateStep(
                                    Number(($event.target as HTMLSelectElement).value),
                                    true,
                                );
                                ($event.target as HTMLSelectElement).value = String(step);
                            "
                        >
                            <option
                                v-for="(name, index) in steps"
                                :key="name"
                                :value="index + 1"
                                :disabled="!canNavigateStep(index + 1)"
                            >
                                {{ name }}{{ stepHasErrors(index + 1) ? ' (needs attention)' : '' }}
                            </option>
                        </select>
                    </div>
                    <ol class="installer-steps">
                        <li
                            v-for="(name, index) in steps"
                            :key="name"
                            class="installer-step"
                            :class="{
                                'is-current': step === index + 1,
                                'is-complete': completedSteps.includes(index + 1),
                                'has-error': stepHasErrors(index + 1),
                            }"
                        >
                            <button
                                type="button"
                                class="installer-step__button"
                                :aria-current="step === index + 1 ? 'step' : undefined"
                                :disabled="!canNavigateStep(index + 1)"
                                @click="navigateStep(index + 1, true)"
                            >
                                <span class="installer-step__marker" aria-hidden="true">
                                    <span v-if="stepHasErrors(index + 1)">!</span>
                                    <Icon
                                        v-else-if="completedSteps.includes(index + 1)"
                                        name="check"
                                        :size="16"
                                    />
                                    <span v-else>{{ index + 1 }}</span>
                                </span>
                                <span class="installer-step__label">{{ name }}</span>
                                <span v-if="stepHasErrors(index + 1)" class="sr-only"
                                    >Needs attention</span
                                >
                                <span v-else-if="completedSteps.includes(index + 1)" class="sr-only"
                                    >Completed</span
                                >
                            </button>
                        </li>
                    </ol>
                </nav>

                <div
                    v-if="requestError || errorEntries.length"
                    id="install-error-summary"
                    class="installer-message installer-message--error"
                    role="alert"
                    tabindex="-1"
                >
                    <p class="font-semibold">
                        {{
                            errorEntries.length
                                ? 'Check these settings to continue'
                                : 'Unable to continue'
                        }}
                    </p>
                    <p v-if="requestError" class="mt-2">{{ requestError }}</p>
                    <ul v-if="errorEntries.length" class="mt-3 space-y-2">
                        <li v-for="entry in errorEntries" :key="entry.path">
                            <a
                                :href="`#install-${entry.path.replaceAll('.', '-')}`"
                                @click.prevent="focusError(entry.path)"
                            >
                                <span class="font-semibold">{{ entry.label }}:</span>
                                {{ entry.messages.join(' ') }}
                            </a>
                        </li>
                    </ul>
                </div>
                <div v-if="notice" class="installer-message" role="status">
                    {{ notice }}
                </div>

                <section
                    v-if="step === 1"
                    id="install-step-panel"
                    class="panel"
                    aria-labelledby="install-step-title"
                >
                    <h2 id="install-step-title" class="title" tabindex="-1">
                        Authorize installation
                    </h2>
                    <div v-if="bootstrapRequired" class="mt-6 space-y-4">
                        <p class="copy">
                            Prepare this server for installation, then enter your installation
                            token.
                        </p>
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            :disabled="loading || !challenge"
                            @click="bootstrap"
                        >
                            {{ loading ? 'Preparing...' : 'Prepare installation' }}
                        </button>
                        <p v-if="!challenge" class="text-sm text-[var(--fb-danger)]">
                            The installation challenge is missing. Reload this page to obtain a
                            fresh challenge.
                        </p>
                    </div>
                    <div v-else class="mt-6 space-y-5">
                        <div
                            class="rounded-lg border border-[var(--fb-border)] bg-[var(--fb-selected-surface)] p-4 text-sm leading-6 text-[var(--fb-text)]"
                        >
                            Open the server
                            <code class="font-mono text-[var(--fb-accent-text)]">{{
                                managed.container ? '/data/config/.env' : '.env'
                            }}</code
                            >, copy
                            <code class="font-mono text-[var(--fb-accent-text)]"
                                >FILEBEAM_INSTALL_TOKEN</code
                            >, then paste it below.
                        </div>
                        <InstallationField
                            v-model="installationToken"
                            path="token"
                            label="Installation token"
                            :error="fieldErrors('token').join(' ')"
                            type="password"
                            autocomplete="off"
                            spellcheck="false"
                            @update:model-value="clearError('token')"
                        />
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            :disabled="loading || !installationToken"
                            @click="loadConfiguration"
                        >
                            {{ loading ? 'Checking...' : 'Check server readiness' }}
                        </button>
                    </div>
                    <div
                        v-if="prerequisites.length"
                        class="mt-8 border-t border-[var(--fb-border)] pt-6"
                    >
                        <h3 class="font-semibold text-[var(--fb-text)]">Prerequisites</h3>
                        <ul class="mt-3 grid gap-2 sm:grid-cols-2">
                            <li
                                v-for="prerequisite in prerequisites"
                                :key="prerequisite.label"
                                class="flex items-center gap-2 rounded-lg bg-[var(--fb-surface-raised)] px-3 py-2 text-sm"
                                :class="
                                    prerequisite.passed
                                        ? 'text-[var(--fb-text)]'
                                        : 'text-[var(--fb-danger)]'
                                "
                            >
                                <span aria-hidden="true">{{
                                    prerequisite.passed ? 'OK' : '!'
                                }}</span
                                >{{ prerequisite.label }}
                            </li>
                        </ul>
                        <button
                            type="button"
                            class="fb-button fb-button--primary mt-5"
                            :disabled="!prerequisitesPassed"
                            @click="navigateStep(2, true)"
                        >
                            Continue to database
                        </button>
                        <p v-if="!prerequisitesPassed" class="mt-3 text-sm text-[var(--fb-danger)]">
                            Resolve every failed prerequisite before continuing.
                        </p>
                    </div>
                </section>

                <section
                    v-else-if="step === 2"
                    id="install-step-panel"
                    class="panel"
                    aria-labelledby="install-step-title"
                >
                    <h2 id="install-step-title" class="title" tabindex="-1">Database and cache</h2>
                    <p v-if="managed.database" class="mt-3 text-sm text-[var(--fb-text-muted)]">
                        Database settings are managed by the container runtime. Correct the
                        container environment configuration and reload to retry.
                    </p>
                    <p
                        v-if="fieldErrors('database').length"
                        id="install-database-error"
                        class="section-error"
                        tabindex="-1"
                    >
                        {{ fieldErrors('database').join(' ') }}
                    </p>
                    <fieldset
                        :disabled="managed.database"
                        class="mt-6 grid gap-4 sm:grid-cols-2 disabled:opacity-70"
                    >
                        <InstallationField
                            v-model="form.database.driver"
                            path="database.driver"
                            label="Driver"
                            :error="fieldErrors('database.driver').join(' ')"
                            type="select"
                            required
                            @update:model-value="clearError('database.driver')"
                        >
                            <option v-for="driver in databaseDrivers" :key="driver" :value="driver">
                                {{ driver }}
                            </option>
                        </InstallationField>
                        <InstallationField
                            v-if="form.database.driver !== 'sqlite'"
                            v-model="form.database.transport"
                            path="database.transport"
                            label="Connection"
                            :error="fieldErrors('database.transport').join(' ')"
                            type="select"
                            required
                            @update:model-value="clearError('database.transport')"
                        >
                            <option value="tcp">TCP (hostname or IP)</option>
                            <option value="socket">Unix socket</option>
                        </InstallationField>
                        <InstallationField
                            v-if="
                                form.database.driver !== 'sqlite' &&
                                form.database.transport === 'socket'
                            "
                            v-model="form.database.socket"
                            path="database.socket"
                            :label="
                                form.database.driver === 'pgsql'
                                    ? 'Socket directory'
                                    : 'Socket file'
                            "
                            :error="fieldErrors('database.socket').join(' ')"
                            :description="
                                form.database.driver === 'pgsql'
                                    ? 'PostgreSQL uses this directory and the port below to locate its Unix socket.'
                                    : 'Use the full path to the MySQL or MariaDB Unix socket; host and port are not used.'
                            "
                            class="sm:col-span-2"
                            autocomplete="off"
                            required
                            pattern="/.*"
                            :placeholder="
                                form.database.driver === 'pgsql'
                                    ? '/var/run/postgresql'
                                    : '/run/mysqld/mysqld.sock'
                            "
                            @update:model-value="clearError('database.socket')"
                        />
                        <InstallationField
                            v-if="
                                form.database.driver !== 'sqlite' &&
                                form.database.transport === 'tcp'
                            "
                            v-model="form.database.host"
                            path="database.host"
                            label="Host"
                            :error="fieldErrors('database.host').join(' ')"
                            autocomplete="off"
                            required
                            @update:model-value="clearError('database.host')"
                        />
                        <InstallationField
                            v-if="
                                form.database.driver !== 'sqlite' &&
                                (form.database.transport === 'tcp' ||
                                    form.database.driver === 'pgsql')
                            "
                            v-model="form.database.port"
                            path="database.port"
                            label="Port"
                            :error="fieldErrors('database.port').join(' ')"
                            type="number"
                            min="1"
                            max="65535"
                            inputmode="numeric"
                            required
                            @update:model-value="clearError('database.port')"
                        />
                        <InstallationField
                            v-model="form.database.database"
                            path="database.database"
                            label="Database"
                            :error="fieldErrors('database.database').join(' ')"
                            autocomplete="off"
                            required
                            @update:model-value="clearError('database.database')"
                        />
                        <InstallationField
                            v-if="form.database.driver !== 'sqlite'"
                            v-model="form.database.username"
                            path="database.username"
                            label="Username"
                            :error="fieldErrors('database.username').join(' ')"
                            autocomplete="username"
                            required
                            @update:model-value="clearError('database.username')"
                        />
                        <InstallationField
                            v-if="form.database.driver !== 'sqlite'"
                            v-model="form.database.password"
                            path="database.password"
                            label="Password"
                            :error="fieldErrors('database.password').join(' ')"
                            type="password"
                            autocomplete="new-password"
                            @update:model-value="clearError('database.password')"
                        />
                        <InstallationField
                            v-if="
                                form.database.driver === 'pgsql' &&
                                form.database.transport === 'tcp'
                            "
                            v-model="form.database.sslmode"
                            path="database.sslmode"
                            label="SSL mode"
                            :error="fieldErrors('database.sslmode').join(' ')"
                            autocomplete="off"
                            @update:model-value="clearError('database.sslmode')"
                        />
                    </fieldset>
                    <div class="mt-4 flex items-center gap-3">
                        <button
                            type="button"
                            class="fb-button fb-button--secondary"
                            :disabled="loading"
                            @click="testDatabase"
                        >
                            {{ loading ? 'Testing...' : 'Test database' }}
                        </button>
                        <span v-if="databaseTested" class="success">Connection verified</span>
                    </div>
                    <div class="mt-8 border-t border-[var(--fb-border)] pt-6">
                        <h3 class="font-semibold text-[var(--fb-text)]">Cache configuration</h3>
                        <p class="mt-2 text-sm leading-6 text-[var(--fb-text-muted)]">
                            Redis provides cache, locks, rate limits, and monitoring. The session
                            cookie is unchanged.
                        </p>
                        <p v-if="managed.cache" class="mt-3 text-sm text-[var(--fb-text-muted)]">
                            Cache settings are managed by the container runtime. Correct the
                            container environment configuration and reload to retry.
                        </p>
                        <p
                            v-if="fieldErrors('cache').length"
                            id="install-cache-error"
                            class="section-error"
                            tabindex="-1"
                        >
                            {{ fieldErrors('cache').join(' ') }}
                        </p>
                        <fieldset
                            :disabled="managed.cache"
                            class="mt-4 grid gap-4 sm:grid-cols-2 disabled:opacity-70"
                        >
                            <InstallationField
                                v-model="form.cache.driver"
                                path="cache.driver"
                                label="Driver"
                                :error="fieldErrors('cache.driver').join(' ')"
                                type="select"
                                required
                                @update:model-value="clearError('cache.driver')"
                            >
                                <option value="file">File</option>
                                <option value="redis" :disabled="!redisAvailable">Redis</option>
                            </InstallationField>
                            <p v-if="!redisAvailable" class="hint sm:col-span-2">
                                Redis requires the PHP Redis extension.
                            </p>
                            <template v-if="form.cache.driver === 'redis'">
                                <InstallationField
                                    v-model="form.cache.transport"
                                    path="cache.transport"
                                    label="Connection"
                                    :error="fieldErrors('cache.transport').join(' ')"
                                    type="select"
                                    required
                                    @update:model-value="clearError('cache.transport')"
                                >
                                    <option value="tcp">TCP</option>
                                    <option value="tls">TLS</option>
                                    <option value="unix">Unix socket</option>
                                </InstallationField>
                                <InstallationField
                                    :class="{
                                        'sm:col-span-2': form.cache.transport === 'unix',
                                    }"
                                    v-model="form.cache.host"
                                    path="cache.host"
                                    :label="
                                        form.cache.transport === 'unix' ? 'Socket path' : 'Host'
                                    "
                                    :error="fieldErrors('cache.host').join(' ')"
                                    autocomplete="off"
                                    :placeholder="
                                        form.cache.transport === 'unix'
                                            ? '/run/redis/redis-server.sock'
                                            : '127.0.0.1'
                                    "
                                    required
                                    @update:model-value="clearError('cache.host')"
                                />
                                <InstallationField
                                    v-if="form.cache.transport !== 'unix'"
                                    v-model="form.cache.port"
                                    path="cache.port"
                                    label="Port"
                                    :error="fieldErrors('cache.port').join(' ')"
                                    type="number"
                                    min="1"
                                    max="65535"
                                    inputmode="numeric"
                                    required
                                    @update:model-value="clearError('cache.port')"
                                />
                                <InstallationField
                                    v-model="form.cache.username"
                                    path="cache.username"
                                    label="Username"
                                    :error="fieldErrors('cache.username').join(' ')"
                                    autocomplete="username"
                                    @update:model-value="clearError('cache.username')"
                                />
                                <InstallationField
                                    v-model="form.cache.password"
                                    path="cache.password"
                                    label="Password"
                                    :error="fieldErrors('cache.password').join(' ')"
                                    type="password"
                                    autocomplete="new-password"
                                    @update:model-value="clearError('cache.password')"
                                />
                                <InstallationField
                                    v-model="form.cache.database"
                                    path="cache.database"
                                    label="Database index"
                                    :error="fieldErrors('cache.database').join(' ')"
                                    type="number"
                                    min="0"
                                    inputmode="numeric"
                                    required
                                    @update:model-value="clearError('cache.database')"
                                />
                                <InstallationField
                                    v-model="form.cache.prefix"
                                    path="cache.prefix"
                                    label="Prefix"
                                    :error="fieldErrors('cache.prefix').join(' ')"
                                    description="Use a unique prefix for this deployment."
                                    autocomplete="off"
                                    @update:model-value="clearError('cache.prefix')"
                                />
                            </template>
                        </fieldset>
                        <div class="mt-4 flex items-center gap-3">
                            <button
                                type="button"
                                class="fb-button fb-button--secondary"
                                :disabled="loading"
                                @click="testCache"
                            >
                                {{ loading ? 'Testing...' : 'Test cache' }}
                            </button>
                            <span v-if="cacheTested" class="success">Cache verified</span>
                        </div>
                    </div>
                    <div class="actions">
                        <button
                            type="button"
                            class="fb-button fb-button--secondary"
                            @click="navigateStep(1)"
                        >
                            Back
                        </button>
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            @click="navigateStep(3, true)"
                        >
                            Continue
                        </button>
                    </div>
                </section>

                <section
                    v-else-if="step === 3"
                    id="install-step-panel"
                    class="panel"
                    aria-labelledby="install-step-title"
                >
                    <h2 id="install-step-title" class="title" tabindex="-1">Instance</h2>
                    <div class="mt-6 grid gap-4 sm:grid-cols-2">
                        <InstallationField
                            v-model="form.instance.name"
                            path="instance.name"
                            label="Instance name"
                            :error="fieldErrors('instance.name').join(' ')"
                            autocomplete="organization"
                            required
                            maxlength="255"
                            :disabled="managed.instance"
                            @update:model-value="clearError('instance.name')"
                        />
                        <InstallationField
                            v-model="form.instance.url"
                            path="instance.url"
                            label="Public URL"
                            :error="fieldErrors('instance.url').join(' ')"
                            type="url"
                            placeholder="https://files.example.com"
                            autocomplete="url"
                            required
                            :disabled="managed.instance"
                            @update:model-value="clearError('instance.url')"
                        />
                        <InstallationField
                            v-model="form.instance.username_domain"
                            path="instance.username_domain"
                            label="Username domain"
                            :error="fieldErrors('instance.username_domain').join(' ')"
                            description="A hostname such as example.com, without https:// or a path."
                            placeholder="example.com"
                            autocomplete="off"
                            maxlength="253"
                            pattern="[A-Za-z0-9](?:[A-Za-z0-9\-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9\-]{0,61}[A-Za-z0-9])?)*\.?"
                            :disabled="managed.instance"
                            @update:model-value="clearError('instance.username_domain')"
                        />
                        <InstallationField
                            v-model="form.instance.visibility"
                            path="instance.visibility"
                            label="Visibility"
                            :error="fieldErrors('instance.visibility').join(' ')"
                            type="select"
                            required
                            @update:model-value="clearError('instance.visibility')"
                            ><option value="private">Private</option>
                            <option value="public">Public</option></InstallationField
                        >
                    </div>
                    <p v-if="managed.instance" class="mt-3 text-sm text-[var(--fb-text-muted)]">
                        Instance identity is managed by the container runtime. Correct the container
                        environment configuration and reload to retry.
                    </p>
                    <p class="mt-4 text-sm leading-6 text-[var(--fb-text-muted)]">
                        Private instances disable registration and anonymous uploads. Existing
                        download links remain available.
                    </p>
                    <InstallationField
                        v-model="form.instance.auto_updates_enabled"
                        path="instance.auto_updates_enabled"
                        label="Enable automatic updates"
                        :error="fieldErrors('instance.auto_updates_enabled').join(' ')"
                        type="checkbox"
                        class="mt-6"
                        :disabled="managed.auto_updates"
                        @update:model-value="clearError('instance.auto_updates_enabled')"
                    />
                    <p class="mt-2 text-sm leading-6 text-[var(--fb-text-muted)]">
                        <template v-if="managed.auto_updates"
                            >Containers never modify their application image. Deploy a new image to
                            update Filebeam.</template
                        >
                        <template v-else>
                            The daily scheduled release check installs compatible, signed release
                            packages automatically. It requires the scheduler, updater write access
                            to the package, sufficient backup disk quota, and enough cron runtime
                            for the backup and update.
                            <template v-if="form.database.driver === 'pgsql'">
                                PostgreSQL also requires
                                <code>pg_dump</code> and <code>pg_restore</code> installed at the
                                exact same major version as the server; the updater checks their
                                versions before updating.
                            </template>
                        </template>
                    </p>
                    <div class="actions">
                        <button
                            type="button"
                            class="fb-button fb-button--secondary"
                            @click="navigateStep(2)"
                        >
                            Back
                        </button>
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            @click="navigateStep(4, true)"
                        >
                            Continue
                        </button>
                    </div>
                </section>

                <section
                    v-else-if="step === 4"
                    id="install-step-panel"
                    class="panel"
                    aria-labelledby="install-step-title"
                >
                    <h2 id="install-step-title" class="title" tabindex="-1">Storage and chunks</h2>
                    <p
                        v-if="fieldErrors('storage').length"
                        id="install-storage-error"
                        class="section-error"
                        tabindex="-1"
                    >
                        {{ fieldErrors('storage').join(' ') }}
                    </p>
                    <div
                        v-for="(store, index) in form.storage"
                        :key="index"
                        class="mt-6 rounded-xl border border-[var(--fb-border)] bg-[var(--fb-bg)] p-4"
                    >
                        <div class="flex items-center justify-between gap-3">
                            <h3 class="font-semibold text-[var(--fb-text)]">
                                Store {{ index + 1 }}
                            </h3>
                            <button
                                v-if="form.storage.length > 1"
                                type="button"
                                class="fb-button fb-button--danger"
                                @click="removeStore(index)"
                            >
                                Remove
                            </button>
                        </div>
                        <p
                            v-if="fieldErrors(`storage.${index}`).length"
                            :id="`install-storage-${index}-error`"
                            class="section-error"
                            tabindex="-1"
                        >
                            {{ fieldErrors(`storage.${index}`).join(' ') }}
                        </p>
                        <div class="mt-4 grid gap-4 sm:grid-cols-2">
                            <InstallationField
                                v-model="store.name"
                                :path="`storage.${index}.name`"
                                label="Name"
                                :error="fieldErrors(`storage.${index}.name`).join(' ')"
                                required
                                maxlength="255"
                                @update:model-value="clearError(`storage.${index}.name`)"
                            />
                            <InstallationField
                                v-model="store.driver"
                                :path="`storage.${index}.driver`"
                                label="Driver"
                                :error="fieldErrors(`storage.${index}.driver`).join(' ')"
                                type="select"
                                required
                                @update:model-value="clearError(`storage.${index}.driver`)"
                                ><option value="local">Local</option>
                                <option value="s3">S3 compatible</option></InstallationField
                            >
                            <InstallationField
                                v-if="store.driver === 'local'"
                                v-model="store.root"
                                :path="`storage.${index}.root`"
                                label="Relative storage path"
                                :error="fieldErrors(`storage.${index}.root`).join(' ')"
                                description="Paths are relative to the application base directory."
                                class="sm:col-span-2"
                                placeholder="storage/app/filebeam"
                                required
                                @update:model-value="clearError(`storage.${index}.root`)"
                            />
                            <template v-else>
                                <InstallationField
                                    v-model="store.bucket"
                                    :path="`storage.${index}.bucket`"
                                    label="Bucket"
                                    :error="fieldErrors(`storage.${index}.bucket`).join(' ')"
                                    required
                                    @update:model-value="clearError(`storage.${index}.bucket`)"
                                />
                                <InstallationField
                                    v-model="store.region"
                                    :path="`storage.${index}.region`"
                                    label="Region"
                                    :error="fieldErrors(`storage.${index}.region`).join(' ')"
                                    required
                                    @update:model-value="clearError(`storage.${index}.region`)"
                                />
                                <InstallationField
                                    v-model="store.key"
                                    :path="`storage.${index}.key`"
                                    label="Access key"
                                    :error="fieldErrors(`storage.${index}.key`).join(' ')"
                                    autocomplete="off"
                                    required
                                    @update:model-value="clearError(`storage.${index}.key`)"
                                />
                                <InstallationField
                                    v-model="store.secret"
                                    :path="`storage.${index}.secret`"
                                    label="Secret key"
                                    :error="fieldErrors(`storage.${index}.secret`).join(' ')"
                                    type="password"
                                    autocomplete="new-password"
                                    required
                                    @update:model-value="clearError(`storage.${index}.secret`)"
                                />
                                <InstallationField
                                    v-model="store.endpoint"
                                    :path="`storage.${index}.endpoint`"
                                    label="HTTPS endpoint"
                                    :error="fieldErrors(`storage.${index}.endpoint`).join(' ')"
                                    description="Use an HTTPS endpoint. Enable path-style only when your provider requires it."
                                    class="sm:col-span-2"
                                    type="url"
                                    pattern="https://.*"
                                    placeholder="https://s3.example.com"
                                    @update:model-value="clearError(`storage.${index}.endpoint`)"
                                />
                                <InstallationField
                                    v-model="store.use_path_style_endpoint"
                                    :path="`storage.${index}.use_path_style_endpoint`"
                                    label="Use path-style endpoint"
                                    :error="
                                        fieldErrors(
                                            `storage.${index}.use_path_style_endpoint`,
                                        ).join(' ')
                                    "
                                    type="checkbox"
                                    class="sm:col-span-2"
                                    @update:model-value="
                                        clearError(`storage.${index}.use_path_style_endpoint`)
                                    "
                                />
                            </template>
                        </div>
                        <div class="mt-4 flex items-center gap-3">
                            <button
                                type="button"
                                class="fb-button fb-button--secondary"
                                :disabled="loading"
                                @click="testStore(index)"
                            >
                                Test store</button
                            ><span v-if="testedStores[index]" class="success">Store verified</span>
                        </div>
                    </div>
                    <button
                        type="button"
                        class="fb-button fb-button--secondary mt-4"
                        :disabled="form.storage.length >= 8"
                        @click="addStore"
                    >
                        + Add another store
                    </button>
                    <p
                        v-if="form.storage.length >= 8"
                        class="mt-2 text-xs text-[var(--fb-text-muted)]"
                    >
                        A maximum of 8 stores can be configured.
                    </p>
                    <div class="mt-8 border-t border-[var(--fb-border)] pt-6">
                        <h3 class="font-semibold text-[var(--fb-text)]">Chunk size</h3>
                        <div class="mt-4 grid gap-4 sm:grid-cols-[1fr_auto]">
                            <InstallationField
                                v-model="form.chunk_max_size"
                                path="chunk_max_size"
                                label="Maximum encrypted body bytes"
                                :error="fieldErrors('chunk_max_size').join(' ')"
                                :description="`${chunkMiB} MiB. Allowed range: ${chunks?.minimum.toLocaleString()} to ${chunks?.maximum.toLocaleString()} bytes.`"
                                type="number"
                                :min="chunks?.minimum"
                                :max="chunks?.maximum"
                                required
                                @update:model-value="clearError('chunk_max_size')"
                            />
                            <button
                                type="button"
                                class="fb-button fb-button--secondary self-end"
                                :disabled="probeStatus === 'testing' || !canProbe"
                                @click="manualProbe"
                            >
                                {{ probeStatus === 'testing' ? 'Testing...' : 'Test chunk size' }}
                            </button>
                        </div>
                        <p
                            v-if="chunkWarning"
                            class="mt-4 rounded-lg border border-[var(--fb-warning)] bg-[var(--fb-selected-surface)] p-3 text-sm text-[var(--fb-warning)]"
                        >
                            Your server may not support this chunk size
                        </p>
                        <InstallationField
                            v-if="chunkWarning"
                            v-model="form.chunk_warning_acknowledged"
                            path="chunk_warning_acknowledged"
                            label="I understand and want to continue."
                            :error="fieldErrors('chunk_warning_acknowledged').join(' ')"
                            type="checkbox"
                            class="mt-3"
                            @update:model-value="clearError('chunk_warning_acknowledged')"
                        />
                        <p class="mt-3 text-sm text-[var(--fb-text-muted)]" aria-live="polite">
                            {{ probeMessage
                            }}<span v-if="chunkStale && testedChunkSize !== null">
                                The tested size is stale.</span
                            >
                        </p>
                        <p class="mt-4 text-xs leading-5 text-[var(--fb-text-muted)]">
                            Encrypted request bodies include
                            {{ chunks?.overhead ?? 16 }} bytes of overhead.
                            <code>upload_max_filesize</code> ({{ chunks?.upload_max_filesize }})
                            diagnoses PHP multipart uploads; it does not limit this raw PUT. Proxies
                            may have unknown limits, and a successful probe does not guarantee the
                            actual upload path. This installer never performs production transfers
                            automatically.
                        </p>
                    </div>
                    <div class="actions">
                        <button
                            type="button"
                            class="fb-button fb-button--secondary"
                            @click="navigateStep(3)"
                        >
                            Back
                        </button>
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            @click="navigateStep(5, true)"
                        >
                            Continue
                        </button>
                    </div>
                </section>

                <section
                    v-else-if="step === 5"
                    id="install-step-panel"
                    class="panel"
                    aria-labelledby="install-step-title"
                >
                    <h2 id="install-step-title" class="title" tabindex="-1">
                        Create the first administrator
                    </h2>
                    <div class="mt-6 grid gap-4 sm:grid-cols-2">
                        <InstallationField
                            v-model="form.admin.name"
                            path="admin.name"
                            label="Name"
                            :error="fieldErrors('admin.name').join(' ')"
                            required
                            maxlength="255"
                            autocomplete="name"
                            @update:model-value="clearError('admin.name')"
                        />
                        <InstallationField
                            v-model="form.admin.username"
                            path="admin.username"
                            label="Username"
                            :error="fieldErrors('admin.username').join(' ')"
                            required
                            minlength="3"
                            maxlength="24"
                            pattern="[a-z0-9_]{3,24}"
                            autocomplete="username"
                            @update:model-value="clearError('admin.username')"
                        />
                        <InstallationField
                            v-model="form.admin.email"
                            path="admin.email"
                            label="Email"
                            :error="fieldErrors('admin.email').join(' ')"
                            class="sm:col-span-2"
                            required
                            type="email"
                            autocomplete="email"
                            @update:model-value="clearError('admin.email')"
                        />
                        <InstallationField
                            v-model="form.admin.password"
                            path="admin.password"
                            label="Password"
                            :error="fieldErrors('admin.password').join(' ')"
                            required
                            type="password"
                            autocomplete="new-password"
                            @update:model-value="clearError('admin.password')"
                        />
                        <InstallationField
                            v-model="form.admin.password_confirmation"
                            path="admin.password_confirmation"
                            label="Confirm password"
                            :error="fieldErrors('admin.password_confirmation').join(' ')"
                            required
                            type="password"
                            autocomplete="new-password"
                            @update:model-value="clearError('admin.password_confirmation')"
                        />
                    </div>
                    <InstallationField
                        v-model="form.admin.email_ownership_confirmed"
                        path="admin.email_ownership_confirmed"
                        label="I confirm that I control this email address."
                        :error="fieldErrors('admin.email_ownership_confirmed').join(' ')"
                        type="checkbox"
                        class="mt-5"
                        required
                        @update:model-value="clearError('admin.email_ownership_confirmed')"
                    />
                    <div class="actions">
                        <button
                            type="button"
                            class="fb-button fb-button--secondary"
                            @click="navigateStep(4)"
                        >
                            Back
                        </button>
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            @click="navigateStep(6, true)"
                        >
                            Review installation
                        </button>
                    </div>
                </section>

                <section
                    v-else
                    id="install-step-panel"
                    class="panel"
                    aria-labelledby="install-step-title"
                >
                    <h2 id="install-step-title" class="title" tabindex="-1">
                        Complete installation
                    </h2>
                    <p class="copy mt-3">
                        The final step runs <code>php artisan optimize</code> to cache
                        configuration, routes, events, and views before opening Filebeam.
                    </p>
                    <dl class="mt-6 grid gap-4 text-sm sm:grid-cols-2">
                        <div class="summary">
                            <dt>Database</dt>
                            <dd>
                                {{ form.database.driver }} at
                                {{ form.database.host }}
                            </dd>
                        </div>
                        <div class="summary">
                            <dt>Cache</dt>
                            <dd v-if="form.cache.driver === 'file'">File</dd>
                            <dd v-else>
                                Redis / {{ form.cache.transport }} / {{ form.cache.host }} / DB
                                {{ form.cache.database }}
                            </dd>
                        </div>
                        <div class="summary">
                            <dt>Instance</dt>
                            <dd>
                                {{ form.instance.name }} /
                                {{ form.instance.visibility }}
                            </dd>
                        </div>
                        <div class="summary">
                            <dt>Storage</dt>
                            <dd>
                                {{ form.storage.length }} configured store{{
                                    form.storage.length === 1 ? '' : 's'
                                }}
                            </dd>
                        </div>
                        <div class="summary">
                            <dt>Chunk size</dt>
                            <dd>{{ formatBytes(form.chunk_max_size) }}</dd>
                        </div>
                        <div class="summary">
                            <dt>Automatic updates</dt>
                            <dd>
                                {{ form.instance.auto_updates_enabled ? 'Enabled' : 'Disabled' }}
                            </dd>
                        </div>
                    </dl>
                    <p
                        v-if="chunkWarning && !form.chunk_warning_acknowledged"
                        class="mt-5 text-sm text-[var(--fb-warning)]"
                    >
                        Acknowledge the chunk warning before completing installation.
                    </p>
                    <div class="actions">
                        <button
                            type="button"
                            class="fb-button fb-button--secondary"
                            @click="navigateStep(5)"
                        >
                            Back
                        </button>
                        <button
                            type="button"
                            class="fb-button fb-button--primary"
                            :disabled="
                                loading || (chunkWarning && !form.chunk_warning_acknowledged)
                            "
                            @click="complete"
                        >
                            {{ loading ? 'Installing...' : 'Complete installation' }}
                        </button>
                    </div>
                </section>
            </template>
        </main>
    </div>
</template>

<style scoped>
@reference '../../css/app.css';

.panel {
    padding: clamp(1.25rem, 4vw, 2rem);
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
}

.installer-content {
    width: 100%;
    max-width: 64rem;
    margin-inline: auto;
    padding: clamp(1.5rem, 4vw, 3rem) clamp(1rem, 4vw, 2rem) 4rem;
}
.installer-heading {
    margin-bottom: 2rem;
    font-size: clamp(1.75rem, 4vw, 2.25rem);
    font-weight: 600;
    letter-spacing: -0.04em;
}
.installer-progress {
    margin-bottom: 2rem;
}
.installer-progress__mobile {
    display: none;
}
.installer-steps {
    display: flex;
}
.installer-step {
    position: relative;
    flex: 1;
    min-width: 0;
}
.installer-step:not(:last-child)::after {
    content: '';
    position: absolute;
    top: 1rem;
    left: calc(50% + 1.35rem);
    right: calc(-50% + 1.35rem);
    height: 1px;
    background: var(--fb-border);
}
.installer-step.is-complete::after {
    background: var(--fb-accent-text);
}
.installer-step__button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    min-height: 4.5rem;
    padding-inline: 0.25rem;
    border-radius: var(--fb-radius-sm);
    color: var(--fb-text-muted);
    font-family: var(--fb-font-ui);
    font-size: var(--fb-font-small);
    font-weight: 500;
    cursor: pointer;
}
.installer-step__button:disabled {
    cursor: not-allowed;
}
.installer-step__button:not(:disabled):hover {
    color: var(--fb-text);
}
.installer-step__marker {
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    border: 1px solid var(--fb-control-border);
    border-radius: 50%;
    background: var(--fb-bg);
}
.is-complete .installer-step__marker {
    border-color: var(--fb-accent-text);
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.is-current .installer-step__button {
    color: var(--fb-text);
    font-weight: 600;
}
.is-current .installer-step__marker {
    border-color: var(--fb-action);
    color: var(--fb-on-action);
    background: var(--fb-action);
    box-shadow: 0 0 0 4px var(--fb-selected-surface);
}
.has-error .installer-step__marker {
    border-color: var(--fb-danger);
    color: var(--fb-danger);
    background: var(--fb-surface);
}
.installer-message {
    margin-bottom: 1.5rem;
    padding: 1rem 1.25rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-control);
    background: var(--fb-surface);
    font-size: var(--fb-font-small);
    overflow-wrap: anywhere;
}
.installer-message--error {
    border-color: var(--fb-danger);
    color: var(--fb-danger);
}
.installer-message--error a {
    text-decoration: underline;
    text-underline-offset: 3px;
}
.installer-message--warning {
    border-color: var(--fb-warning);
    color: var(--fb-warning);
}
.title {
    @apply text-2xl font-semibold tracking-tight;
}
.copy {
    @apply max-w-2xl text-sm leading-6;
    color: var(--fb-text-muted);
}
.hint {
    @apply text-sm leading-5 font-normal;
    color: var(--fb-text-muted);
}
.section-error {
    margin-top: 1rem;
    padding: 0.75rem 1rem;
    border-left: 2px solid var(--fb-danger);
    color: var(--fb-danger);
    font-size: var(--fb-font-small);
}
.actions {
    @apply mt-8 flex flex-wrap items-center justify-between gap-3 border-t pt-6;
    border-color: var(--fb-border);
}
.actions .fb-button--primary {
    margin-left: auto;
}
.success {
    @apply text-sm font-medium;
    color: var(--fb-success);
}
.summary {
    @apply rounded-xl border p-4;
    border-color: var(--fb-border);
    background: var(--fb-bg);
    overflow-wrap: anywhere;
}
.summary dt {
    @apply text-sm;
    color: var(--fb-text-muted);
}
.summary dd {
    @apply mt-1 font-medium;
}
@media (max-width: 639px) {
    .installer-progress__mobile {
        display: grid;
        gap: 0.5rem;
        margin-bottom: 1rem;
        color: var(--fb-text-muted);
        font-size: var(--fb-font-small);
    }
    .installer-step__label {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip-path: inset(50%);
    }
    .installer-step__button {
        min-height: 2.75rem;
    }
}
</style>
