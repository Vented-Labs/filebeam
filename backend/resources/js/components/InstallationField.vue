<script setup lang="ts">
import { FormField, Input } from '@filebeam/ui';
import { computed, useAttrs } from 'vue';

defineOptions({ inheritAttrs: false });

const props = withDefaults(
    defineProps<{
        path: string;
        label: string;
        error?: string;
        description?: string;
        type?: string;
    }>(),
    { type: 'text' },
);

const model = defineModel<string | number | boolean>();
const id = computed(() => `install-${props.path.replaceAll('.', '-')}`);
const attrs = useAttrs();
function controlAttrs(): Record<string, unknown> {
    const { class: _, ...nativeAttrs } = attrs;

    return nativeAttrs;
}

function updateValue(value: string): void {
    const numericValue = Number(value);

    model.value =
        props.type === 'number' && value !== '' && Number.isFinite(numericValue)
            ? numericValue
            : value;
}
</script>

<template>
    <FormField
        v-if="type !== 'checkbox'"
        :id="id"
        :label="label"
        :error="error"
        :description="description"
        :class="attrs.class"
    >
        <template #default="{ id: fieldId, describedBy, invalid }">
            <select
                v-if="type === 'select'"
                v-bind="controlAttrs()"
                :id="fieldId"
                v-model="model"
                :name="path"
                class="fb-input"
                :aria-invalid="invalid || undefined"
                :aria-describedby="describedBy"
            >
                <slot />
            </select>
            <Input
                v-else
                v-bind="controlAttrs()"
                :id="fieldId"
                :name="path"
                :type="type"
                :model-value="model as string | number"
                class="fb-input"
                :invalid="invalid"
                :aria-describedby="describedBy"
                @update:model-value="updateValue"
            />
        </template>
    </FormField>
    <div v-else class="fb-form-field" :class="attrs.class">
        <label class="fb-checkbox-label" :for="id">
            <input
                v-bind="controlAttrs()"
                :id="id"
                v-model="model"
                :name="path"
                type="checkbox"
                class="fb-checkbox"
                :aria-invalid="Boolean(error) || undefined"
                :aria-describedby="
                    error ? `${id}-error` : description ? `${id}-description` : undefined
                "
            />
            <span>{{ label }}</span>
        </label>
        <p v-if="description && !error" :id="`${id}-description`" class="fb-field-description">
            {{ description }}
        </p>
        <p v-if="error" :id="`${id}-error`" class="fb-field-error" role="alert">
            {{ error }}
        </p>
    </div>
</template>

<style scoped>
:deep(.fb-input[aria-invalid='true']:focus-visible) {
    outline-color: var(--fb-danger);
}

.fb-checkbox[aria-invalid='true'] {
    outline: 2px solid var(--fb-danger);
    outline-offset: 2px;
}

.fb-checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: var(--fb-font-small);
    color: var(--fb-text);
    font-weight: var(--fb-weight-medium);
}

.fb-checkbox {
    flex-shrink: 0;
    width: 1rem;
    height: 1rem;
    accent-color: var(--fb-action);
}
</style>
