import {
  type TrackMeta,
  type Lyrics,
  type PlaybackState,
  type AudioStream,
  type AudioFidelity,
  type FidelityBadgeConfig,
  parseAudioFidelity,
  resolveBadgeConfig,
} from '../types';

function computeHarmonicPalette(title: string, artist: string): [string, string, string, string] {
  let hash = 0;
  const str = `${title} - ${artist}`;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0;
  }
  const hue = Math.abs(hash) % 360;
  const secondaryHue = (hue + 45) % 360;
  const accentHue = (hue + 180) % 360;

  // Convert HSL to Hex
  const hslToHex = (h: number, s: number, l: number): string => {
    l /= 100;
    const a = (s * Math.min(l, 1 - l)) / 100;
    const f = (n: number) => {
      const k = (n + h / 30) % 12;
      const color = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
      return Math.round(255 * color).toString(16).padStart(2, '0');
    };
    return `#${f(0)}${f(8)}${f(4)}`;
  };

  return [
    hslToHex(hue, 65, 14),           // c1: Dominant vibrant deep tone
    hslToHex(secondaryHue, 55, 12),  // c2: Secondary harmonic
    '#060608',                       // c3: OLED void black
    hslToHex(accentHue, 80, 20),     // c4: High-energy accent
  ];
}

export class PlayerState {
  currentTrack = $state<TrackMeta | null>(null);
  isPlaying = $state(false);
  isBuffering = $state(false);
  positionMs = $state(0);
  durationMs = $state(0);
  volume = $state(0.85);
  isMuted = $state(false);
  queue = $state<TrackMeta[]>([]);
  isQueueOpen = $state(false);
  isFullScreenLyrics = $state(false);
  currentLyrics = $state<Lyrics | null>(null);
  activeTab = $state<'flow' | 'search' | 'library' | 'favorites'>('flow');
  searchQuery = $state('');
  searchResults = $state<TrackMeta[]>([]);
  isSearching = $state(false);
  palette = $state<[string, string, string, string]>([
    '#201226',
    '#0e2238',
    '#060608',
    '#401117'
  ]);

  // Derived Values
  progressFraction = $derived(
    this.durationMs > 0 ? Math.min(1, Math.max(0, this.positionMs / this.durationMs)) : 0
  );

  formattedPosition = $derived(this.formatTime(this.positionMs));
  formattedDuration = $derived(this.formatTime(this.durationMs));

  // Reactive audio quality badge configuration derived from active track
  fidelityBadge = $derived<FidelityBadgeConfig | null>(
    resolveBadgeConfig(this.currentTrack)
  );

  // Normalized active fidelity enum
  activeFidelity = $derived<AudioFidelity | null>(
    this.currentTrack ? parseAudioFidelity(this.currentTrack.fidelity) : null
  );

  // Formatted hardware technical spec string (e.g. "96.0 kHz / 24-BIT • FLAC")
  audioTechReadout = $derived.by(() => {
    if (!this.currentTrack) return '';
    const rateKhz = ((this.currentTrack.sample_rate_hz || 44100) / 1000).toFixed(1);
    const depth = this.currentTrack.bit_depth ? `${this.currentTrack.bit_depth}-BIT` : null;
    const codec = (this.currentTrack.codec || 'FLAC').toUpperCase();
    const kbps = this.currentTrack.bitrate_kbps ? `${this.currentTrack.bitrate_kbps} KBPS` : '';

    if (this.currentTrack.is_lossless || (this.currentTrack.bit_depth && this.currentTrack.bit_depth >= 16)) {
      return depth ? `${rateKhz} kHz / ${depth} • ${codec}` : `${rateKhz} kHz • ${codec}`;
    }
    return `${codec} • ${kbps || `${rateKhz} kHz`}`;
  });

  formatTime(ms: number): string {
    const totalSecs = Math.floor(ms / 1000);
    const m = Math.floor(totalSecs / 60);
    const s = totalSecs % 60;
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }

  setTrack(track: TrackMeta) {
    this.currentTrack = {
      ...track,
      fidelity: parseAudioFidelity(track.fidelity) ?? track.fidelity,
    };
    this.durationMs = track.duration_seconds * 1000;
    this.positionMs = 0;
    this.palette = computeHarmonicPalette(track.title, track.artist);
  }

  /**
   * Updates stream resolution properties in-place without resetting playback progress.
   */
  updateStreamMetadata(meta: Partial<TrackMeta> | AudioStream) {
    if (!this.currentTrack) return;
    const normalizedFidelity = parseAudioFidelity(meta.fidelity) ?? this.currentTrack.fidelity;
    this.currentTrack = {
      ...this.currentTrack,
      codec: (meta.codec as any) ?? this.currentTrack.codec,
      bitrate_kbps: meta.bitrate_kbps ?? this.currentTrack.bitrate_kbps,
      sample_rate_hz: meta.sample_rate_hz ?? this.currentTrack.sample_rate_hz,
      is_lossless: meta.is_lossless ?? this.currentTrack.is_lossless,
      bit_depth: meta.bit_depth ?? this.currentTrack.bit_depth,
      fidelity: normalizedFidelity,
    };
  }

  addToQueue(track: TrackMeta) {
    this.queue = [...this.queue, track];
  }

  clearQueue() {
    this.queue = [];
  }

  playNext(): TrackMeta | null {
    if (this.queue.length === 0) return null;
    const [next, ...rest] = this.queue;
    this.queue = rest;
    this.setTrack(next);
    return next;
  }
}

export const player = new PlayerState();
