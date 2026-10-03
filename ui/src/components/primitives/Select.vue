<script setup lang="ts">
import {
    SelectContent,
    SelectItem,
    SelectItemIndicator,
    SelectItemText,
    SelectPortal,
    SelectRoot,
    SelectTrigger,
    SelectValue,
    SelectViewport,
} from 'reka-ui';
import Icon from './Icon.vue';

defineProps<{
    label: string;
    options: Array<{ value: string; label: string }>;
    disabled?: boolean;
}>();
const value = defineModel<string>({ default: '' });
const emit = defineEmits<{ change: [] }>();
function select(next: unknown): void {
    value.value = next === '__all__' ? '' : String(next);
    emit('change');
}
</script>

<template>
    <SelectRoot :model-value="value || '__all__'" :disabled="disabled" @update:model-value="select">
        <SelectTrigger :aria-label="label" class="fb-select-trigger">
            <SelectValue class="fb-select-value" />
            <Icon name="chevron-down" :size="15" class="ml-auto" />
        </SelectTrigger>
        <SelectPortal>
            <SelectContent
                position="popper"
                :body-lock="false"
                :side-offset="8"
                :collision-padding="8"
                class="fb-select-content"
            >
                <SelectViewport>
                    <SelectItem
                        v-for="option in options"
                        :key="option.value"
                        :value="option.value || '__all__'"
                        class="fb-select-item"
                    >
                        <SelectItemText>{{ option.label }}</SelectItemText>
                        <SelectItemIndicator><Icon name="check" :size="15" /></SelectItemIndicator>
                    </SelectItem>
                </SelectViewport>
            </SelectContent>
        </SelectPortal>
    </SelectRoot>
</template>

<style scoped>
.fb-select-value {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
}
</style>
