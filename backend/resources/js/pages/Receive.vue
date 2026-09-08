<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { FilebeamHome, RouteSurface, type FilebeamConfig } from '@filebeam/ui';
import type { PublicRecipient } from '../../../../ui/src/types';

defineOptions({ layout: RouteSurface });
defineProps<{
    recipient: PublicRecipient;
    filebeam: FilebeamConfig;
    auth: { user: { name: string; username?: string | null } | null };
}>();
</script>

<template>
    <div>
        <Head :title="`Send files to @${recipient.username}`" />
        <FilebeamHome :config="filebeam" :user="auth.user" :recipient="recipient" />
    </div>
</template>
