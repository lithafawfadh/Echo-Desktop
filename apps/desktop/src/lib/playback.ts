import type { TrackMeta } from '../types';
import { player } from '../stores/playerState.svelte';

/**
 * Universal track player: Sets the active track on the Svelte store.
 * The reactive YouTubeAudioBridge automatically handles live audio playback!
 */
export async function playTrackWithFallback(track: TrackMeta): Promise<void> {
  player.setTrack(track);
  player.isPlaying = true;
}
