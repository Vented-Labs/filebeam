<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { FilebeamTransfer, RouteSurface, type FilebeamConfig } from '@filebeam/ui';

defineOptions({ layout: RouteSurface });
defineProps<{
    transferId: string;
    filebeam: FilebeamConfig;
    auth: { user: { name: string; username?: string | null } };
}>();
</script>

<template>
    <div>
        <Head title="Received files" />
        <FilebeamTransfer :transfer-id="transferId" :config="filebeam" :user="auth.user" inbox />
    </div>
</template>
