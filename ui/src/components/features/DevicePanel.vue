<script setup lang="ts">
import { computed, ref } from 'vue';
import AuthLink from '../auth/AuthLink.vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';

defineProps<{ enabled: boolean; signedIn: boolean; registrationEnabled: boolean }>();

// Illustration only: choosing a destination never pairs devices or sends files.
// Device transfers run between computers (Linux and macOS) addressed by the name they were given.
const targets = {
    desktop: { label: 'Desktop', icon: 'monitor' },
    laptop: { label: 'Laptop', icon: 'laptop' },
    server: { label: 'Server', icon: 'storage' },
} as const;
type Target = keyof typeof targets;
const target = ref<Target>('desktop');
const device = computed(() => targets[target.value]);
</script>

<template>
    <section id="devices" class="device-panel feature-section" aria-labelledby="devices-title">
        <div class="device-heading">
            <div class="section-icon"><Icon class="icon" name="webrtc-p2p" /></div>
            <span class="pill">New</span><span class="spacer" />
        </div>
        <div class="device-grid">
            <div class="device-copy">
                <h2 id="devices-title">Device to device.</h2>
                <p>
                    Send files and folders to your own computers by name. They travel over an
                    encrypted peer-to-peer connection; the server only helps your devices connect.
                </p>
                <div class="device-facts">
                    <span><Icon class="icon" name="lock" /> Encrypted, peer-to-peer</span>
                    <span><Icon class="icon" name="folder" /> Lands in your Dropzone</span>
                </div>
                <div class="device-account">
                    Linux and macOS through the Beam CLI. Devices pair after you compare
                    fingerprints; revoke one from Account › My devices any time.
                </div>
                <AppLink v-if="signedIn" href="/account/devices" class="feature-cta"
                    >Manage devices <Icon class="icon" name="arrow-right"
                /></AppLink>
                <AuthLink
                    v-else-if="enabled"
                    :href="registrationEnabled ? '/register' : '/login'"
                    class="feature-cta"
                    >{{
                        registrationEnabled
                            ? 'Create an account to pair devices'
                            : 'Sign in to pair devices'
                    }}
                    <Icon class="icon" name="arrow-right"
                /></AuthLink>
            </div>
            <div class="device-playground" :data-target="target">
                <div class="device-command-row">
                    <div class="terminal-command">
                        <span class="terminal-prompt" aria-hidden="true">$</span
                        ><span class="command-verb">beam</span> to
                        <label class="target-select"
                            ><span class="sr-only">Destination device in the illustration</span
                            ><select v-model="target">
                                <option v-for="(item, key) in targets" :key="key" :value="key">
                                    {{ item.label }}
                                </option></select
                            ><Icon class="icon" name="chevron-down" :size="12" /></label
                        ><span class="command-file">field-notes.zip</span>
                    </div>
                    <span class="device-command-note">Straight to your device, peer-to-peer</span>
                </div>
                <div class="device-scene" aria-hidden="true">
                    <div class="computer source-computer">
                        <div class="computer-screen">
                            <div class="computer-chrome"><i /><i /><i /></div>
                            <div class="computer-content">
                                <span class="screen-file"><Icon class="icon" name="archive" /></span
                                ><span class="screen-line" /><span class="screen-line short" />
                            </div>
                        </div>
                        <div class="computer-base" />
                        <div class="device-name">This computer<span>Sending</span></div>
                    </div>
                    <div class="beam-path">
                        <div class="beam-line" />
                        <div class="beam-packet"><Icon class="icon" name="lock" /></div>
                        <div class="connection-caption">Encrypted P2P</div>
                    </div>
                    <div class="device-target-card" :data-target="target">
                        <div class="device-target-head">
                            <span>{{ device.label }}</span
                            ><span class="connection-dot" />
                        </div>
                        <div class="device-target-body">
                            <div class="target-device-glyph">
                                <Icon class="icon" :name="device.icon" />
                            </div>
                            <strong>{{ device.label }}</strong>
                            <small>Paired &amp; reachable</small>
                            <div class="dropzone-tray">
                                <Icon class="icon" name="folder" /><span>Dropzone</span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
        <div class="device-status-line">
            <span v-if="enabled" role="status"
                ><Icon class="icon" name="check" /> {{ device.label }} is paired with this computer.
                Transfers land in its Dropzone.</span
            >
            <span v-else
                ><Icon class="icon" name="info" /> Device to device isn’t enabled on this
                server.</span
            >
            <span class="device-preview-note">Choose a destination to preview it.</span>
        </div>
    </section>
</template>
