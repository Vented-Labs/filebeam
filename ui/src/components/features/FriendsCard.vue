<script setup lang="ts">
import { ref } from 'vue';
import AuthLink from '../auth/AuthLink.vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';
import { onTabKeydown } from './tabs';

defineProps<{ signedIn: boolean; registrationEnabled: boolean }>();

// Illustrative fixtures only; selecting one changes the example command, nothing is sent.
const friends = [
    { handle: 'alex', name: 'Alex', avatar: '' },
    { handle: 'sam', name: 'Sam', avatar: 'avatar-two' },
    { handle: 'jules', name: 'Jules', avatar: 'avatar-three' },
];
const tab = ref<'friends' | 'receiving'>('friends');
const selected = ref('alex');
</script>

<template>
    <article class="feature-card friends-card" aria-labelledby="friends-title">
        <div class="card-heading">
            <div class="section-icon"><Icon class="icon" name="users" /></div>
            <span class="pill">New</span>
        </div>
        <h2 id="friends-title">Send to a friend.</h2>
        <p>
            Add a contact once. Send straight to their inbox.<br />Friends stay on the same Filebeam
            server.
        </p>
        <div class="friends-demo">
            <div class="friends-toolbar">
                <div class="friend-tabs" role="tablist" aria-label="Contacts illustration">
                    <button
                        id="friends-tab"
                        type="button"
                        role="tab"
                        aria-controls="friends-panel"
                        :aria-selected="tab === 'friends'"
                        :tabindex="tab === 'friends' ? 0 : -1"
                        @click="tab = 'friends'"
                        @keydown="onTabKeydown"
                    >
                        Friends <span>3</span>
                    </button>
                    <button
                        id="receiving-tab"
                        type="button"
                        role="tab"
                        aria-controls="friends-panel"
                        :aria-selected="tab === 'receiving'"
                        :tabindex="tab === 'receiving' ? 0 : -1"
                        @click="tab = 'receiving'"
                        @keydown="onTabKeydown"
                    >
                        Receiving
                    </button>
                </div>
                <span class="demo-label">Example</span>
            </div>
            <div id="friends-panel" role="tabpanel" :aria-labelledby="`${tab}-tab`">
                <template v-if="tab === 'friends'">
                    <div class="friend-list" role="group" aria-label="Example recipient">
                        <button
                            v-for="friend in friends"
                            :key="friend.handle"
                            type="button"
                            class="friend-row"
                            :class="{ selected: selected === friend.handle }"
                            :aria-pressed="selected === friend.handle"
                            @click="selected = friend.handle"
                        >
                            <span class="avatar" :class="friend.avatar">{{ friend.name[0] }}</span>
                            <span
                                >{{ friend.name }}<small>@{{ friend.handle }}</small></span
                            >
                            <span class="friend-check"><Icon class="icon" name="check" /></span>
                        </button>
                    </div>
                    <div class="friend-send">
                        <code
                            ><span>beam</span> to <b>@{{ selected }}</b
                            >{{ ' ' }} <span class="muted">notes.zip</span></code
                        >
                    </div>
                    <div class="friend-status">
                        Pick a friend to see the command. Deliveries land in their encrypted inbox.
                    </div>
                </template>
                <template v-else>
                    <ul class="receiving-summary">
                        <li>
                            <strong>Accept files from</strong
                            ><span>Anyone, friends only, or no one</span>
                        </li>
                        <li>
                            <strong>Auto-receive from friends</strong
                            ><span>Off by default, per client</span>
                        </li>
                    </ul>
                    <p class="receive-explanation">
                        Accepted friends can deliver into encrypted private staging. Saving or
                        exporting stays an explicit choice.
                    </p>
                    <div class="private-staging-note">
                        <Icon class="icon" name="lock" /> Encrypted staging. Explicit Save / Export.
                    </div>
                </template>
            </div>
        </div>
        <AppLink v-if="signedIn" href="/account/contacts" class="feature-cta"
            >Open contacts <Icon class="icon" name="arrow-right"
        /></AppLink>
        <AuthLink v-else :href="registrationEnabled ? '/register' : '/login'" class="feature-cta"
            >{{
                registrationEnabled ? 'Create an account to add friends' : 'Sign in to add friends'
            }}
            <Icon class="icon" name="arrow-right"
        /></AuthLink>
    </article>
</template>
