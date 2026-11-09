<script setup lang="ts">
import { ref } from 'vue';
import Icon from '../primitives/Icon.vue';

defineProps<{ host: string; securityUrl: string }>();
const includeKey = ref(true);
const detailsOpen = ref(false);
</script>

<template>
    <article class="feature-card privacy-card" aria-labelledby="privacy-title">
        <div class="section-icon"><Icon class="icon" name="eye-off" /></div>
        <h2 id="privacy-title">Private by design.</h2>
        <p>
            Encryption keys stay with the people<br class="desktop-only" />
            you share with.
        </p>
        <div class="key-demo">
            <div class="key-demo-label">
                <span><Icon class="icon" name="key" /> Include key in link</span>
                <button
                    type="button"
                    class="switch"
                    role="switch"
                    :aria-checked="includeKey"
                    aria-label="Include the key in the example link"
                    @click="includeKey = !includeKey"
                >
                    <span />
                </button>
            </div>
            <div class="key-link" aria-label="Example share link">
                <span class="example-domain">{{ host }}</span
                ><span>/01K7Q2…J2H</span
                ><span v-if="includeKey" class="key-fragment">#k8f3…9ad</span>
            </div>
            <div v-if="!includeKey" class="separate-key">
                <Icon class="icon" name="key" /><code>k8f3…9ad</code><small>Share separately</small>
            </div>
            <div class="key-explanation" role="status">
                {{
                    includeKey
                        ? 'Anyone with the complete link can open the transfer.'
                        : 'The recipient needs both the link and the key. Send the key separately.'
                }}
            </div>
        </div>
        <button
            type="button"
            class="text-link disclosure-button"
            :aria-expanded="detailsOpen"
            aria-controls="privacy-detail"
            @click="detailsOpen = !detailsOpen"
        >
            What the server can see <Icon class="icon" :name="detailsOpen ? 'x' : 'plus'" />
        </button>
        <div v-show="detailsOpen" id="privacy-detail" class="privacy-detail">
            Transfer identifiers, sizes, timing, expiry, and other operational data are still
            visible. End-to-end encryption protects the contents; it does not make a transfer
            anonymous.
            <a :href="securityUrl" target="_blank" rel="noopener noreferrer">Security details ↗</a>
        </div>
    </article>
</template>
