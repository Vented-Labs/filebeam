<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { FilebeamHome, RouteSurface, type FilebeamConfig } from '@filebeam/ui';

defineOptions({ layout: RouteSurface });

defineProps<{
    filebeam: FilebeamConfig;
    auth: { user: { name: string; username?: string | null } | null };
}>();
</script>

<template>
    <div>
        <Head title="Secure sharing" />
        <FilebeamHome :config="filebeam" :user="auth.user" />
    </div>
</template>
