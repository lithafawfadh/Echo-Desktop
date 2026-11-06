/**
 * Supported audio encoding codecs across all providers and local extensions.
 */
export type AudioCodec = 'Opus' | 'Aac' | 'Flac' | 'Alac' | 'Wav' | 'Mp3' | 'Unknown';

/**
 * Detailed audio fidelity classification matching `echo_core::AudioFidelity`.
 */
export enum AudioFidelity {
  LossyStandard = 1,
  LossyHigh = 2,
  Lossy320 = 3,
  LosslessCD = 4,
  LosslessHiRes = 5,
}

/**
 * Serde JSON variant names emitted over Tauri IPC.
 */
export type AudioFidelityName =
  | 'LossyStandard'
  | 'LossyHigh'
  | 'Lossy320'
  | 'LosslessCD'
  | 'LosslessHiRes';

/**
 * Permissive union of numeric enum or string variant name.
 */
export type AudioFidelityValue = AudioFidelity | AudioFidelityName;

/**
 * Unified stream payload with audio properties and authentication headers.
 */
export interface AudioStream {
  url: string;
  codec: AudioCodec;
  bitrate_kbps: number;
  sample_rate_hz: number;
  is_lossless: boolean;
  bit_depth?: number;
  fidelity?: AudioFidelityValue;
  content_length_bytes?: number;
  http_headers?: Record<string, string>;
}

/**
 * Canonical Track entity used across search, queue, and playback layers.
 */
export interface Track {
  id: string;
  title: string;
  artist: string;
  album?: string;
  duration_seconds: number;
  thumbnail_url?: string;
  isrc?: string;
  fidelity?: AudioFidelityValue;
  bit_depth?: number;
  sample_rate_hz?: number;
  bitrate_kbps?: number;
  codec?: AudioCodec | string;
}

/**
 * Combined track metadata and active stream properties emitted over `playback://track_change`.
 */
export interface TrackMeta extends Track {
  codec: AudioCodec;
  bitrate_kbps: number;
  sample_rate_hz: number;
  is_lossless: boolean;
  bit_depth?: number;
  fidelity?: AudioFidelityValue;
}

export interface LyricLine {
  timestamp_ms: number;
  text: string;
}

export interface Lyrics {
  plain?: string;
  synced: LyricLine[];
}

export interface PlaybackProgress {
  position_ms: number;
  duration_ms: number;
}

export type PlaybackState = 'Stopped' | 'Buffering' | 'Playing' | 'Paused';

/**
 * Normalizes any incoming fidelity value (string, number, or enum) into canonical AudioFidelity.
 */
export function parseAudioFidelity(val?: AudioFidelityValue | number | string | null): AudioFidelity | null {
  if (val === null || val === undefined) return null;
  if (typeof val === 'number') {
    if (val >= AudioFidelity.LossyStandard && val <= AudioFidelity.LosslessHiRes) {
      return val as AudioFidelity;
    }
    return null;
  }
  if (typeof val === 'string') {
    switch (val) {
      case 'LossyStandard': return AudioFidelity.LossyStandard;
      case 'LossyHigh': return AudioFidelity.LossyHigh;
      case 'Lossy320': return AudioFidelity.Lossy320;
      case 'LosslessCD': return AudioFidelity.LosslessCD;
      case 'LosslessHiRes': return AudioFidelity.LosslessHiRes;
      default: return null;
    }
  }
  return null;
}

/**
 * Audio fidelity badge metadata for UI styling and technical indicators.
 */
export interface FidelityBadgeConfig {
  fidelity: AudioFidelity;
  label: string;          // Full label: "HI-RES LOSSLESS", "LOSSLESS", etc.
  shortLabel: string;     // Pill label: "HI-RES LOSSLESS", "LOSSLESS", "320 KBPS", etc.
  color: string;          // Accent text & dot color
  bg: string;             // Translucent pill background
  border: string;         // 1px rim border
  glow: string;           // Dot box-shadow glow
  detailText: string;     // Technical subtitle
  bannerTitle: string;    // Fullscreen banner title
}

export const FIDELITY_BADGE_CONFIGS: Record<AudioFidelity, FidelityBadgeConfig> = {
  [AudioFidelity.LosslessHiRes]: {
    fidelity: AudioFidelity.LosslessHiRes,
    label: 'HI-RES LOSSLESS',
    shortLabel: 'HI-RES LOSSLESS',
    color: '#a855f7', // Purple
    bg: 'rgba(168, 85, 247, 0.12)',
    border: 'rgba(168, 85, 247, 0.35)',
    glow: 'rgba(168, 85, 247, 0.6)',
    detailText: 'FLAC • 24-bit / 96kHz+',
    bannerTitle: 'STUDIO MASTER',
  },
  [AudioFidelity.LosslessCD]: {
    fidelity: AudioFidelity.LosslessCD,
    label: 'LOSSLESS',
    shortLabel: 'LOSSLESS',
    color: '#3b82f6', // Blue
    bg: 'rgba(59, 130, 246, 0.12)',
    border: 'rgba(59, 130, 246, 0.35)',
    glow: 'rgba(59, 130, 246, 0.6)',
    detailText: 'FLAC • 16-bit / 44.1kHz',
    bannerTitle: 'CD LOSSLESS',
  },
  [AudioFidelity.Lossy320]: {
    fidelity: AudioFidelity.Lossy320,
    label: 'HIGH QUALITY',
    shortLabel: '320 KBPS',
    color: '#10b981', // Green
    bg: 'rgba(16, 185, 129, 0.12)',
    border: 'rgba(16, 185, 129, 0.35)',
    glow: 'rgba(16, 185, 129, 0.6)',
    detailText: 'MP3 • 320 kbps CBR',
    bannerTitle: 'HIGH FIDELITY',
  },
  [AudioFidelity.LossyHigh]: {
    fidelity: AudioFidelity.LossyHigh,
    label: 'STANDARD',
    shortLabel: 'STANDARD',
    color: '#6b7280', // Gray
    bg: 'rgba(107, 114, 128, 0.12)',
    border: 'rgba(107, 114, 128, 0.35)',
    glow: 'rgba(107, 114, 128, 0.5)',
    detailText: 'OPUS • 160 kbps (InnerTube)',
    bannerTitle: 'STANDARD AUDIO',
  },
  [AudioFidelity.LossyStandard]: {
    fidelity: AudioFidelity.LossyStandard,
    label: 'LEGACY',
    shortLabel: 'LEGACY',
    color: '#ef4444', // Red
    bg: 'rgba(239, 68, 68, 0.12)',
    border: 'rgba(239, 68, 68, 0.35)',
    glow: 'rgba(239, 68, 68, 0.5)',
    detailText: 'AAC • 128 kbps (InnerTube)',
    bannerTitle: 'LEGACY AUDIO',
  },
};

/**
 * Resolves the appropriate badge configuration with graceful inference fallbacks.
 */
export function resolveBadgeConfig(track?: TrackMeta | null): FidelityBadgeConfig | null {
  if (!track) return null;
  const parsed = parseAudioFidelity(track.fidelity);
  if (parsed && FIDELITY_BADGE_CONFIGS[parsed]) {
    return FIDELITY_BADGE_CONFIGS[parsed];
  }
  // Smart inference fallback if fidelity enum is not yet populated
  if (track.is_lossless) {
    if (track.bit_depth && track.bit_depth > 16) {
      return FIDELITY_BADGE_CONFIGS[AudioFidelity.LosslessHiRes];
    }
    return FIDELITY_BADGE_CONFIGS[AudioFidelity.LosslessCD];
  }
  if (track.bitrate_kbps && track.bitrate_kbps >= 320) {
    return FIDELITY_BADGE_CONFIGS[AudioFidelity.Lossy320];
  }
  if (track.codec === 'Opus') {
    return FIDELITY_BADGE_CONFIGS[AudioFidelity.LossyHigh];
  }
  if (track.codec === 'Aac') {
    return FIDELITY_BADGE_CONFIGS[AudioFidelity.LossyStandard];
  }
  return FIDELITY_BADGE_CONFIGS[AudioFidelity.LossyHigh];
}
