<script setup lang="ts">
import { computed, ref } from 'vue';
import BrandLogo from '../brand/BrandLogo.vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';
import { onTabKeydown } from './tabs';

const props = defineProps<{ host: string }>();
const emit = defineEmits<{ explore: [] }>();

// An explanation of the flow, not an encryption implementation.
const steps = [
    {
        label: 'Choose',
        title: 'Ready to send',
        subtitle: '2 files · 16.4 MB',
        foot: 'Encryption happens here.',
        action: 'Encrypt',
    },
    {
        label: 'Encrypt',
        title: 'Encrypted on your device',
        subtitle: 'Ciphertext illustration',
        foot: 'The server gets ciphertext.',
        action: 'Share',
    },
    {
        label: 'Share',
        title: 'Ready to share',
        subtitle: 'Example link · not a real transfer',
        foot: 'Share the key with care.',
        action: 'Start over',
    },
] as const;
const step = ref(0);
const current = computed(() => steps[step.value]!);
const status = ref('');

function select(index: number): void {
    step.value = index;
    status.value = `Illustration: ${steps[index]!.title}. ${steps[index]!.foot}`;
}
</script>

<template>
    <section id="encryption" class="hero" aria-labelledby="hero-title">
        <div class="hero-copy">
            <h1 id="hero-title">End-to-end<br /><span class="accent-word">encrypted.</span></h1>
            <p class="lead">
                Files and notes, encrypted on your device.<br class="desktop-only" />
                Share a link, send to a friend, or beam to another computer.
            </p>
            <div class="hero-actions">
                <AppLink href="/" class="button"
                    >Send files <Icon class="icon" name="arrow-right" :size="17"
                /></AppLink>
                <a class="text-link" href="#explore" @click.prevent="emit('explore')"
                    >Take a look <Icon class="icon" name="arrow-down" :size="16"
                /></a>
            </div>
            <div class="hero-footnote">
                <Icon class="icon" name="lock" :size="14" /><span
                    >Encrypted before upload. Decrypted by the recipient.</span
                >
            </div>
        </div>
        <div class="hero-art" :data-step="step">
            <div class="halo halo-one" aria-hidden="true" />
            <div class="halo halo-two" aria-hidden="true" />
            <div class="back-note" aria-hidden="true">
                <div class="back-note-head">
                    <Icon class="icon" name="note" :size="14" /><span>notes.md</span>
                </div>
                <div class="note-strokes"><span /><span /><span /><span /></div>
            </div>
            <div class="back-image" aria-hidden="true">
                <Icon class="icon" name="folder" :size="18" /><span>.zip</span>
            </div>
            <div
                id="hero-panel"
                class="hero-transfer"
                role="tabpanel"
                :aria-labelledby="`hero-tab-${step}`"
            >
                <div class="transfer-top">
                    <span class="mini-mark" aria-hidden="true"><BrandLogo compact label="" /></span>
                    <span
                        >{{ current.title }}<small>{{ current.subtitle }}</small></span
                    >
                    <span class="demo-label">Demo</span>
                </div>
                <div class="hero-transfer-content" aria-hidden="true">
                    <div v-if="step === 0" class="hero-view">
                        <div class="file-row">
                            <span class="file-symbol"><Icon class="icon" name="file" /></span>
                            <div>
                                <strong>field-notes.pdf</strong><small>Document · 2.4 MB</small>
                            </div>
                            <span class="file-tick"><Icon class="icon" name="check" /></span>
                        </div>
                        <div class="file-row">
                            <span class="file-symbol"><Icon class="icon" name="archive" /></span>
                            <div>
                                <strong>project-files.zip</strong><small>Archive · 14 MB</small>
                            </div>
                            <span class="file-tick"><Icon class="icon" name="check" /></span>
                        </div>
                    </div>
                    <div v-else-if="step === 1" class="hero-view ciphertext-view">
                        <div class="cipher-lock">
                            <Icon class="icon" name="end-to-end-encrypted" />
                        </div>
                        <div class="ciphertext">
                            8c 7a f1 42 9d 03 b7 a0<br />e5 21 6f c9 44 8b d1 5e<br />0a f8 33 6c 7e
                            a4 12 9b
                        </div>
                        <span class="cipher-caption">File contents → ciphertext</span>
                    </div>
                    <div v-else class="hero-view hero-link-view">
                        <div class="link-ready-icon"><Icon class="icon" name="check" /></div>
                        <strong>Ready to share</strong>
                        <div class="demo-url">{{ props.host }}/01K8Y4VC…6D1B<span>#key</span></div>
                    </div>
                </div>
                <div class="transfer-bottom">
                    <span
                        ><Icon class="icon" name="lock" :size="13" /><span>{{
                            current.foot
                        }}</span></span
                    >
                    <button
                        type="button"
                        class="button compact"
                        :aria-label="`Illustration: ${current.action}`"
                        @click="select((step + 1) % steps.length)"
                    >
                        {{ current.action }}
                        <Icon class="icon" :name="step === 2 ? 'undo' : 'arrow-right'" />
                    </button>
                </div>
            </div>
            <div class="key-tag" aria-hidden="true">
                <Icon class="icon" name="key" />
                <div>The key stays<br /><strong>with you.</strong></div>
            </div>
            <div class="hero-step-tabs" role="tablist" aria-label="Encryption illustration">
                <template v-for="(item, index) in steps" :key="item.label">
                    <span v-if="index" class="step-divider" />
                    <button
                        :id="`hero-tab-${index}`"
                        type="button"
                        role="tab"
                        aria-controls="hero-panel"
                        :aria-selected="step === index"
                        :tabindex="step === index ? 0 : -1"
                        @click="select(index)"
                        @keydown="onTabKeydown"
                    >
                        <span>0{{ index + 1 }}</span> {{ item.label }}
                    </button>
                </template>
            </div>
            <span class="sr-only" role="status">{{ status }}</span>
        </div>
    </section>
</template>
