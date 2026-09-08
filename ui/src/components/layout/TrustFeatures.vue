<script lang="ts" setup>
import FilebeamIcon, { type FilebeamIconName } from '../primitives/FilebeamIcon.vue';

const features: Array<{ icon: FilebeamIconName; title: string; description: string }> = [
    {
        icon: 'lock',
        title: 'End-to-end encrypted',
        description: 'Files are encrypted in your browser before they leave your device.',
    },
    { icon: 'bolt', title: 'Fast and simple', description: 'Share files and notes in seconds.' },
    {
        icon: 'eye-off',
        title: 'Private by design',
        description: 'Encryption keys stay with the people you share with.',
    },
    {
        icon: 'heart',
        title: 'Open Source',
        description: "MIT-licensed, so you're free to self-host or customize",
    },
];
</script>

<template>
    <section aria-label="Sharing features" class="fb-trust-features">
        <article v-for="feature in features" :key="feature.title" class="fb-trust-feature">
            <FilebeamIcon :name="feature.icon" :size="30" />
            <div>
                <h2>{{ feature.title }}</h2>
                <p>{{ feature.description }}</p>
            </div>
        </article>
    </section>
</template>
