import type { InjectionKey } from 'vue';

export const cliInstallKey: InjectionKey<(event: MouseEvent) => void> = Symbol('cli-install');
