<script setup lang="ts">
import { computed, ref } from 'vue';
import Icon from '../primitives/Icon.vue';
import { onTabKeydown } from './tabs';
import type { TransferDriver } from '../../types';

const props = defineProps<{ enabledDrivers: TransferDriver[]; cliDocsUrl: string }>();

const transports = {
    http: {
        label: 'HTTP',
        hint: 'Download later',
        icon: 'http-server',
        caption: 'Encrypted storage',
        description: 'Upload once. Share a link. The recipient can download before it expires.',
        detail: 'The server stores ciphertext, not file contents.',
    },
    webrtc: {
        label: 'WebRTC',
        hint: 'Send live',
        icon: 'webrtc-p2p',
        caption: 'Live connection',
        description: 'Connect live over WebRTC. Keep the sender open while the files transfer.',
        detail: 'Peer-to-peer where possible. An encrypted relay may be needed.',
    },
} as const;
const transport = ref<TransferDriver>('http');
const turbo = ref(false);
const active = computed(() => transports[transport.value]);
const available = computed(() => props.enabledDrivers.includes(transport.value));
const description = computed(() =>
    transport.value === 'http' && turbo.value
        ? 'Share the link before upload finishes. Recipients receive verified chunks as they arrive.'
        : active.value.description,
);
const caption = computed(() =>
    transport.value === 'http' && turbo.value ? 'Published chunks' : active.value.caption,
);
const detail = computed(() =>
    transport.value === 'http' && turbo.value
        ? 'HTTP only. Keep the sender running until upload completes.'
        : active.value.detail,
);
</script>

<template>
    <section id="sharing" class="sharing-section feature-section" aria-labelledby="sharing-title">
        <div class="section-copy">
            <div class="section-icon"><Icon class="icon" name="fast-and-simple" /></div>
            <h2 id="sharing-title">Fast and simple.</h2>
            <p>Share files and notes in seconds.</p>
        </div>
        <div class="transport-demo" :data-transport="transport === 'http' ? 'http' : 'p2p'">
            <div class="transport-tabs" role="tablist" aria-label="Transfer method">
                <button
                    v-for="(item, key) in transports"
                    :id="`transport-${key}-tab`"
                    :key="key"
                    type="button"
                    role="tab"
                    aria-controls="transport-panel"
                    :aria-selected="transport === key"
                    :tabindex="transport === key ? 0 : -1"
                    @click="transport = key"
                    @keydown="onTabKeydown"
                >
                    <Icon class="icon" :name="item.icon" /><span
                        >{{ item.label }}<small>{{ item.hint }}</small></span
                    >
                </button>
            </div>
            <div
                id="transport-panel"
                class="transport-body"
                role="tabpanel"
                :aria-labelledby="`transport-${transport}-tab`"
            >
                <div class="route-graphic" aria-hidden="true">
                    <div class="route-endpoint">
                        <Icon class="icon" name="laptop" /><span>You</span>
                    </div>
                    <div class="route-line"><i /></div>
                    <div class="route-center">
                        <span><Icon class="icon" :name="active.icon" /></span
                        ><span>{{ caption }}</span>
                    </div>
                    <div class="route-line"><i /></div>
                    <div class="route-endpoint">
                        <Icon class="icon" name="laptop" /><span>Recipient</span>
                    </div>
                </div>
                <p>{{ description }}</p>
                <div class="turbo-row">
                    <span
                        ><Icon class="icon" name="bolt" /> Turbo Transfer
                        <small>Share while uploading</small></span
                    >
                    <button
                        type="button"
                        class="switch"
                        role="switch"
                        :aria-checked="transport === 'http' && turbo"
                        :disabled="transport !== 'http'"
                        aria-label="Show how Turbo Transfer works (illustration)"
                        @click="turbo = !turbo"
                    >
                        <span />
                    </button>
                </div>
                <div class="transport-detail">
                    <span v-if="available"><Icon class="icon" name="lock" /> {{ detail }}</span>
                    <span v-else
                        ><Icon class="icon" name="info" /> {{ active.label }} isn’t enabled on this
                        server.</span
                    >
                    <a :href="cliDocsUrl" target="_blank" rel="noopener noreferrer"
                        >CLI supported <Icon class="icon" name="arrow-up-right"
                    /></a>
                </div>
            </div>
        </div>
    </section>
</template>
