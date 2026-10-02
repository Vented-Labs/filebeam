<script setup lang="ts">
import { useAppearance, type Appearance } from '../../composables/useAppearance';
import { useId } from 'vue';

const { preference, set } = useAppearance();
const id = useId();
</script>

<template>
    <div class="fb-appearance">
        <label :for="id" class="fb-field-label">Appearance</label>
        <select
            :id="id"
            class="fb-select-trigger"
            :value="preference"
            @change="set(($event.target as HTMLSelectElement).value as Appearance)"
        >
            <option value="system">Use system setting</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
        </select>
    </div>
</template>

<style scoped>
.fb-appearance {
    display: grid;
    gap: 0.5rem;
    margin-top: 1.5rem;
}
</style>
