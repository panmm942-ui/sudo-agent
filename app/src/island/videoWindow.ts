import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { validArtworkUrl } from './artwork';
import { validCanvasUrl } from './canvas';
import type { VideoQuality } from './videoQuality';
import type { VideoSyncClock } from './videoSync';

export type VideoViewMode = 'fullscreen' | 'desktop';
export type VideoPresentation = 'video' | 'artwork' | 'lyrics';
export type VideoViewInput = { url: string | null; quality: VideoQuality; playing: boolean; pauseWithSong: boolean; presentation?: VideoPresentation; clock?: VideoSyncClock; expectedGeneration?:number };
export type VideoViewState = VideoViewInput & { mode: VideoViewMode; generation?:number };
export type VideoView = VideoViewState;

export function validVideoViewUrl(url: unknown): url is string {
  return validArtworkUrl(url) || validCanvasUrl(url);
}
export function openVideoView(input: VideoViewState): Promise<VideoViewState> {
  if (!validVideoInput(input)) return Promise.reject(new Error('This track has no supported video.'));
  return invoke('open_video_view', input);
}
export function syncVideoView(input: VideoViewInput): Promise<VideoViewState | null> {
  if (!validVideoInput(input)) return Promise.reject(new Error('This track has no supported video.'));
  return invoke('sync_video_view', input);
}
export function validVideoInput(input: VideoViewInput): boolean {
  return input.url === null ? input.presentation === 'artwork' || input.presentation === 'lyrics' : validVideoViewUrl(input.url);
}
export function getVideoView(): Promise<VideoViewState | null> { return invoke('get_video_view'); }
/** Builds the hidden fullscreen window ahead of time so opening it is instant. */
export function prepareVideoView(): Promise<void> { return invoke('prepare_video_view'); }
export function closeVideoView(): Promise<void> { return invoke('close_video_view'); }
export function listenVideoView(update: (state: VideoViewState | null) => void): Promise<() => void> {
  return listen<VideoViewState | null>('video-view-changed', event => update(event.payload));
}
