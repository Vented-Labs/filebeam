import { createApp, h, type DefineComponent } from 'vue';
import { createInertiaApp } from '@inertiajs/vue3';
import '../../backend/resources/css/app.css';
import '../../backend/resources/themes/appearance.js';
import Gallery from './Gallery.vue';
import ThemeFixture from './ThemeFixture.vue';
import './gallery.css';

if (new URLSearchParams(window.location.search).has('placement')) {
    void createInertiaApp({
        page: {
            component: 'Gallery',
            props: { errors: {}, filebeam: { main_site_url: window.location.origin } },
            url: '/?placement',
            version: null,
            clearHistory: false,
            encryptHistory: false,
            rescuedProps: [],
            flash: {},
            rememberedState: {},
        },
        resolve: () => Gallery as DefineComponent,
        setup({ el, App, props, plugin }) {
            createApp({ render: () => h(App, props) })
                .use(plugin)
                .mount(el);
        },
    });
} else if (new URLSearchParams(window.location.search).has('theme')) {
    createApp(ThemeFixture).mount('#app');
} else {
    createApp(Gallery).mount('#app');
}
