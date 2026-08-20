import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PetSettings, PetSnapshot } from './types';

const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const readCommand = <T>(name: string, args?: Record<string, unknown>) =>
  inTauri() ? invoke<T>(name, args) : Promise.reject(new Error('Tauri is unavailable'));
const writeCommand = (name: string, args?: Record<string, unknown>) =>
  inTauri() ? invoke<void>(name, args) : Promise.resolve();

export const getSnapshot = () => readCommand<PetSnapshot>('get_snapshot');
export const getSettings = () => readCommand<PetSettings>('get_settings');
export const setMotionEnabled = (enabled: boolean) => writeCommand('set_motion_enabled', { enabled });
export const hideToTray = () => writeCommand('hide_to_tray');
export const acknowledgeCompletion = () => writeCommand('acknowledge_completion');
export const dismissCompletedTask = (turnId: string) => writeCommand('dismiss_completed_task', { turnId });

export const onSnapshot = (handler: (snapshot: PetSnapshot) => void): Promise<UnlistenFn> =>
  inTauri()
    ? listen<PetSnapshot>('pet://snapshot', (event) => handler(event.payload))
    : Promise.resolve(() => undefined);
export const onSettings = (handler: (settings: PetSettings) => void): Promise<UnlistenFn> =>
  inTauri() ? listen<PetSettings>('pet://settings', (event) => handler(event.payload)) : Promise.resolve(() => undefined);
