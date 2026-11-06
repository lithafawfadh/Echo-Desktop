import { invoke } from '@tauri-apps/api/core';
import type { TrackMeta } from '../types';
import { resolveStreamViaWebView } from './ytWebResolve';

/**
 * Universal track player that attempts standard Rust backend streaming first,
 * and if blocked by YouTube LOGIN_REQUIRED / bot-guard, automatically falls back
 * to resolving via WebView2 authenticated browser session and plays via CPAL.
 */
export async function playTrackWithFallback(track: TrackMeta): Promise<void> {
  try {
    // Attempt standard Rust stream resolution & playback
    await invoke('play_stream', { track });
  } catch (err: any) {
    const errMsg = String(err?.message || err || '');
    console.warn('Standard play_stream failed, evaluating fallback:', errMsg);

    if (
      errMsg.includes('LOGIN_REQUIRED') ||
      errMsg.includes('bot') ||
      errMsg.includes('restricted') ||
      errMsg.includes('403') ||
      errMsg.includes('Stream resolution error')
    ) {
      console.info('Attempting authenticated WebView2 stream resolution for:', track.id);
      try {
        const webStream = await resolveStreamViaWebView(track.id);
        console.info('WebView2 stream resolved successfully:', webStream.codec, webStream.bitrate_kbps);

        await invoke('play_direct_url', {
          track,
          url: webStream.url,
          codec: webStream.codec,
          bitrateKbps: webStream.bitrate_kbps,
          sampleRateHz: webStream.sample_rate_hz,
        });
        return;
      } catch (fallbackErr) {
        console.error('WebView2 stream resolution fallback failed:', fallbackErr);
        throw fallbackErr;
      }
    }

    throw err;
  }
}
