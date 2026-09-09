<script setup lang="ts">
import { router } from '@inertiajs/vue3';
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import { inject, ref } from 'vue';
import BrandLogo from '../brand/BrandLogo.vue';
import Icon from '../primitives/Icon.vue';
import AnimatedHeight from '../layout/AnimatedHeight.vue';

defineProps<{ title: string; description: string }>();

const open = ref(true);
const returnUrl = inject<() => string>('authReturnUrl', () => '/');

function restoreHomeFocus(): void {
    document.querySelector<HTMLElement>('.fb-header__brand')?.focus();
}

function focusFirstField(): void {
    document
        .querySelector<HTMLElement>('.auth-drawer input:not([disabled])')
        ?.focus({ preventScroll: true });
}

function closeAutoFocus(event: Event): void {
    event.preventDefault();
    if (!open.value) {
        router.visit(returnUrl(), {
            preserveScroll: true,
            viewTransition: false,
            onFinish: restoreHomeFocus,
        });
    }
}
</script>

<template>
    <DialogRoot v-model:open="open">
        <DialogPortal>
            <DialogOverlay class="auth-drawer__overlay" />
            <DialogContent class="auth-drawer" @close-auto-focus="closeAutoFocus">
                <header class="auth-drawer__header">
                    <BrandLogo />
                    <DialogClose class="auth-drawer__close" aria-label="Close authentication"
                        ><Icon name="x" :size="19"
                    /></DialogClose>
                </header>
                <div class="auth-drawer__body">
                    <AnimatedHeight class="focus-safe-height">
                        <Transition
                            name="auth-drawer__mode"
                            mode="out-in"
                            @after-enter="focusFirstField"
                        >
                            <div :key="title">
                                <DialogTitle class="auth-drawer__title">{{ title }}</DialogTitle>
                                <DialogDescription class="auth-drawer__description">{{
                                    description
                                }}</DialogDescription>
                                <slot />
                            </div>
                        </Transition>
                    </AnimatedHeight>
                </div>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>
