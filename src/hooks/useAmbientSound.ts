import { useSyncExternalStore } from 'react';
import { getAmbientSoundSnapshot, subscribeAmbientSound, type AmbientSoundSnapshot } from '../services/ambientSoundPlayer';

/** 订阅白噪音播放器状态。播放器是模块级单例，组件卸载不会停止播放。 */
export function useAmbientSound(): AmbientSoundSnapshot {
  return useSyncExternalStore(subscribeAmbientSound, getAmbientSoundSnapshot, getAmbientSoundSnapshot);
}
