export type AudioCodec = 'Opus' | 'Aac' | 'Flac' | 'Mp3' | 'Unknown';

export interface AudioStream {
  url: string;
  codec: AudioCodec;
  bitrate_kbps: number;
  sample_rate_hz: number;
  is_lossless: boolean;
}

export interface Track {
  id: string;
  title: string;
  artist: string;
  album?: string;
  duration_seconds: number;
  thumbnail_url?: string;
}

export interface TrackMeta extends Track {
  codec: AudioCodec;
  bitrate_kbps: number;
  sample_rate_hz: number;
  is_lossless: boolean;
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
