<script setup lang="ts">
import { Primitive } from 'reka-ui';

withDefaults(
    defineProps<{
        as?: 'button' | 'a';
        variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
        disabled?: boolean;
        type?: 'button' | 'submit' | 'reset';
        icon?: boolean;
        size?: 'default' | 'large';
    }>(),
    {
        as: 'button',
        variant: 'primary',
        disabled: false,
        type: 'button',
        icon: false,
        size: 'default',
    },
);
</script>

<template>
    <Primitive
        :as="as"
        class="fb-button"
        :class="[
            `fb-button--${variant}`,
            { 'fb-button--icon': icon, 'fb-button--large': size === 'large' },
        ]"
        :disabled="as === 'button' ? disabled : undefined"
        :aria-disabled="as === 'a' && disabled ? 'true' : undefined"
        :tabindex="as === 'a' && disabled ? -1 : undefined"
        :type="as === 'button' ? type : undefined"
        @click="disabled && $event.preventDefault()"
    >
        <slot />
    </Primitive>
</template>
