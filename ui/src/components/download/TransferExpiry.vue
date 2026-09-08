<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import FilebeamIcon from '../primitives/FilebeamIcon.vue';
import Tooltip from '../primitives/Tooltip.vue';

const props = defineProps<{ expiresAt: string }>();
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | undefined;

const expiry = computed(() => new Date(props.expiresAt));

const relativeExpiry = computed(() => {
    const remaining = expiry.value.getTime() - now.value;
    if (!Number.isFinite(remaining)) return 'Expiry unavailable';
    if (remaining <= 0) return 'Expired';

    const minute = 60_000;
    const hour = 60 * minute;
    const day = 24 * hour;
    if (remaining < minute) return 'Expires in less than a minute';
    const [duration, unit]: [number, string] =
        remaining >= day ? [day, 'day'] : remaining >= hour ? [hour, 'hour'] : [minute, 'minute'];
    const count = Math.floor(remaining / duration);
    return `Expires in ${count} ${unit}${count === 1 ? '' : 's'}`;
});

const absoluteExpiry = computed(() => {
    const date = expiry.value;
    if (Number.isNaN(date.getTime())) return props.expiresAt;
    const pad = (value: number) => String(value).padStart(2, '0');
    return `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())} ${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}:${pad(date.getUTCSeconds())} UTC`;
});

onMounted(() => {
    timer = setInterval(() => {
        now.value = Date.now();
    }, 30_000);
});

onBeforeUnmount(() => {
    if (timer) clearInterval(timer);
});
</script>

<template>
    <Tooltip :content="absoluteExpiry" toggle-on-click>
        <button type="button" aria-label="Expiry details" class="fb-expiry-trigger">
            <FilebeamIcon name="clock" :size="17" />
            <span>{{ relativeExpiry }}</span>
        </button>
    </Tooltip>
</template>

<style scoped>
.fb-expiry-trigger {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    border: 0;
    border-radius: var(--fb-radius-sm);
    padding: 0.375rem 0;
    background: transparent;
    color: var(--fb-text-muted);
    cursor: help;
    transition: color var(--fb-duration-fast) ease;
}
.fb-expiry-trigger:hover,
.fb-expiry-trigger:focus-visible {
    color: var(--fb-text);
}
</style>
