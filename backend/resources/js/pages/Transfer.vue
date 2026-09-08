<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { reactive } from 'vue';
import { FilebeamTransfer, RouteSurface, type FilebeamConfig } from '@filebeam/ui';
import FileReportController from '@/actions/App/Http/Controllers/FileReportController';
import ReportDialog from '../../../../ui/src/components/reports/ReportDialog.vue';
import { csrfHeaders } from '../../../../ui/src/lib/csrf';

defineOptions({ layout: RouteSurface });

defineProps<{
    transferId: string;
    filebeam: FilebeamConfig;
    auth: { user: { name: string; username?: string | null } | null };
}>();

type ReportFields = {
    category: string;
    description: string;
    reporter_email: string;
    website: string;
};

const report = reactive({
    errors: {} as Partial<Record<keyof ReportFields, string>>,
    processing: false,
    successful: false,
});

async function submitReport(transferId: string, fields: ReportFields): Promise<void> {
    if (report.processing) return;
    report.processing = true;
    report.errors = {};

    try {
        const response = await fetch(FileReportController.store().url, {
            method: 'POST',
            credentials: 'same-origin',
            headers: csrfHeaders(true),
            body: JSON.stringify({
                transfer_id: transferId,
                category: fields.category,
                description: fields.description,
                reporter_email: fields.reporter_email,
                website: fields.website,
            }),
        });

        if (response.status === 422) {
            const body = (await response.json()) as {
                errors?: Partial<Record<keyof ReportFields, string[]>>;
            };

            report.errors = Object.fromEntries(
                Object.entries(body.errors ?? {}).map(([field, messages]) => [field, messages[0]]),
            );

            return;
        }

        if (response.status === 202) {
            report.successful = true;
        } else {
            report.errors.description = 'Unable to submit your report. Please try again.';
        }
    } catch {
        report.errors.description = 'Unable to submit your report. Please try again.';
    } finally {
        report.processing = false;
    }
}
</script>

<template>
    <div>
        <Head title="Encrypted transfer" />
        <FilebeamTransfer :transfer-id="transferId" :config="filebeam" :user="auth.user">
            <template #report="{ transferId: reportTransferId }">
                <ReportDialog
                    :transfer-id="reportTransferId"
                    :action-uri="FileReportController.store().url"
                    :errors="report.errors"
                    :processing="report.processing"
                    :successful="report.successful"
                    @submit="submitReport(reportTransferId, $event)"
                />
            </template>
        </FilebeamTransfer>
    </div>
</template>
