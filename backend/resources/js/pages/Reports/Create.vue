<script setup lang="ts">
import { Head, useForm } from '@inertiajs/vue3';
import { RouteSurface } from '@filebeam/ui';
import FileReportController from '@/actions/App/Http/Controllers/FileReportController';

defineOptions({ layout: RouteSurface });

const props = defineProps<{
    created: boolean;
    transferId: string | null;
}>();

const form = useForm({
    transfer_id: props.transferId ?? '',
    category: '',
    description: '',
    reporter_email: '',
    website: '',
});

const submit = (): void => {
    form.post(FileReportController.store().url);
};
</script>

<template>
    <section class="mx-auto w-full max-w-2xl px-4 py-12 sm:px-6">
        <Head title="Report a transfer" />

        <div
            class="rounded-2xl border border-violet-400/20 bg-[#100d25]/90 p-6 shadow-2xl shadow-black/20 sm:p-8"
        >
            <template v-if="created">
                <h1 class="text-2xl font-semibold text-white">Thank you</h1>
                <p class="mt-3 leading-6 text-violet-100/70">
                    Your report has been received and will be reviewed.
                </p>
            </template>

            <template v-else>
                <h1 class="text-2xl font-semibold text-white">Report a transfer</h1>
                <p class="mt-3 leading-6 text-violet-100/70">
                    Provide the transfer identifier and details that help us review the report. Do
                    not send decryption keys, passwords, or attachments.
                </p>

                <form class="mt-7 space-y-5" @submit.prevent="submit">
                    <label class="block text-sm font-medium text-violet-100" for="transfer_id">
                        Transfer identifier
                        <input
                            id="transfer_id"
                            v-model="form.transfer_id"
                            class="fb-input mt-2"
                            autocomplete="off"
                            required
                        />
                        <span
                            v-if="form.errors.transfer_id"
                            class="mt-1 block text-sm text-rose-300"
                            >{{ form.errors.transfer_id }}</span
                        >
                    </label>

                    <label class="block text-sm font-medium text-violet-100" for="category">
                        Category
                        <select
                            id="category"
                            v-model="form.category"
                            class="fb-input mt-2"
                            required
                        >
                            <option disabled value="">Select a category</option>
                            <option value="spam">Spam</option>
                            <option value="malware">Malware</option>
                            <option value="illegal_content">Illegal content</option>
                            <option value="privacy">Privacy</option>
                            <option value="copyright">Copyright</option>
                            <option value="other">Other</option>
                        </select>
                        <span
                            v-if="form.errors.category"
                            class="mt-1 block text-sm text-rose-300"
                            >{{ form.errors.category }}</span
                        >
                    </label>

                    <label class="block text-sm font-medium text-violet-100" for="description">
                        Description
                        <textarea
                            id="description"
                            v-model="form.description"
                            class="fb-input mt-2 min-h-36 resize-y"
                            maxlength="2000"
                            required
                        />
                        <span
                            v-if="form.errors.description"
                            class="mt-1 block text-sm text-rose-300"
                            >{{ form.errors.description }}</span
                        >
                    </label>

                    <label class="block text-sm font-medium text-violet-100" for="reporter_email">
                        Email address
                        <span class="font-normal text-violet-200/60">(optional)</span>
                        <input
                            id="reporter_email"
                            v-model="form.reporter_email"
                            class="fb-input mt-2"
                            type="email"
                            autocomplete="email"
                        />
                        <span
                            v-if="form.errors.reporter_email"
                            class="mt-1 block text-sm text-rose-300"
                            >{{ form.errors.reporter_email }}</span
                        >
                    </label>

                    <input
                        v-model="form.website"
                        class="sr-only"
                        tabindex="-1"
                        autocomplete="off"
                        aria-hidden="true"
                    />

                    <button
                        class="fb-button fb-button--danger"
                        type="submit"
                        :disabled="form.processing"
                    >
                        Submit report
                    </button>
                </form>
            </template>
        </div>
    </section>
</template>
