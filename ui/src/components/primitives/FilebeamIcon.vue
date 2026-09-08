<script setup lang="ts">
export type FilebeamIconName =
    | 'upload'
    | 'folder'
    | 'file'
    | 'note'
    | 'lock'
    | 'shield'
    | 'bolt'
    | 'eye-off'
    | 'heart'
    | 'github'
    | 'arrow-right'
    | 'arrow-up-right'
    | 'plus'
    | 'x'
    | 'check'
    | 'copy'
    | 'key'
    | 'clock'
    | 'chevron-down'
    | 'settings'
    | 'trash'
    | 'download'
    | 'code'
    | 'image'
    | 'archive'
    | 'music'
    | 'video'
    | 'alert'
    | 'loader';

defineProps<{ name: FilebeamIconName; size?: number; strokeWidth?: number }>();
</script>

<template>
    <svg
        class="fb-icon"
        :class="{ 'fb-icon--loader': name === 'loader' }"
        :width="size ?? 20"
        :height="size ?? 20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        :stroke-width="strokeWidth ?? 1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
    >
        <template v-if="name === 'upload'">
            <path d="M4 17.5v2h16v-2" />
            <path d="m12 4 4.5 4.5M12 4 7.5 8.5M12 4v11" />
        </template>
        <template v-else-if="name === 'folder'"><path d="M3.5 6.5h6l2 2h9v10.5H3.5z" /></template>
        <template v-else-if="name === 'file' || name === 'note'">
            <path d="M6.5 3.5h7l4 4v13h-11z" />
            <path d="M13.5 3.5v4h4" />
            <path v-if="name === 'note'" d="M9 13h6M9 16.5h4.5" />
        </template>
        <template v-else-if="name === 'lock'">
            <rect x="5.5" y="10" width="13" height="10" rx="2" />
            <path d="M8.5 10V7a3.5 3.5 0 0 1 7 0v3" />
        </template>
        <template v-else-if="name === 'shield'">
            <path d="M12 3.5 19 6v5.3c0 4.4-2.9 7.6-7 9.2-4.1-1.6-7-4.8-7-9.2V6z" />
            <path d="m9 12 2 2 4-4" />
        </template>
        <template v-else-if="name === 'bolt'">
            <path d="m13.5 2.8-8 11h6l-1 7.4 8-11h-6z" />
        </template>
        <template v-else-if="name === 'eye-off'">
            <path d="M3 3 21 21" />
            <path
                d="M10.6 6.3A10.6 10.6 0 0 1 12 6.2c5.2 0 8.6 5.8 8.6 5.8a15.7 15.7 0 0 1-2.3 3"
            />
            <path d="M6.1 6.1C4.3 7.5 3.4 9.2 3.4 12c0 0 3.4 5.8 8.6 5.8 1.2 0 2.4-.3 3.4-.8" />
            <path d="M9.8 9.8a3.1 3.1 0 0 0 4.4 4.4" />
        </template>
        <template v-else-if="name === 'heart'">
            <path
                d="M20.5 8.7c0 5.4-8.5 10.1-8.5 10.1S3.5 14.1 3.5 8.7a4.6 4.6 0 0 1 8.5-2.5 4.6 4.6 0 0 1 8.5 2.5Z"
            />
        </template>
        <template v-else-if="name === 'github'">
            <path d="M8.2 19.5c-4.8 1.5-4.8-2.4-6.7-3" />
            <path
                d="M15.8 21v-3.8c.1-1-.4-2-1.1-2.7 3.7-.4 7.6-1.8 7.6-8a6.2 6.2 0 0 0-1.7-4.3 5.8 5.8 0 0 0-.1-4.3s-1.4-.4-4.5 1.7a15.6 15.6 0 0 0-8.1 0C4.8-2.5 3.4-2.1 3.4-2.1a5.8 5.8 0 0 0-.1 4.3 6.2 6.2 0 0 0-1.7 4.3c0 6.2 3.9 7.6 7.6 8-.7.7-1.2 1.7-1.1 2.7V21"
                transform="translate(0 2) scale(.84)"
            />
        </template>
        <template v-else-if="name === 'arrow-right' || name === 'arrow-up-right'">
            <path :d="name === 'arrow-right' ? 'M4 12h16m-6-6 6 6-6 6' : 'M6 18 18 6m-8 0h8v8'" />
        </template>
        <template v-else-if="name === 'plus'"><path d="M12 5v14M5 12h14" /></template>
        <template v-else-if="name === 'x'"><path d="m6 6 12 12M18 6 6 18" /></template>
        <template v-else-if="name === 'check'"><path d="m5 12 4 4L19 6" /></template>
        <template v-else-if="name === 'copy'">
            <rect x="8" y="8" width="11" height="12" rx="2" />
            <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h2" />
        </template>
        <template v-else-if="name === 'key'">
            <circle cx="8" cy="15" r="3.5" />
            <path d="m10.7 12.3 7.8-7.8M15.5 7.5l2 2M13 10l2 2" />
        </template>
        <template v-else-if="name === 'clock'">
            <circle cx="12" cy="12" r="8.5" />
            <path d="M12 7v5l3.5 2" />
        </template>
        <template v-else-if="name === 'chevron-down'"><path d="m6.5 9 5.5 5.5L17.5 9" /></template>
        <template v-else-if="name === 'settings'">
            <circle cx="12" cy="12" r="3" />
            <path
                d="M19 12a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3 1a7.2 7.2 0 0 0-2-1.2L14.3 3h-4l-.3 2.7a7.2 7.2 0 0 0-2 1.2l-2.3-1-2 3.4 2 1.5A7 7 0 0 0 5.6 12c0 .4 0 .8.1 1.2l-2 1.5 2 3.4 2.3-1a7.2 7.2 0 0 0 2 1.2l.3 2.7h4l.3-2.7a7.2 7.2 0 0 0 2-1.2l2.3 1 2-3.4-2-1.5c.1-.4.1-.8.1-1.2Z"
            />
        </template>
        <template v-else-if="name === 'trash'">
            <path d="M4 7h16M10 11v5M14 11v5M6.5 7l1 13h9l1-13M9 7V4h6v3" />
        </template>
        <template v-else-if="name === 'download'">
            <path d="M4 18v2h16v-2M12 4v11m0 0 4-4m-4 4-4-4" />
        </template>
        <template v-else-if="name === 'code'"><path d="m8.5 7-5 5 5 5M15.5 7l5 5-5 5" /></template>
        <template v-else-if="name === 'image'">
            <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
            <circle cx="8.5" cy="9" r="1.5" />
            <path d="m4 18 5.5-5 3.5 3 2.5-2.5 4.5 4.5" />
        </template>
        <template v-else-if="name === 'archive'">
            <path d="M4 7h16v13H4zM3 4h18v3H3zM10 11h4" />
        </template>
        <template v-else-if="name === 'music'">
            <path
                d="M9 18.5a2.5 2.5 0 1 1-2.5-2.5A2.5 2.5 0 0 1 9 18.5ZM18 16.5a2.5 2.5 0 1 1-2.5-2.5A2.5 2.5 0 0 1 18 16.5Z"
            />
            <path d="M9 18.5V6l9-2v12.5" />
        </template>
        <template v-else-if="name === 'video'">
            <rect x="3.5" y="6" width="12" height="12" rx="2" />
            <path d="m15.5 10 5-3v10l-5-3" />
        </template>
        <template v-else-if="name === 'alert'">
            <path
                d="M10.3 4.5 3.5 17a2 2 0 0 0 1.8 3h13.4a2 2 0 0 0 1.8-3L13.7 4.5a2 2 0 0 0-3.4 0Z"
            />
            <path d="M12 9v4M12 16.5h.01" />
        </template>
        <template v-else><circle cx="12" cy="12" r="8" stroke-dasharray="3 3" /></template>
    </svg>
</template>
