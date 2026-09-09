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
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import Input from '../primitives/Input.vue';
import Switch from '../primitives/Switch.vue';
import Tooltip from '../primitives/Tooltip.vue';

const props = defineProps<{
    disabled: boolean;
    canUpload: boolean;
    retentionOptions: number[];
    mode: 'files' | 'note';
    uploading: boolean;
    recipient?: boolean;
}>();

const password = defineModel<string>('password', { required: true });
const includeKey = defineModel<boolean>('includeKey', { required: true });
const retentionHours = defineModel<number>('retentionHours', { required: true });
const burnOnRead = defineModel<boolean>('burnOnRead', { default: false });
const emit = defineEmits<{ submit: []; turbo: []; cancel: [] }>();

const labels: Record<number, string> = {
    1: '1 hour',
    6: '6 hours',
    12: '12 hours',
    24: '1 day',
    72: '3 days',
    168: '7 days',
    720: '30 days',
};
function retentionLabel(hours: number): string {
    return labels[hours] ?? `${hours} hours`;
}
function setRetention(value: unknown): void {
    retentionHours.value = Number(value);
}
</script>

<template>
    <section
        class="transfer-options mt-5 w-full rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-5 sm:p-6"
        :class="mode === 'note' ? 'max-w-[896px]' : 'max-w-none'"
    >
        <div class="controls-grid">
            <div v-if="!recipient" class="control-field">
                <label for="transfer-password">Password <span>optional</span></label>
                <Input
                    id="transfer-password"
                    v-model="password"
                    :disabled="disabled"
                    type="password"
                    autocomplete="new-password"
                    minlength="8"
                    placeholder="At least 8 characters"
                />
            </div>

            <div class="control-field">
                <label>Retained for</label>
                <SelectRoot
                    :model-value="String(retentionHours)"
                    :disabled="disabled"
                    @update:model-value="setRetention"
                >
                    <SelectTrigger aria-label="Retention period" class="fb-select-trigger">
                        <SelectValue>{{ retentionLabel(retentionHours) }}</SelectValue>
                        <Icon name="chevron-down" :size="16" />
                    </SelectTrigger>
                    <SelectPortal>
                        <SelectContent
                            :body-lock="false"
                            position="popper"
                            class="fb-select-content"
                        >
                            <SelectViewport>
                                <SelectItem
                                    v-for="hours in [
                                        ...new Set([...props.retentionOptions, retentionHours]),
                                    ].sort((a, b) => a - b)"
                                    :key="hours"
                                    :value="String(hours)"
                                    class="fb-select-item"
                                >
                                    <SelectItemText>{{ retentionLabel(hours) }}</SelectItemText>
                                    <SelectItemIndicator
                                        ><Icon name="check" :size="15"
                                    /></SelectItemIndicator>
                                </SelectItem>
                            </SelectViewport>
                        </SelectContent>
                    </SelectPortal>
                </SelectRoot>
            </div>
        </div>

        <div v-if="!recipient" class="preferences-row">
            <p>Sharing preferences</p>
            <div class="preferences">
                <label class="preference">
                    <Switch
                        v-model="includeKey"
                        :disabled="disabled"
                        aria-label="Include key in link"
                    />
                    <span>Include key in link</span>
                </label>
                <label v-if="mode === 'note'" class="preference preference--burn">
                    <Switch v-model="burnOnRead" :disabled="disabled" aria-label="Burn on read" />
                    <span
                        >Burn on read<small
                            >Disappears from the server after the first successful decrypt.</small
                        ></span
                    >
                </label>
            </div>
        </div>

        <footer class="action-footer">
            <p><Icon name="lock" :size="15" />Encrypted in your browser before upload.</p>
            <div class="action-area">
                <Button
                    v-if="uploading"
                    variant="secondary"
                    class="action-button"
                    @click="emit('cancel')"
                    >Cancel</Button
                >
                <template v-else>
                    <div class="action-buttons">
                        <Tooltip
                            v-if="mode === 'files' && !recipient"
                            content="Share the link while files are still uploading."
                        >
                            <Button
                                variant="secondary"
                                class="action-button"
                                :disabled="!canUpload"
                                @click="emit('turbo')"
                                ><Icon name="bolt" :size="17" />Turbo Transfer</Button
                            >
                        </Tooltip>
                        <Button
                            variant="primary"
                            class="action-button"
                            :disabled="!canUpload"
                            @click="emit('submit')"
                            >{{ recipient ? 'Encrypt and send' : 'Encrypt and share' }}</Button
                        >
                    </div>
                </template>
            </div>
        </footer>
    </section>
</template>

<style scoped>
.transfer-options {
    margin-inline: auto;
}
.controls-grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
}
.control-field {
    min-width: 0;
}
.control-field > label,
.preferences-row > p {
    display: block;
    margin-bottom: 0.5rem;
    color: var(--fb-text);
    font-size: 0.875rem;
    font-weight: 500;
}
.control-field > label span {
    color: var(--fb-text-muted);
    font-weight: 400;
}
.preferences-row {
    display: flex;
    align-items: flex-start;
    gap: 1.5rem;
    margin-top: 1.25rem;
    padding-top: 1.25rem;
    border-top: 1px solid var(--fb-border);
}
.preferences-row > p {
    min-width: 9.5rem;
    margin: 0.125rem 0 0;
    color: var(--fb-text-muted);
}
.preferences {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    gap: 1.25rem 2rem;
}
.preference {
    display: flex;
    align-items: flex-start;
    gap: 0.625rem;
    color: var(--fb-text);
    font-size: 0.875rem;
    line-height: 1.25rem;
}
.preference--burn {
    max-width: 22rem;
}
.preference small {
    display: block;
    margin-top: 0.125rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1rem;
}
.action-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-top: 1.25rem;
    padding-top: 1.25rem;
    border-top: 1px solid var(--fb-border);
}
.action-footer > p {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.action-button {
    min-width: 10.5rem;
}
.action-area {
    min-width: 0;
}
.action-buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
}
@media (min-width: 1024px) {
    .controls-grid {
        grid-template-columns: minmax(0, 1.35fr) minmax(12rem, 0.65fr);
    }
}
@media (max-width: 639px) {
    .controls-grid {
        grid-template-columns: 1fr;
    }
    .preferences-row {
        display: block;
    }
    .preferences-row > p {
        margin-bottom: 0.75rem;
    }
    .preferences {
        display: grid;
        gap: 1rem;
    }
    .preference {
        width: 100%;
    }
    .action-footer {
        display: block;
    }
    .action-footer > p {
        margin-bottom: 1rem;
    }
    .action-area,
    .action-button {
        width: 100%;
    }
    .action-buttons {
        justify-content: stretch;
        flex-wrap: nowrap;
    }
    .action-buttons .action-button {
        flex: 1;
        min-width: 0;
        padding-inline: 0.5rem;
        font-size: 0.8125rem;
    }
}
</style>
