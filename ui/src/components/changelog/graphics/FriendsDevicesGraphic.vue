<script setup lang="ts">
import Icon from '../../primitives/Icon.vue';
import ReleaseArtFrame from './ReleaseArtFrame.vue';

// Decorative fixtures only: never the visitor's contacts or devices.
const friends = [
    { initial: 'S', name: 'Sam', handle: '@sam', primary: true },
    { initial: 'J', name: 'Jules', handle: '@jules', primary: false },
];
const devices = [
    { icon: 'monitor', label: 'Desktop', selected: true },
    { icon: 'laptop', label: 'Laptop', selected: false },
    { icon: 'storage', label: 'Server', selected: false },
] as const;
</script>

<template>
    <ReleaseArtFrame
        title="Send by name."
        caption="Friends and your own devices."
        description="Illustration: a Friends list sending with beam to @sam through an encrypted inbox, beside your own paired computers sending with beam to Desktop over a direct peer-to-peer connection."
        tall
    >
        <div class="connections">
            <div class="connections__shadow" />
            <div class="slip slip--friends">
                <div class="slip__title">
                    <Icon name="users" :size="14" /> Friends <span>2</span>
                </div>
                <div v-for="friend in friends" :key="friend.handle" class="friend">
                    <span
                        class="friend__avatar"
                        :class="{ 'friend__avatar--second': !friend.primary }"
                        >{{ friend.initial }}</span
                    >
                    <div>
                        {{ friend.name }}<small>{{ friend.handle }}</small>
                    </div>
                    <Icon name="check" :size="12" />
                </div>
                <div class="command"><span>beam</span> to <b>@sam</b> <i>notes.zip</i></div>
                <div class="foot"><Icon name="lock" :size="10" /> Encrypted inbox</div>
            </div>
            <div class="slip slip--devices">
                <div class="slip__title"><Icon name="webrtc-p2p" :size="14" /> Your devices</div>
                <div class="lineup">
                    <div
                        v-for="device in devices"
                        :key="device.label"
                        :class="{ 'lineup__item--selected': device.selected }"
                    >
                        <Icon :name="device.icon" :size="25" /><span>{{ device.label }}</span>
                    </div>
                </div>
                <div class="command"><span>beam</span> to <b>Desktop</b> <i>notes.zip</i></div>
                <div class="foot">
                    <Icon name="webrtc-p2p" :size="10" /> Paired devices · Direct P2P
                </div>
            </div>
        </div>
    </ReleaseArtFrame>
</template>

<style scoped>
.connections {
    position: absolute;
    top: 0;
    right: 16px;
    width: 478px;
    height: 236px;
    transform-origin: right center;
}
.connections__shadow {
    position: absolute;
    top: 23px;
    left: 186px;
    width: 231px;
    height: 167px;
    border: 1px solid var(--fb-border);
    border-radius: 13px;
    background: var(--fb-accent-surface);
    opacity: 0.6;
    transform: rotate(13deg);
}
.slip {
    position: absolute;
    display: flex;
    box-sizing: border-box;
    flex-direction: column;
    padding: 13px 14px 10px;
    border: 1px solid color-mix(in srgb, var(--fb-control-border) 48%, var(--fb-border));
    border-radius: 12px;
    background: linear-gradient(145deg, var(--cl-paper), var(--fb-surface));
    box-shadow: var(--cl-shadow);
    color: var(--fb-text);
}
.slip--friends {
    z-index: 3;
    top: 22px;
    left: 17px;
    width: 223px;
    height: 192px;
    transform: rotate(-5deg);
}
.slip--devices {
    z-index: 2;
    top: 43px;
    left: 236px;
    width: 229px;
    height: 174px;
    transform: rotate(6deg);
}
.slip__title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 10px;
    font-weight: 500;
}
.slip__title .fb-icon {
    color: var(--fb-accent-text);
}
.slip__title span {
    margin-left: auto;
    color: var(--fb-text-muted);
    font: 8px var(--fb-font-code);
}
.friend {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 10px;
    font-size: 10px;
}
.friend > div {
    flex: 1;
    line-height: 1.5;
}
.friend small {
    display: block;
    color: var(--fb-text-muted);
    font-size: 8px;
}
.friend > .fb-icon {
    color: var(--fb-accent-text);
    opacity: 0.8;
}
.friend__avatar {
    display: grid;
    width: 27px;
    height: 29px;
    place-items: center;
    border: 1px solid var(--cl-line);
    border-radius: 9px;
    background: var(--fb-selected-surface);
    color: var(--fb-accent-text);
    font-size: 11px;
}
.friend__avatar--second {
    background: var(--fb-surface-raised);
    color: var(--fb-text-muted);
}
.command {
    margin-top: auto;
    padding: 5px 6px;
    border: 1px solid var(--cl-line);
    border-radius: 5px;
    background: var(--fb-surface-sunken);
    font: 8px/1.7 var(--fb-font-code);
    white-space: nowrap;
}
.command span,
.command b {
    color: var(--fb-accent-text);
    font-weight: 400;
}
.command i {
    color: var(--fb-text-muted);
    font-style: normal;
}
.foot {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    margin-top: 7px;
    color: var(--fb-text-muted);
    font-size: 7px;
}
.foot .fb-icon {
    color: var(--fb-accent-text);
}
.lineup {
    display: flex;
    justify-content: space-between;
    gap: 5px;
    margin: 15px 0 13px;
}
.lineup > div {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    padding: 8px 4px 6px;
    border-radius: 7px;
    color: var(--fb-text-muted);
}
.lineup > .lineup__item--selected {
    background: var(--fb-accent-surface);
    box-shadow: inset 0 0 0 1px var(--cl-line);
    color: var(--fb-accent-text);
}
.lineup span {
    font-size: 7px;
}
@container (max-width: 760px) {
    .connections {
        top: -7px;
        right: 2px;
        transform: scale(0.86);
    }
}
@container (max-width: 640px) {
    .connections {
        top: 0;
        right: 50%;
        margin-right: -239px;
        transform: scale(0.95);
        transform-origin: center center;
    }
}
@container (max-width: 540px) {
    .connections {
        top: calc(50% - 118px);
        transform: scale(0.8);
    }
}
@container (max-width: 440px) {
    .connections {
        transform: scale(0.7);
    }
}
@container (max-width: 360px) {
    .connections {
        transform: scale(0.6);
    }
}
</style>
