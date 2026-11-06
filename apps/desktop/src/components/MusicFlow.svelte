<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { player } from '../stores/playerState.svelte';
  import type { TrackMeta } from '../types';

  const bentoCards: Array<{
    id: number;
    title: string;
    subtitle: string;
    span: string;
    badge: string;
    sampleTrack: TrackMeta;
  }> = [
    {
      id: 1,
      title: 'DAILY LOSSLESS RADAR',
      subtitle: '96kHz / 24-Bit Studio Curated FLAC Streams',
      span: 'col-span-2 row-span-2',
      badge: 'HI-RES LIVE',
      sampleTrack: {
        id: 'radar_01',
        title: 'Solar Resonance (Studio Master)',
        artist: 'Echo Harmonic Labs',
        album: 'Lossless Echoes Vol. 1',
        duration_seconds: 245,
        thumbnail_url: 'https://images.unsplash.com/photo-1618005182384-a83a8bd57fbe?w=500&q=80',
        codec: 'Flac',
        bitrate_kbps: 1411,
        sample_rate_hz: 96000,
        is_lossless: true,
      },
    },
    {
      id: 2,
      title: 'SYNTHESIS & GLITCH',
      subtitle: 'Nothing OS Monochromatic Sub-bass Waves',
      span: 'col-span-1 row-span-1',
      badge: 'BASSLINE',
      sampleTrack: {
        id: 'synth_02',
        title: 'Binary Cascade',
        artist: 'Nothing Sound Collective',
        album: 'Matrix Audio',
        duration_seconds: 198,
        thumbnail_url: 'https://images.unsplash.com/photo-1550751827-4bd374c3f58b?w=500&q=80',
        codec: 'Opus',
        bitrate_kbps: 160,
        sample_rate_hz: 48000,
        is_lossless: false,
      },
    },
    {
      id: 3,
      title: 'NOTHING RADIO 01',
      subtitle: 'Broadcast host: Carl Pei & Guests',
      span: 'col-span-1 row-span-1',
      badge: 'LIVE ON AIR',
      sampleTrack: {
        id: 'radio_03',
        title: 'Design Conversations (Episode 01)',
        artist: 'Nothing Studio London',
        album: 'Design Cast',
        duration_seconds: 320,
        thumbnail_url: 'https://images.unsplash.com/photo-1508700115892-45ecd05ae2ad?w=500&q=80',
        codec: 'Aac',
        bitrate_kbps: 256,
        sample_rate_hz: 44100,
        is_lossless: false,
      },
    },
    {
      id: 4,
      title: 'ACOUSTIC REVERIE',
      subtitle: 'Uncompressed Direct Studio Microphones',
      span: 'col-span-2 row-span-1',
      badge: 'LOSSLESS MASTER',
      sampleTrack: {
        id: 'acoustic_04',
        title: 'Nocturne in C Minor',
        artist: 'Echo String Quartet',
        album: 'Acoustic Sessions',
        duration_seconds: 280,
        thumbnail_url: 'https://images.unsplash.com/photo-1511671782779-c97d3d27a1d4?w=500&q=80',
        codec: 'Flac',
        bitrate_kbps: 1411,
        sample_rate_hz: 96000,
        is_lossless: true,
      },
    },
  ];

  async function handleCardClick(card: (typeof bentoCards)[0]) {
    player.setTrack(card.sampleTrack);
    player.isPlaying = true;
    try {
      await invoke('play_stream', { track: card.sampleTrack });
    } catch (err) {
      console.warn('Play stream IPC:', err);
    }
  }
</script>

<div class="echo-music-flow">
  <header class="section-header">
    <div class="flex items-center gap-3">
      <span class="live-dot" aria-hidden="true"></span>
      <h2 class="title-text">FLOW MATRIX</h2>
    </div>
    <span class="subtitle-text">ALGORITHMIC HARMONIC DISCOVERY × HIGH RESOLUTION LOSSLESS</span>
  </header>

  <div class="bento-grid">
    {#each bentoCards as card}
      <div
        class="echo-glass-card bento-card {card.span}"
        onclick={() => handleCardClick(card)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && handleCardClick(card)}
      >
        <div class="bento-glow" aria-hidden="true"></div>
        <div class="bento-content">
          <span class="card-badge">{card.badge}</span>
          <h3 class="card-title">{card.title}</h3>
          <p class="card-subtitle">{card.subtitle}</p>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .echo-music-flow {
    display: flex;
    flex-direction: column;
    gap: var(--echo-spacing-lg);
  }

  .section-header {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--echo-color-accent);
    box-shadow: 0 0 10px var(--echo-color-accent);
  }

  .section-header .title-text {
    font-family: "NDot57", monospace;
    font-size: 20px;
    letter-spacing: 0.1em;
    margin: 0;
    color: #fff;
  }

  .section-header .subtitle-text {
    font-family: "NDot57", monospace;
    font-size: 10px;
    color: var(--echo-text-muted);
    letter-spacing: 0.08em;
  }

  .bento-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    grid-auto-rows: 190px;
    gap: 20px;
  }

  .col-span-2 { grid-column: span 2; }
  .col-span-1 { grid-column: span 1; }
  .row-span-2 { grid-row: span 2; }
  .row-span-1 { grid-row: span 1; }

  .bento-card {
    padding: var(--echo-spacing-lg);
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    cursor: pointer;
    transition: transform 0.25s var(--echo-spring-bounce), box-shadow 0.25s;
    background: rgba(14, 15, 20, 0.6);
  }

  .bento-card:hover {
    transform: translateY(-4px) scale(1.008);
    box-shadow: 0 24px 48px -12px rgba(0, 0, 0, 0.7),
                0 0 0 1px rgba(255, 255, 255, 0.18);
  }

  .card-badge {
    font-family: "NDot57", monospace;
    font-size: 9px;
    color: var(--echo-color-accent);
    letter-spacing: 0.12em;
    font-weight: 700;
  }

  .card-title {
    font-size: 20px;
    font-weight: 700;
    margin: 6px 0 0 0;
    color: #fff;
    letter-spacing: -0.015em;
  }

  .card-subtitle {
    font-size: 12px;
    color: var(--echo-text-secondary);
    margin: 4px 0 0 0;
    line-height: 1.4;
  }
</style>
