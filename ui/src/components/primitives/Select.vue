<script setup lang="ts">
import { computed, useAttrs } from 'vue';
import {
    SelectRoot,
    SelectTrigger,
    SelectValue,
    SelectPortal,
    SelectContent,
    SelectViewport,
    SelectItem,
    SelectItemText,
    SelectItemIndicator,
} from 'reka-ui';
import Icon, { type IconName } from './Icon.vue';
defineOptions({ inheritAttrs: false });
const props = defineProps<{
    label?: string;
    options: Array<{ value: string; label: string }>;
    disabled?: boolean;
    icon?: IconName;
    placeholder?: string;
    describedBy?: string;
    invalid?: boolean;
}>();
const value = defineModel<string>({ default: '' });
const emit = defineEmits<{ change: [] }>();
const attrs = useAttrs();
const triggerAttrs = computed(() => ({
    ...attrs,
    'aria-label': props.label ?? attrs['aria-label'],
    'aria-describedby': props.describedBy ?? attrs['aria-describedby'],
    'aria-invalid': props.invalid || attrs['aria-invalid'] || undefined,
}));
const selectedLabel = computed(
    () => props.options.find((option) => option.value === value.value)?.label ?? props.placeholder,
);
function update(next: unknown): void {
    value.value = next === '__all__' ? '' : String(next);
    emit('change');
}
</script>
<template>
    <SelectRoot :model-value="value || '__all__'" :disabled="disabled" @update:model-value="update">
        <SelectTrigger v-bind="triggerAttrs" class="fb-select-trigger account-select"
            ><Icon v-if="icon" :name="icon" :size="16" /><SelectValue
                class="account-select__value"
                >{{ selectedLabel }}</SelectValue
            ><Icon name="chevron-down" :size="15"
        /></SelectTrigger>
        <SelectPortal
            ><SelectContent
                :body-lock="false"
                position="popper"
                :side-offset="8"
                :collision-padding="8"
                class="fb-select-content"
                ><SelectViewport
                    ><SelectItem
                        v-for="option in options"
                        :key="option.value"
                        :value="option.value || '__all__'"
                        class="fb-select-item"
                        ><SelectItemText>{{ option.label }}</SelectItemText
                        ><SelectItemIndicator
                            ><Icon
                                name="check"
                                :size="
                                    15
                                " /></SelectItemIndicator></SelectItem></SelectViewport></SelectContent
        ></SelectPortal>
    </SelectRoot>
</template>
<style scoped>
.account-select {
    width: 100%;
    min-width: 0;
    gap: 0.625rem;
}
.account-select__value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
    white-space: nowrap;
}
.fb-select-content {
    max-width: calc(100vw - 1rem);
}
</style>
