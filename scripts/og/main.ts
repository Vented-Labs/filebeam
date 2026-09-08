import { createApp } from 'vue';
import '@fontsource-variable/inter';
import '../../ui/src/filebeam-tokens.css';
import OgCard from '../../ui/src/components/brand/OgCard.vue';
import './preview.css';

const variant = new URLSearchParams(window.location.search).get('variant') ?? 'home';
if (variant !== 'home' && variant !== 'receive' && variant !== 'transfer') {
    throw new Error(`Unknown social card variant: ${variant}`);
}

createApp(OgCard, { variant }).mount('#app');
