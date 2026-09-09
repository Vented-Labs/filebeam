<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
    DialogTrigger,
} from 'reka-ui';
import { nextTick, ref } from 'vue';
import Button from '../primitives/Button.vue';
import CopyButton from '../primitives/CopyButton.vue';
import Icon from '../primitives/Icon.vue';

const installCommand = 'curl -fsSL https://releases.filebeam.io/cli/install.sh | sh';
const customInstallCommand = `${installCommand} -s -- --dir "$HOME/apps/filebeam"`;
const examples = ['beam', 'beam up ./file.zip', 'beam down <link>'];
const open = ref(false);
const trigger = ref<HTMLElement | null>(null);

function rememberTrigger(event: MouseEvent): void {
    trigger.value = event.currentTarget as HTMLElement;
}

function restoreTriggerFocus(event: Event): void {
    event.preventDefault();
    nextTick(() => trigger.value?.focus());
}
</script>

<template>
    <DialogRoot v-model:open="open">
        <DialogTrigger as-child>
            <Button variant="ghost" class="cli-install-trigger" @click="rememberTrigger"
                ><Icon name="code" :size="16" />Get the CLI</Button
            >
        </DialogTrigger>
        <DialogPortal>
            <DialogOverlay class="fb-dialog__overlay" />
            <DialogContent
                class="fb-dialog__content cli-install-dialog"
                @close-auto-focus="restoreTriggerFocus"
            >
                <DialogTitle class="fb-dialog__title">Filebeam CLI</DialogTitle>
                <DialogDescription class="fb-dialog__description">
                    Send and receive from your terminal. The installer supports Linux x86_64 and
                    ARM64.
                </DialogDescription>

                <section class="cli-install-dialog__section" aria-labelledby="cli-install-heading">
                    <h2 id="cli-install-heading" class="cli-install-dialog__heading">Install</h2>
                    <div class="cli-install-dialog__command">
                        <code class="fb-code">{{ installCommand }}</code>
                        <CopyButton :value="installCommand" label="Copy install command" />
                    </div>
                </section>

                <section class="cli-install-dialog__section" aria-labelledby="cli-custom-heading">
                    <h2 id="cli-custom-heading" class="cli-install-dialog__heading">
                        Install to a custom directory
                    </h2>
                    <div class="cli-install-dialog__command">
                        <code class="fb-code">{{ customInstallCommand }}</code>
                        <CopyButton
                            :value="customInstallCommand"
                            label="Copy custom install command"
                            variant="secondary"
                        />
                    </div>
                </section>

                <section class="cli-install-dialog__section" aria-labelledby="cli-examples-heading">
                    <h2 id="cli-examples-heading" class="cli-install-dialog__heading">
                        Quick commands
                    </h2>
                    <ul class="cli-install-dialog__examples">
                        <li v-for="example in examples" :key="example">
                            <code class="fb-code">{{ example }}</code>
                            <CopyButton :value="example" :label="`Copy ${example}`" icon-only />
                        </li>
                    </ul>
                </section>

                <DialogClose class="fb-dialog__close" aria-label="Close Filebeam CLI dialog">
                    <Icon name="x" :size="18" />
                </DialogClose>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.cli-install-trigger {
    color: var(--fb-text-muted);
}
.cli-install-dialog {
    width: min(calc(100vw - 2rem), 42rem);
    max-height: min(44rem, calc(100svh - 2rem));
    overflow-y: auto;
}
.cli-install-dialog__section {
    margin-top: 1.5rem;
}
.cli-install-dialog__heading {
    margin: 0 0 0.55rem;
    font-size: 0.82rem;
    font-weight: 650;
    color: var(--fb-text-muted);
}
.cli-install-dialog__command,
.cli-install-dialog__examples li {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.65rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.65rem;
    padding: 0.55rem;
    background: var(--fb-surface);
}
.cli-install-dialog__command code {
    min-width: 0;
    flex: 1;
    overflow-x: auto;
    white-space: nowrap;
    color: var(--fb-accent-text);
}
.cli-install-dialog__examples {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
}
.cli-install-dialog__examples li {
    justify-content: space-between;
}
.cli-install-dialog__examples code {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
@media (max-width: 520px) {
    .cli-install-dialog__examples {
        grid-template-columns: 1fr;
    }
}
</style>
