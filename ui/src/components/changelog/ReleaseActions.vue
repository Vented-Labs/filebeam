<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue';
import Icon from '../primitives/Icon.vue';
import { copyText } from '../../lib/copy-text';
import type { ChangelogRelease } from '../../types/changelog';

const props = defineProps<{ release: ChangelogRelease }>();
const emit = defineEmits<{ copied: [message: string] }>();
const copied = ref(false);
let timer: number | undefined;

async function copy(): Promise<void> {
    // The server provides the canonical URL on this instance.
    const ok = await copyText(props.release.url);
    window.clearTimeout(timer);
    copied.value = ok;
    emit(
        'copied',
        ok
            ? `Link to ${props.release.tag} copied.`
            : 'Clipboard unavailable. Copy the page address from your browser.',
    );
    timer = window.setTimeout(() => (copied.value = false), 1900);
}

onBeforeUnmount(() => window.clearTimeout(timer));
</script>

<template>
    <div class="cl-actions">
        <span v-if="release.author" class="cl-contributor"
            ><Icon name="user" :size="13" /><a
                :href="`https://github.com/${release.author.replace(/\[bot\]$/, '')}`"
                target="_blank"
                rel="noopener noreferrer"
                >@{{ release.author }}</a
            ></span
        >
        <div class="cl-action-links">
            <button type="button" :aria-label="`Copy link to ${release.tag}`" @click="copy">
                <Icon :name="copied ? 'check' : 'link'" :size="13" />{{
                    copied ? 'Copied' : 'Copy link'
                }}
            </button>
            <a :href="release.html_url" target="_blank" rel="noopener noreferrer"
                >View on GitHub <Icon name="arrow-up-right" :size="13"
            /></a>
        </div>
    </div>
</template>
