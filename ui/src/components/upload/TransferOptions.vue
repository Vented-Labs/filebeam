<script setup lang="ts">
import { computed } from 'vue';
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
import Switch from '../primitives/Switch.vue';
import Tooltip from '../primitives/Tooltip.vue';
import TransferPasswordPopover from './TransferPasswordPopover.vue';
import type { TransferDriver } from '../../types';
import AnimatedReveal from '../layout/AnimatedReveal.vue';

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
const driver = defineModel<TransferDriver>('driver', { required: true });
const emit = defineEmits<{ submit: []; turbo: []; cancel: [] }>();
const passwordInvalid = computed(
    () => password.value.length > 0 && Array.from(password.value).length < 8,
);

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
    <section class="transfer-options" data-testid="prism-settings" aria-label="Transfer settings">
        <div
            class="transfer-options__grid"
            :class="{ 'transfer-options__grid--recipient': recipient }"
            data-testid="prism-settings-controls"
            :inert="disabled || undefined"
        >
            <div v-if="!recipient" class="transfer-options__field">
                <label id="password-setting-label"> Password <span>optional</span> </label>
                <TransferPasswordPopover
                    v-model="password"
                    :disabled="disabled"
                    :invalid="passwordInvalid"
                    labelled-by="password-setting-label"
                />
            </div>

            <div class="transfer-options__field">
                <label id="retention-setting-label">
                    {{ driver === 'webrtc' ? 'Link lifetime' : 'Retained for' }}
                </label>
                <SelectRoot
                    :model-value="String(retentionHours)"
                    :disabled="disabled"
                    @update:model-value="setRetention"
                >
                    <SelectTrigger
                        aria-label="Retention period"
                        class="fb-select-trigger transfer-options__trigger"
                    >
                        <Icon name="clock" :size="15" />
                        <SelectValue class="transfer-options__value">{{
                            retentionLabel(retentionHours)
                        }}</SelectValue>
                        <Icon name="chevron-down" :size="15" />
                    </SelectTrigger>
                    <SelectPortal>
                        <SelectContent
                            :body-lock="false"
                            position="popper"
                            :side-offset="9"
                            :collision-padding="8"
                            class="fb-select-content retention-menu"
                        >
                            <p class="retention-menu__label">
                                {{ driver === 'webrtc' ? 'Link lifetime' : 'Retained for' }}
                            </p>
                            <SelectViewport>
                                <SelectItem
                                    v-for="hours in [
                                        ...new Set([...props.retentionOptions, retentionHours]),
                                    ].sort((a, b) => a - b)"
                                    :key="hours"
                                    :value="String(hours)"
                                    class="fb-select-item"
                                >
                                    <Icon name="clock" :size="14" />
                                    <SelectItemText class="transfer-options__value">{{
                                        retentionLabel(hours)
                                    }}</SelectItemText>
                                    <SelectItemIndicator>
                                        <Icon name="check" :size="15" />
                                    </SelectItemIndicator>
                                </SelectItem>
                            </SelectViewport>
                            <p v-if="driver === 'webrtc'" class="retention-menu__footer">
                                The server may shorten this lifetime.
                            </p>
                        </SelectContent>
                    </SelectPortal>
                </SelectRoot>
                <AnimatedReveal :show="driver === 'webrtc'">
                    <p class="transfer-options__help">The server may shorten this lifetime.</p>
                </AnimatedReveal>
            </div>

            <div v-if="!recipient" class="transfer-options__sharing">
                <p class="transfer-options__label">Sharing preferences</p>
                <label class="transfer-options__preference">
                    <span>Include key in link</span>
                    <Switch
                        v-model="includeKey"
                        :disabled="disabled"
                        aria-label="Include key in link"
                    />
                </label>
                <AnimatedReveal :show="mode === 'note'">
                    <label class="transfer-options__preference">
                        <span>
                            Burn on read
                            <small>{{
                                driver === 'webrtc'
                                    ? 'Revokes after the first successful decrypt.'
                                    : 'Removed after the first successful decrypt.'
                            }}</small>
                        </span>
                        <Switch
                            v-model="burnOnRead"
                            :disabled="disabled"
                            aria-label="Burn on read"
                        />
                    </label>
                </AnimatedReveal>
            </div>
        </div>

        <AnimatedReveal :show="!recipient && !includeKey">
            <div class="transfer-options__key-hint" role="status">
                <Icon name="key" :size="14" />Share the decryption key separately.
            </div>
        </AnimatedReveal>

        <div class="transfer-options__footer">
            <p><Icon name="lock" :size="14" />Encrypted in your browser before upload.</p>
            <div class="transfer-options__actions">
                <Button
                    v-if="uploading"
                    variant="secondary"
                    class="transfer-options__action"
                    @click="emit('cancel')"
                >
                    Cancel
                </Button>
                <template v-else>
                    <Tooltip
                        v-if="mode === 'files' && !recipient && driver === 'http'"
                        content="Share the link while files are still uploading."
                    >
                        <Button
                            variant="secondary"
                            class="transfer-options__action transfer-options__action--turbo"
                            :disabled="!canUpload"
                            @click="emit('turbo')"
                        >
                            <Icon name="bolt" :size="16" />Turbo Transfer
                        </Button>
                    </Tooltip>
                    <Button
                        class="transfer-options__action"
                        :disabled="!canUpload"
                        :aria-busy="uploading || undefined"
                        @click="emit('submit')"
                    >
                        <Icon name="lock" :size="16" />
                        {{ recipient ? 'Encrypt and send' : 'Encrypt and share' }}
                        <Icon name="arrow-right" :size="16" />
                    </Button>
                </template>
            </div>
        </div>
    </section>
</template>

<style scoped>
.transfer-options {
    border-top: 1px solid #ffffff08;
    background: var(--fb-settings-surface);
}
.transfer-options__grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1.12fr);
    gap: 1.5rem;
    align-items: start;
    padding: 1.25rem 1.5rem 1.125rem;
}
.transfer-options__grid--recipient {
    grid-template-columns: minmax(0, 1fr);
}
.transfer-options__field,
.transfer-options__sharing {
    min-width: 0;
}
.transfer-options__field > label,
.transfer-options__label {
    display: flex;
    min-height: 1rem;
    align-items: baseline;
    gap: 0.25rem;
    margin: 0 0 0.5rem;
    color: #d8cee4;
    font-size: 0.75rem;
    font-weight: 500;
    line-height: 1rem;
}
.transfer-options__field > label span {
    color: var(--fb-text-subtle);
    font-weight: 400;
}
.transfer-options__trigger {
    height: 2.625rem;
    min-height: 2.625rem;
    gap: 0.5625rem;
    padding-inline: 0.6875rem;
    border-color: #44374f;
    font-size: 0.8125rem;
}
.transfer-options__value {
    min-width: 0;
    flex: 1;
}
.transfer-options__help {
    margin: 0.5rem 0 0;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
    line-height: 1.5;
}
.transfer-options__sharing {
    display: flex;
    flex-direction: column;
}
.transfer-options__preference {
    display: flex;
    min-height: 2.125rem;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    color: #d3c9df;
    font-size: 0.75rem;
    cursor: pointer;
}
.transfer-options__preference small {
    display: block;
    margin-top: 0.125rem;
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
    line-height: 1.4;
}
.transfer-options__key-hint {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: -0.0625rem 1.5rem 1.125rem;
    padding: 0.625rem 0.75rem;
    border: 1px solid #ffffff08;
    border-radius: 0.625rem;
    color: var(--fb-text-muted);
    background: #ffffff03;
    font-size: 0.75rem;
}
.transfer-options__footer {
    display: flex;
    min-height: 4.875rem;
    box-sizing: border-box;
    align-items: center;
    justify-content: space-between;
    gap: 1.125rem;
    padding: 1.0625rem 1.5rem;
    border-top: 1px solid #ffffff08;
    border-radius: 0 0 var(--fb-radius-panel) var(--fb-radius-panel);
    background: var(--fb-footer-surface);
}
.transfer-options__footer > p {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.transfer-options__footer > p :deep(.fb-icon) {
    color: var(--fb-text-subtle);
}
.transfer-options__actions {
    display: flex;
    min-width: 0;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
}
.transfer-options__action {
    min-width: 12.375rem;
    min-height: 2.6875rem;
}
.transfer-options__action--turbo {
    min-width: 10rem;
}
.retention-menu {
    min-width: 12.8125rem;
}
.retention-menu__label,
.retention-menu__footer {
    margin: 0;
    padding: 0.5rem 0.625rem 0.625rem;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
}
.retention-menu__footer {
    padding-top: 0.625rem;
    border-top: 1px solid #ffffff08;
}
@media (max-width: 780px) {
    .transfer-options__grid {
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 1.125rem 1.25rem;
    }
    .transfer-options__sharing {
        grid-column: 1 / -1;
    }
}
@media (max-width: 730px) {
    .transfer-options__grid {
        gap: 1.0625rem 0.875rem;
        padding: 1.125rem 1.0625rem;
    }
    .transfer-options__field > label,
    .transfer-options__label {
        font-size: 0.6875rem;
    }
    .transfer-options__trigger {
        padding-inline: 0.5625rem;
        font-size: 0.75rem;
    }
    .transfer-options__footer {
        align-items: stretch;
        flex-direction: column;
        padding: 1rem 1.0625rem 1.0625rem;
    }
    .transfer-options__footer > p {
        justify-content: center;
        font-size: 0.6875rem;
    }
    .transfer-options__actions,
    .transfer-options__action {
        width: 100%;
    }
    .transfer-options__actions {
        flex-direction: column-reverse;
    }
}
@media (max-width: 380px) {
    .transfer-options__grid {
        gap: 1rem 0.6875rem;
    }
    .transfer-options__trigger,
    :deep(.password-trigger) {
        font-size: 0.6875rem;
    }
}
</style>
