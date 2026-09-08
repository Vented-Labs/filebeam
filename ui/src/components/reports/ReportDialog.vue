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
    SelectContent,
    SelectItem,
    SelectItemText,
    SelectPortal,
    SelectRoot,
    SelectTrigger,
    SelectValue,
    SelectViewport,
} from 'reka-ui';
import { ref } from 'vue';
import Button from '../primitives/Button.vue';
import FilebeamIcon from '../primitives/FilebeamIcon.vue';
import FormField from '../primitives/FormField.vue';
import Input from '../primitives/Input.vue';
import Tooltip from '../primitives/Tooltip.vue';

type ReportFields = {
    category: string;
    description: string;
    reporter_email: string;
    website: string;
};

defineProps<{
    transferId: string;
    actionUri: string;
    errors: Partial<Record<keyof ReportFields, string>>;
    processing: boolean;
    successful: boolean;
}>();

const emit = defineEmits<{ submit: [fields: ReportFields] }>();
const open = ref(false);
const trigger = ref<InstanceType<typeof Button> | null>(null);
const fields = ref<ReportFields>({
    category: '',
    description: '',
    reporter_email: '',
    website: '',
});

function closeAutoFocus(event: Event): void {
    event.preventDefault();
    (trigger.value?.$el as HTMLElement | undefined)?.focus();
}
</script>

<template>
    <DialogRoot v-model:open="open">
        <Tooltip content="Report this transfer">
            <DialogTrigger as-child>
                <Button ref="trigger" variant="danger" icon aria-label="Report this transfer"
                    ><FilebeamIcon name="alert" :size="19"
                /></Button>
            </DialogTrigger>
        </Tooltip>
        <DialogPortal>
            <DialogOverlay class="fb-dialog__overlay" />
            <DialogContent
                class="fb-report-dialog fixed left-1/2 top-1/2 z-50 max-h-[calc(100svh-2rem)] w-[calc(100%-2rem)] max-w-xl overflow-y-auto rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-5 text-[var(--fb-text)] shadow-2xl sm:p-7"
                @close-auto-focus="closeAutoFocus"
            >
                <DialogTitle class="text-xl font-semibold">Report this transfer</DialogTitle>
                <DialogDescription class="mt-2 text-sm leading-6 text-[var(--fb-text-muted)]">
                    Do not include decryption keys, passwords, file names, or attachments.
                </DialogDescription>
                <DialogClose as-child
                    ><Button
                        variant="ghost"
                        icon
                        class="absolute right-3 top-3"
                        aria-label="Close report dialog"
                        ><FilebeamIcon name="x" :size="18" /></Button
                ></DialogClose>

                <div v-if="successful" class="mt-6 space-y-5">
                    <p class="leading-6 text-[var(--fb-text-muted)]">
                        Your report has been received and will be reviewed.
                    </p>
                    <div class="flex justify-end">
                        <DialogClose as-child><Button>Close</Button></DialogClose>
                    </div>
                </div>

                <form
                    v-else
                    class="mt-6 space-y-4"
                    :action="actionUri"
                    @submit.prevent="emit('submit', fields)"
                >
                    <FormField id="report-category" label="Category" :error="errors.category">
                        <template #default="{ id, describedBy, invalid }">
                            <SelectRoot v-model="fields.category" :disabled="processing">
                                <SelectTrigger
                                    :id="id"
                                    :aria-describedby="describedBy"
                                    :aria-invalid="invalid"
                                    class="fb-select-trigger"
                                    ><SelectValue placeholder="Select a category" /><FilebeamIcon
                                        name="chevron-down"
                                        :size="16"
                                /></SelectTrigger>
                                <SelectPortal>
                                    <SelectContent
                                        :body-lock="false"
                                        position="popper"
                                        class="fb-select-content"
                                        ><SelectViewport
                                            ><SelectItem
                                                v-for="category in [
                                                    ['spam', 'Spam'],
                                                    ['malware', 'Malware'],
                                                    ['illegal_content', 'Illegal content'],
                                                    ['privacy', 'Privacy'],
                                                    ['copyright', 'Copyright'],
                                                    ['other', 'Other'],
                                                ]"
                                                :key="category[0]"
                                                :value="category[0]"
                                                class="fb-select-item"
                                                ><SelectItemText>{{
                                                    category[1]
                                                }}</SelectItemText></SelectItem
                                            ></SelectViewport
                                        ></SelectContent
                                    >
                                </SelectPortal>
                            </SelectRoot>
                        </template>
                    </FormField>

                    <FormField
                        id="report-description"
                        label="Description"
                        :error="errors.description"
                    >
                        <template #default="{ id, describedBy, invalid }">
                            <textarea
                                :id="id"
                                v-model="fields.description"
                                :aria-describedby="describedBy"
                                :aria-invalid="invalid"
                                :disabled="processing"
                                class="fb-input min-h-32"
                                maxlength="2000"
                                required
                            />
                        </template>
                    </FormField>

                    <FormField
                        id="reporter-email"
                        label="Email address"
                        optional
                        :error="errors.reporter_email"
                    >
                        <template #default="{ id, describedBy, invalid }">
                            <Input
                                :id="id"
                                v-model="fields.reporter_email"
                                :aria-describedby="describedBy"
                                :invalid="invalid"
                                :disabled="processing"
                                type="email"
                                autocomplete="email"
                            />
                        </template>
                    </FormField>

                    <div class="sr-only" aria-hidden="true">
                        <input v-model="fields.website" tabindex="-1" autocomplete="off" />
                    </div>

                    <div class="flex justify-end">
                        <Button type="submit" :disabled="processing">
                            {{ processing ? 'Submitting...' : 'Submit report' }}
                        </Button>
                    </div>
                </form>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>
