<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import {
    bootstrap as bootstrapAction,
    cache as cacheAction,
    complete as completeAction,
    configuration as configurationAction,
    database as databaseAction,
    probe as probeAction,
    storage as storageAction,
} from '@/actions/App/Http/Controllers/InstallationController';
import { computed, reactive, ref, watch } from 'vue';

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
            errors: payload.errors ?? {},
        });
    }

    return payload;
}

function showFailure(error: unknown): void {
    const failure = error as Error & { errors?: Record<string, string[]> };
    errors.value = failure.errors ?? {};
    requestError.value = failure.message || 'The server could not process this request.';
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
    errors.value = {};

    try {
        const result = await json<{ message: string }>(url, body);
        success();
        notice.value = result.message || 'Connection verified.';
    } catch (error) {
        showFailure(error);
        if (errorPrefix) {
            errors.value = Object.fromEntries(
                Object.entries(errors.value).map(([field, messages]) => [
                    field.startsWith('storage.')
                        ? `${errorPrefix}.${field.slice('storage.'.length)}`
                        : field,
                    messages,
                ]),
            );
        }
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
    }
}

function removeStore(index: number): void {
    if (form.storage.length > 1) {
        form.storage.splice(index, 1);
        Object.keys(testedStores).forEach((key) => delete testedStores[Number(key)]);
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
        Object.keys(errors.value)
            .filter((path) => path.startsWith('cache.'))
            .forEach((path) => delete errors.value[path]);
    },
    { deep: true },
);
</script>

<template>
    <main class="min-h-screen bg-[var(--fb-bg)] px-4 py-8 font-sans text-slate-100 sm:px-6 lg:px-8">
        <Head title="Install Filebeam" />

        <div class="mx-auto max-w-5xl">
            <header
                class="mb-8 flex flex-col gap-5 border-b border-slate-700/70 pb-6 sm:flex-row sm:items-end sm:justify-between"
            >
                <div>
                    <div class="mb-3 flex items-center gap-3 text-purple-300">
                        <img src="/brand/filebeam-mark.svg" alt="" class="h-10 w-10" />
                        <span class="font-mono text-sm font-semibold tracking-[0.24em]"
                            >FILEBEAM</span
                        >
                    </div>
                    <h1 class="text-3xl font-semibold tracking-tight text-white">
                        Set up your secure transfer service
                    </h1>
                    <p class="mt-2 max-w-2xl text-sm leading-6 text-slate-400">
                        This installer keeps tokens and credentials in this browser tab only.
                        Nothing is stored in browser history or local storage.
                    </p>
                </div>
                <p class="font-mono text-xs text-slate-500">INSTALLER / v1</p>
            </header>

            <div
                v-if="unavailableReason"
                class="rounded-xl border border-amber-400/30 bg-amber-400/10 p-5 text-sm text-amber-100"
                role="alert"
            >
                <p class="font-semibold">Installation is unavailable</p>
                <p class="mt-1 text-amber-100/80">{{ unavailableReason }}</p>
            </div>

            <template v-else>
                <nav class="mb-8 overflow-x-auto" aria-label="Installation progress">
                    <ol class="flex min-w-max gap-2">
                        <li
                            v-for="(name, index) in steps"
                            :key="name"
                            class="flex items-center gap-2"
                        >
                            <button
                                type="button"
                                class="rounded-full px-3 py-2 text-xs font-semibold"
                                :class="
                                    step === index + 1
                                        ? 'bg-purple-300 text-slate-950'
                                        : step > index + 1
                                          ? 'bg-purple-300/15 text-purple-200'
                                          : 'bg-slate-800 text-slate-400'
                                "
                                :disabled="(!ready || !prerequisitesPassed) && index > 0"
                                @click="step = index + 1"
                            >
                                <span class="mr-1 font-mono">0{{ index + 1 }}</span
                                >{{ name }}
                            </button>
                        </li>
                    </ol>
                </nav>

                <div
                    v-if="requestError"
                    class="mb-6 rounded-xl border border-rose-400/30 bg-rose-400/10 p-4 text-sm text-rose-100"
                    role="alert"
                >
                    {{ requestError }}
                </div>
                <div
                    v-if="Object.keys(errors).length"
                    class="mb-6 rounded-xl border border-rose-400/20 bg-slate-900 p-4 text-sm text-rose-100"
                    role="alert"
                >
                    <p class="font-semibold">Please correct the highlighted values.</p>
                    <ul class="mt-2 list-disc space-y-1 pl-5">
                        <li v-for="(messages, field) in errors" :key="field">
                            {{ messages.join(' ') }}
                        </li>
                    </ul>
                </div>
                <div
                    v-if="notice"
                    class="mb-6 rounded-xl border border-purple-300/25 bg-purple-300/10 p-4 text-sm text-purple-50"
                    role="status"
                >
                    {{ notice }}
                </div>

                <section v-if="step === 1" class="panel">
                    <p class="eyebrow">01 / Access & readiness</p>
                    <h2 class="title">Authorize this installation</h2>
                    <div v-if="bootstrapRequired" class="mt-6 space-y-4">
                        <p class="copy">
                            Create the one-time server-side bootstrap state. This request is signed
                            with the challenge provided by the initial page, not a browser session.
                        </p>
                        <button
                            type="button"
                            class="button-primary"
                            :disabled="loading || !challenge"
                            @click="bootstrap"
                        >
                            {{ loading ? 'Preparing...' : 'Prepare installation' }}
                        </button>
                        <p v-if="!challenge" class="text-sm text-rose-200">
                            The installation challenge is missing. Reload this page to obtain a
                            fresh challenge.
                        </p>
                    </div>
                    <div v-else class="mt-6 space-y-5">
                        <div
                            class="rounded-lg border border-purple-300/25 bg-purple-300/10 p-4 text-sm leading-6 text-purple-50"
                        >
                            Open the server
                            <code class="font-mono text-purple-200">{{
                                managed.container ? '/data/config/.env' : '.env'
                            }}</code
                            >, copy
                            <code class="font-mono text-purple-200">FILEBEAM_INSTALL_TOKEN</code>,
                            then paste it below. The token never leaves this tab except in
                            authenticated installer requests.
                        </div>
                        <label class="field">
                            <span>Installation token</span>
                            <input
                                v-model="installationToken"
                                class="input"
                                type="password"
                                autocomplete="off"
                                spellcheck="false"
                                @input="clearError('token')"
                            />
                            <small
                                v-for="error in fieldErrors('token')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            >
                        </label>
                        <button
                            type="button"
                            class="button-primary"
                            :disabled="loading || !installationToken"
                            @click="loadConfiguration"
                        >
                            {{ loading ? 'Checking...' : 'Check server readiness' }}
                        </button>
                    </div>
                    <div v-if="prerequisites.length" class="mt-8 border-t border-slate-700 pt-6">
                        <h3 class="font-semibold text-white">Prerequisites</h3>
                        <ul class="mt-3 grid gap-2 sm:grid-cols-2">
                            <li
                                v-for="prerequisite in prerequisites"
                                :key="prerequisite.label"
                                class="flex items-center gap-2 rounded-lg bg-slate-800/70 px-3 py-2 text-sm"
                                :class="prerequisite.passed ? 'text-slate-200' : 'text-rose-200'"
                            >
                                <span aria-hidden="true">{{
                                    prerequisite.passed ? 'OK' : '!'
                                }}</span
                                >{{ prerequisite.label }}
                            </li>
                        </ul>
                        <button
                            type="button"
                            class="button-primary mt-5"
                            :disabled="!prerequisitesPassed"
                            @click="step = 2"
                        >
                            Continue to database
                        </button>
                        <p v-if="!prerequisitesPassed" class="mt-3 text-sm text-rose-200">
                            Resolve every failed prerequisite before continuing.
                        </p>
                    </div>
                </section>

                <section v-else-if="step === 2" class="panel">
                    <p class="eyebrow">02 / Database & cache</p>
                    <h2 class="title">Connect Filebeam's database</h2>
                    <p v-if="managed.database" class="mt-3 text-sm text-slate-400">
                        Database settings are managed by the container runtime. Credentials remain
                        server-side.
                    </p>
                    <fieldset
                        :disabled="managed.database"
                        class="mt-6 grid gap-4 sm:grid-cols-2 disabled:opacity-70"
                    >
                        <label class="field"
                            ><span>Driver</span
                            ><select v-model="form.database.driver" class="input">
                                <option
                                    v-for="driver in databaseDrivers"
                                    :key="driver"
                                    :value="driver"
                                >
                                    {{ driver }}
                                </option>
                            </select></label
                        >
                        <label v-if="form.database.driver !== 'sqlite'" class="field">
                            <span>Connection</span>
                            <select v-model="form.database.transport" class="input">
                                <option value="tcp">TCP (hostname or IP)</option>
                                <option value="socket">Unix socket</option>
                            </select>
                        </label>
                        <label
                            v-if="
                                form.database.driver !== 'sqlite' &&
                                form.database.transport === 'socket'
                            "
                            class="field sm:col-span-2"
                        >
                            <span>{{
                                form.database.driver === 'pgsql'
                                    ? 'Socket directory'
                                    : 'Socket file'
                            }}</span>
                            <input
                                v-model="form.database.socket"
                                class="input"
                                autocomplete="off"
                                :placeholder="
                                    form.database.driver === 'pgsql'
                                        ? '/var/run/postgresql'
                                        : '/run/mysqld/mysqld.sock'
                                "
                            />
                            <small class="hint">{{
                                form.database.driver === 'pgsql'
                                    ? 'PostgreSQL uses this directory and the port below to locate its Unix socket.'
                                    : 'Use the full path to the MySQL or MariaDB Unix socket; host and port are not used.'
                            }}</small>
                            <small
                                v-for="error in fieldErrors('database.socket')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            >
                        </label>
                        <label
                            v-if="
                                form.database.driver !== 'sqlite' &&
                                form.database.transport === 'tcp'
                            "
                            class="field"
                            ><span>Host</span
                            ><input
                                v-model="form.database.host"
                                class="input"
                                autocomplete="off"
                            /><small
                                v-for="error in fieldErrors('database.host')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        >
                        <label
                            v-if="
                                form.database.driver !== 'sqlite' &&
                                (form.database.transport === 'tcp' ||
                                    form.database.driver === 'pgsql')
                            "
                            class="field"
                            ><span>Port</span
                            ><input
                                v-model="form.database.port"
                                class="input"
                                inputmode="numeric"
                            /><small
                                v-for="error in fieldErrors('database.port')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        >
                        <label class="field"
                            ><span>Database</span
                            ><input
                                v-model="form.database.database"
                                class="input"
                                autocomplete="off"
                            /><small
                                v-for="error in fieldErrors('database.database')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        >
                        <label v-if="form.database.driver !== 'sqlite'" class="field"
                            ><span>Username</span
                            ><input
                                v-model="form.database.username"
                                class="input"
                                autocomplete="username"
                        /></label>
                        <label v-if="form.database.driver !== 'sqlite'" class="field"
                            ><span>Password</span
                            ><input
                                v-model="form.database.password"
                                class="input"
                                type="password"
                                autocomplete="new-password"
                        /></label>
                        <label
                            v-if="
                                form.database.driver === 'pgsql' &&
                                form.database.transport === 'tcp'
                            "
                            class="field"
                            ><span>SSL mode</span
                            ><input
                                v-model="form.database.sslmode"
                                class="input"
                                autocomplete="off"
                        /></label>
                    </fieldset>
                    <div class="actions">
                        <button
                            type="button"
                            class="button-secondary"
                            :disabled="loading"
                            @click="testDatabase"
                        >
                            {{ loading ? 'Testing...' : 'Test database' }}</button
                        ><span v-if="databaseTested" class="success">Connection verified</span
                        ><button type="button" class="button-primary" @click="step = 3">
                            Continue
                        </button>
                    </div>
                    <div class="mt-8 border-t border-slate-700 pt-6">
                        <h3 class="font-semibold text-white">Cache configuration</h3>
                        <p class="mt-2 text-sm leading-6 text-slate-400">
                            Redis provides cache, locks, rate limits, and monitoring. The session
                            cookie is unchanged.
                        </p>
                        <p v-if="managed.cache" class="mt-3 text-sm text-slate-400">
                            Cache settings are managed by the container runtime. Credentials remain
                            server-side.
                        </p>
                        <fieldset
                            :disabled="managed.cache"
                            class="mt-4 grid gap-4 sm:grid-cols-2 disabled:opacity-70"
                        >
                            <label class="field">
                                <span>Driver</span>
                                <select v-model="form.cache.driver" class="input">
                                    <option value="file">File</option>
                                    <option value="redis" :disabled="!redisAvailable">Redis</option>
                                </select>
                                <small v-if="!redisAvailable" class="hint">
                                    Redis requires the PHP Redis extension.
                                </small>
                                <small
                                    v-for="error in fieldErrors('cache.driver')"
                                    :key="error"
                                    class="error"
                                    >{{ error }}</small
                                >
                            </label>
                            <template v-if="form.cache.driver === 'redis'">
                                <label class="field">
                                    <span>Connection</span>
                                    <select v-model="form.cache.transport" class="input">
                                        <option value="tcp">TCP</option>
                                        <option value="tls">TLS</option>
                                        <option value="unix">Unix socket</option>
                                    </select>
                                    <small
                                        v-for="error in fieldErrors('cache.transport')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                                <label
                                    class="field"
                                    :class="{
                                        'sm:col-span-2': form.cache.transport === 'unix',
                                    }"
                                >
                                    <span>{{
                                        form.cache.transport === 'unix' ? 'Socket path' : 'Host'
                                    }}</span>
                                    <input
                                        v-model="form.cache.host"
                                        class="input"
                                        autocomplete="off"
                                        :placeholder="
                                            form.cache.transport === 'unix'
                                                ? '/run/redis/redis-server.sock'
                                                : '127.0.0.1'
                                        "
                                    />
                                    <small
                                        v-for="error in fieldErrors('cache.host')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                                <label v-if="form.cache.transport !== 'unix'" class="field">
                                    <span>Port</span>
                                    <input
                                        v-model="form.cache.port"
                                        class="input"
                                        inputmode="numeric"
                                    />
                                    <small
                                        v-for="error in fieldErrors('cache.port')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                                <label class="field">
                                    <span
                                        >Username
                                        <small class="font-normal text-slate-500"
                                            >optional</small
                                        ></span
                                    >
                                    <input
                                        v-model="form.cache.username"
                                        class="input"
                                        autocomplete="username"
                                    />
                                    <small
                                        v-for="error in fieldErrors('cache.username')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                                <label class="field">
                                    <span
                                        >Password
                                        <small class="font-normal text-slate-500"
                                            >optional</small
                                        ></span
                                    >
                                    <input
                                        v-model="form.cache.password"
                                        class="input"
                                        type="password"
                                        autocomplete="new-password"
                                    />
                                    <small
                                        v-for="error in fieldErrors('cache.password')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                                <label class="field">
                                    <span>Database index</span>
                                    <input
                                        v-model.number="form.cache.database"
                                        class="input"
                                        type="number"
                                        min="0"
                                        inputmode="numeric"
                                    />
                                    <small
                                        v-for="error in fieldErrors('cache.database')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                                <label class="field">
                                    <span>Prefix</span>
                                    <input
                                        v-model="form.cache.prefix"
                                        class="input"
                                        autocomplete="off"
                                    />
                                    <small class="hint"
                                        >Use a unique prefix for this deployment.</small
                                    >
                                    <small
                                        v-for="error in fieldErrors('cache.prefix')"
                                        :key="error"
                                        class="error"
                                        >{{ error }}</small
                                    >
                                </label>
                            </template>
                        </fieldset>
                        <div class="mt-4 flex items-center gap-3">
                            <button
                                type="button"
                                class="button-secondary"
                                :disabled="loading"
                                @click="testCache"
                            >
                                {{ loading ? 'Testing...' : 'Test cache' }}
                            </button>
                            <span v-if="cacheTested" class="success">Cache verified</span>
                        </div>
                    </div>
                </section>

                <section v-else-if="step === 3" class="panel">
                    <p class="eyebrow">03 / Instance</p>
                    <h2 class="title">Name and expose your service</h2>
                    <div class="mt-6 grid gap-4 sm:grid-cols-2">
                        <label class="field"
                            ><span>Instance name</span
                            ><input
                                v-model="form.instance.name"
                                class="input"
                                autocomplete="organization"
                                :disabled="managed.instance"
                            /><small
                                v-for="error in fieldErrors('instance.name')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        >
                        <label class="field"
                            ><span>Public URL</span
                            ><input
                                v-model="form.instance.url"
                                class="input"
                                type="url"
                                placeholder="https://files.example.com"
                                autocomplete="url"
                                :disabled="managed.instance"
                            /><small
                                v-for="error in fieldErrors('instance.url')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        >
                        <label class="field"
                            ><span>Username domain</span
                            ><input
                                v-model="form.instance.username_domain"
                                class="input"
                                placeholder="example.com"
                                autocomplete="off"
                                :disabled="managed.instance"
                        /></label>
                        <label class="field"
                            ><span>Visibility</span
                            ><select v-model="form.instance.visibility" class="input">
                                <option value="private">Private</option>
                                <option value="public">Public</option>
                            </select></label
                        >
                    </div>
                    <p v-if="managed.instance" class="mt-3 text-sm text-slate-400">
                        Instance identity is managed by the container runtime.
                    </p>
                    <p class="mt-4 text-sm leading-6 text-slate-400">
                        Private instances disable registration and anonymous uploads. Existing
                        download links remain available.
                    </p>
                    <label class="check mt-6">
                        <input
                            v-model="form.instance.auto_updates_enabled"
                            :disabled="managed.auto_updates"
                            type="checkbox"
                        />
                        Enable automatic updates
                    </label>
                    <p class="mt-2 text-sm leading-6 text-slate-400">
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
                    <small
                        v-for="error in fieldErrors('instance.auto_updates_enabled')"
                        :key="error"
                        class="error"
                        >{{ error }}</small
                    >
                    <div class="actions">
                        <button type="button" class="button-secondary" @click="step = 2">
                            Back</button
                        ><button type="button" class="button-primary" @click="step = 4">
                            Continue
                        </button>
                    </div>
                </section>

                <section v-else-if="step === 4" class="panel">
                    <p class="eyebrow">04 / Storage & chunks</p>
                    <h2 class="title">Store encrypted transfer bodies</h2>
                    <div
                        v-for="(store, index) in form.storage"
                        :key="index"
                        class="mt-6 rounded-xl border border-slate-700 bg-slate-950/40 p-4"
                    >
                        <div class="flex items-center justify-between gap-3">
                            <h3 class="font-semibold text-white">Store {{ index + 1 }}</h3>
                            <button
                                v-if="form.storage.length > 1"
                                type="button"
                                class="text-sm text-rose-200 hover:text-rose-100"
                                @click="removeStore(index)"
                            >
                                Remove
                            </button>
                        </div>
                        <div class="mt-4 grid gap-4 sm:grid-cols-2">
                            <label class="field"
                                ><span>Name</span><input v-model="store.name" class="input" /><small
                                    v-for="error in fieldErrors(`storage.${index}.name`)"
                                    :key="error"
                                    class="error"
                                    >{{ error }}</small
                                ></label
                            >
                            <label class="field"
                                ><span>Driver</span
                                ><select v-model="store.driver" class="input">
                                    <option value="local">Local</option>
                                    <option value="s3">S3 compatible</option>
                                </select></label
                            >
                            <template v-if="store.driver === 'local'"
                                ><label class="field sm:col-span-2"
                                    ><span>Relative storage path</span
                                    ><input
                                        v-model="store.root"
                                        class="input"
                                        placeholder="storage/app/filebeam"
                                    /><small class="hint"
                                        >Paths are relative to the application base
                                        directory.</small
                                    ></label
                                ></template
                            >
                            <template v-else
                                ><label class="field"
                                    ><span>Bucket</span
                                    ><input v-model="store.bucket" class="input" /></label
                                ><label class="field"
                                    ><span>Region</span
                                    ><input v-model="store.region" class="input" /></label
                                ><label class="field"
                                    ><span>Access key</span
                                    ><input
                                        v-model="store.key"
                                        class="input"
                                        autocomplete="off" /></label
                                ><label class="field"
                                    ><span>Secret key</span
                                    ><input
                                        v-model="store.secret"
                                        class="input"
                                        type="password"
                                        autocomplete="new-password" /></label
                                ><label class="field sm:col-span-2"
                                    ><span>HTTPS endpoint</span
                                    ><input
                                        v-model="store.endpoint"
                                        class="input"
                                        type="url"
                                        pattern="https://.*"
                                        placeholder="https://s3.example.com"
                                    /><small class="hint"
                                        >Use an HTTPS endpoint. Enable path-style only when your
                                        provider requires it.</small
                                    ></label
                                ><label class="check sm:col-span-2"
                                    ><input
                                        v-model="store.use_path_style_endpoint"
                                        type="checkbox"
                                    />
                                    Use path-style endpoint</label
                                ></template
                            >
                        </div>
                        <div class="mt-4 flex items-center gap-3">
                            <button
                                type="button"
                                class="button-secondary"
                                :disabled="loading"
                                @click="testStore(index)"
                            >
                                Test store</button
                            ><span v-if="testedStores[index]" class="success">Store verified</span>
                        </div>
                    </div>
                    <button
                        type="button"
                        class="mt-4 text-sm font-semibold text-purple-200 hover:text-purple-100 disabled:cursor-not-allowed disabled:opacity-50"
                        :disabled="form.storage.length >= 8"
                        @click="addStore"
                    >
                        + Add another store
                    </button>
                    <p v-if="form.storage.length >= 8" class="mt-2 text-xs text-slate-500">
                        A maximum of 8 stores can be configured.
                    </p>
                    <div class="mt-8 border-t border-slate-700 pt-6">
                        <h3 class="font-semibold text-white">Chunk size</h3>
                        <div class="mt-4 grid gap-4 sm:grid-cols-[1fr_auto]">
                            <label class="field"
                                ><span>Maximum encrypted body bytes</span
                                ><input
                                    v-model.number="form.chunk_max_size"
                                    class="input"
                                    type="number"
                                    :min="chunks?.minimum"
                                    :max="chunks?.maximum"
                                /><small class="hint"
                                    >{{ chunkMiB }} MiB. Allowed range:
                                    {{ chunks?.minimum.toLocaleString() }} to
                                    {{ chunks?.maximum.toLocaleString() }}
                                    bytes.</small
                                ><small
                                    v-for="error in fieldErrors('chunk_max_size')"
                                    :key="error"
                                    class="error"
                                    >{{ error }}</small
                                ></label
                            ><button
                                type="button"
                                class="button-secondary self-end"
                                :disabled="probeStatus === 'testing' || !canProbe"
                                @click="manualProbe"
                            >
                                {{ probeStatus === 'testing' ? 'Testing...' : 'Test chunk size' }}
                            </button>
                        </div>
                        <p
                            v-if="chunkWarning"
                            class="mt-4 rounded-lg border border-amber-400/30 bg-amber-400/10 p-3 text-sm text-amber-100"
                        >
                            Your server may not support this chunk size
                        </p>
                        <label v-if="chunkWarning" class="check mt-3"
                            ><input v-model="form.chunk_warning_acknowledged" type="checkbox" /> I
                            understand and want to continue.</label
                        >
                        <p class="mt-3 text-sm text-slate-400" aria-live="polite">
                            {{ probeMessage
                            }}<span v-if="chunkStale && testedChunkSize !== null">
                                The tested size is stale.</span
                            >
                        </p>
                        <p class="mt-4 text-xs leading-5 text-slate-500">
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
                        <button type="button" class="button-secondary" @click="step = 3">
                            Back</button
                        ><button type="button" class="button-primary" @click="step = 5">
                            Continue
                        </button>
                    </div>
                </section>

                <section v-else-if="step === 5" class="panel">
                    <p class="eyebrow">05 / Administrator</p>
                    <h2 class="title">Create the first administrator</h2>
                    <div class="mt-6 grid gap-4 sm:grid-cols-2">
                        <label class="field"
                            ><span>Name</span
                            ><input
                                v-model="form.admin.name"
                                class="input"
                                required
                                autocomplete="name"
                            /><small
                                v-for="error in fieldErrors('admin.name')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        ><label class="field"
                            ><span>Username</span
                            ><input
                                v-model="form.admin.username"
                                class="input"
                                required
                                autocomplete="username"
                            /><small
                                v-for="error in fieldErrors('admin.username')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        ><label class="field sm:col-span-2"
                            ><span>Email</span
                            ><input
                                v-model="form.admin.email"
                                class="input"
                                required
                                type="email"
                                autocomplete="email"
                            /><small
                                v-for="error in fieldErrors('admin.email')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        ><label class="field"
                            ><span>Password</span
                            ><input
                                v-model="form.admin.password"
                                class="input"
                                required
                                type="password"
                                autocomplete="new-password" /></label
                        ><label class="field"
                            ><span>Confirm password</span
                            ><input
                                v-model="form.admin.password_confirmation"
                                class="input"
                                required
                                type="password"
                                autocomplete="new-password"
                            /><small
                                v-for="error in fieldErrors('admin.password')"
                                :key="error"
                                class="error"
                                >{{ error }}</small
                            ></label
                        >
                    </div>
                    <label class="check mt-5"
                        ><input
                            v-model="form.admin.email_ownership_confirmed"
                            required
                            type="checkbox"
                        />
                        I confirm that I control this email address.</label
                    ><small
                        v-for="error in fieldErrors('admin.email_ownership_confirmed')"
                        :key="error"
                        class="error"
                        >{{ error }}</small
                    >
                    <div class="actions">
                        <button type="button" class="button-secondary" @click="step = 4">
                            Back</button
                        ><button type="button" class="button-primary" @click="step = 6">
                            Review installation
                        </button>
                    </div>
                </section>

                <section v-else class="panel">
                    <p class="eyebrow">06 / Review</p>
                    <h2 class="title">Complete installation</h2>
                    <p class="copy mt-3">
                        The final step runs php artisan optimize to cache configuration, routes,
                        events, and views before opening Filebeam.
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
                        class="mt-5 text-sm text-amber-100"
                    >
                        Acknowledge the chunk warning before completing installation.
                    </p>
                    <div class="actions">
                        <button type="button" class="button-secondary" @click="step = 5">
                            Back</button
                        ><button
                            type="button"
                            class="button-primary"
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
        </div>
    </main>
</template>

<style scoped>
@reference '../../css/app.css';

.panel {
    @apply rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-5 shadow-2xl shadow-black/20;
}

@media (min-width: 640px) {
    .panel {
        padding: 2rem;
    }
}
.eyebrow {
    @apply font-mono text-xs font-semibold tracking-[0.16em] text-purple-300;
}
.title {
    @apply mt-2 text-2xl font-semibold tracking-tight text-white;
}
.copy {
    @apply max-w-2xl text-sm leading-6 text-slate-300;
}
.field {
    @apply flex min-w-0 flex-col gap-1.5 text-sm font-medium text-slate-200;
}
.input {
    @apply min-h-11 w-full rounded-lg border border-[var(--fb-control-border)] bg-[var(--fb-editor-bg)] px-3 py-2 text-sm text-white outline-none placeholder:text-slate-600 focus:border-purple-300 focus:ring-2 focus:ring-purple-300/20;
}
.hint {
    @apply text-xs leading-5 font-normal text-slate-500;
}
.error {
    @apply text-xs font-normal text-rose-200;
}
.actions {
    @apply mt-8 flex flex-wrap items-center gap-3 border-t border-slate-700 pt-6;
}
.button-primary {
    @apply inline-flex min-h-10 items-center justify-center rounded-lg bg-[var(--fb-action)] px-4 py-2 text-sm font-semibold text-[var(--fb-on-action)] transition hover:bg-[var(--fb-action-hover)] disabled:cursor-not-allowed disabled:opacity-50;
}
.button-secondary {
    @apply inline-flex min-h-10 items-center justify-center rounded-lg border border-slate-600 bg-slate-800 px-4 py-2 text-sm font-semibold text-slate-100 transition hover:border-slate-500 hover:bg-slate-700 disabled:cursor-not-allowed disabled:opacity-50;
}
.success {
    @apply text-sm font-medium text-emerald-300;
}
.check {
    @apply flex items-center gap-2 text-sm text-slate-300;
}
.check input {
    @apply h-4 w-4 rounded border-slate-500 bg-slate-950 text-purple-300 focus:ring-purple-300/30;
}
.summary {
    @apply rounded-lg border border-slate-700 bg-slate-950/40 p-4;
}
.summary dt {
    @apply text-xs font-semibold tracking-wider text-slate-500 uppercase;
}
.summary dd {
    @apply mt-1 text-slate-200;
}
</style>
