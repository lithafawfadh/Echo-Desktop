<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { invoke } from '@tauri-apps/api/core';
  import { player } from '../stores/playerState.svelte';
  import type { Track, TrackMeta } from '../types';

  let searchTimeout: number | null = null;

  async function handleSearchInput(e: Event) {
    const query = (e.target as HTMLInputElement).value;
    player.searchQuery = query;

    if (searchTimeout) clearTimeout(searchTimeout);
    if (!query.trim()) {
      player.searchResults = [];
      player.isSearching = false;
      return;
    }

    player.isSearching = true;
    searchTimeout = window.setTimeout(async () => {
      try {
        const results = await invoke<Track[]>('search_tracks', { query });
        player.searchResults = results.map((t) => ({
          ...t,
          codec: 'Opus',
          bitrate_kbps: 160,
          sample_rate_hz: 48000,
          is_lossless: false,
        }));
        if (player.activeTab !== 'search') {
          player.activeTab = 'search';
        }
      } catch (err) {
        console.error('Search failed:', err);
      } finally {
        player.isSearching = false;
      }
    }, 280);
  }

  async function minimize() {
    try {
      await invoke('minimize_window');
    } catch {
      try {
        await getCurrentWindow().minimize();
      } catch (e) {
        console.warn('Tauri window minimize:', e);
      }
    }
  }

  async function toggleMaximize() {
    try {
      await invoke('toggle_maximize_window');
    } catch {
      try {
        await getCurrentWindow().toggleMaximize();
      } catch (e) {
        console.warn('Tauri window toggleMaximize:', e);
      }
    }
  }

  async function close() {
    try {
      await invoke('close_window');
    } catch {
      try {
        await getCurrentWindow().close();
      } catch (e) {
        console.warn('Tauri window close:', e);
      }
    }
  }
</script>

<header class="echo-topbar" data-tauri-drag-region>
  <div class="search-region" data-tauri-drag-region="false">
    <div class="search-input-wrapper">
      <span class="search-icon" aria-hidden="true">⚲</span>
      <input
        type="text"
        placeholder="Search track, artist, lossless flac..."
        value={player.searchQuery}
        oninput={handleSearchInput}
        class="search-input"
        aria-label="Search track or artist"
      />
      {#if player.isSearching}
        <span class="searching-spinner" aria-hidden="true">◌</span>
      {/if}
    </div>
  </div>

  <!-- Audio Codec Specification Pill (Nothing OS 5.0 / Apple Music Lossless) -->
  {#if player.currentTrack}
    <div class="spec-pill" title="Hardware Stream Format Indicator">
      <span class="dot-live" class:buffering={player.isBuffering}></span>
      <span class="pill-codec">{(player.currentTrack.codec || 'Opus').toUpperCase()}</span>
      <span class="pill-divider">/</span>
      <span class="pill-specs">
        {((player.currentTrack.sample_rate_hz || 48000) / 1000).toFixed(1)}kHz {player.currentTrack.bitrate_kbps || 160}kbps
      </span>
      {#if player.currentTrack.is_lossless}
        <span class="lossless-badge">HI-RES</span>
      {/if}
    </div>
  {:else}
    <div class="standby-pill">
      <span class="standby-dot"></span>
      <span class="echo-font-matrix text-[10px] tracking-wider text-echo-text-muted">ECHO ENGINE STANDBY</span>
    </div>
  {/if}

  <!-- Tauri Window Controls -->
  <div class="window-controls" data-tauri-drag-region="false">
    <button class="win-btn" onclick={minimize} aria-label="Minimize Window">─</button>
    <button class="win-btn" onclick={toggleMaximize} aria-label="Maximize Window">□</button>
    <button class="win-btn close" onclick={close} aria-label="Close Window">✕</button>
  </div>
</header>

<style>
  .echo-topbar {
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--echo-spacing-md);
    background: rgba(6, 6, 8, 0.72);
    backdrop-filter: blur(20px);
    border-bottom: var(--echo-border-rim);
    z-index: 50;
    user-select: none;
  }

  .search-region {
    width: 320px;
  }

  .search-input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 11px;
    font-size: 13px;
    color: var(--echo-text-muted);
    pointer-events: none;
  }

  .searching-spinner {
    position: absolute;
    right: 12px;
    font-size: 13px;
    color: var(--echo-color-accent);
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .search-input {
    width: 100%;
    padding: 7px 12px 7px 32px;
    background: var(--echo-glass-subtle);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--echo-radius-full);
    color: #fff;
    font-size: 12px;
    outline: none;
    transition: all 0.2s var(--echo-spring-quick);
  }

  .search-input:focus {
    background: var(--echo-glass-medium);
    border-color: rgba(255, 255, 255, 0.22);
    box-shadow: 0 0 16px -4px rgba(255, 255, 255, 0.12);
  }

  .spec-pill {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 12px;
    background: rgba(0, 0, 0, 0.5);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--echo-radius-full);
    font-family: "NDot57", "D-DIN Exp", monospace;
    font-size: 10px;
    letter-spacing: 0.06em;
  }

  .standby-pill {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 12px;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: var(--echo-radius-full);
  }

  .standby-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--echo-text-muted);
  }

  .dot-live {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--echo-color-accent);
    box-shadow: 0 0 8px var(--echo-color-accent);
    animation: pulse-glow 2s infinite ease-in-out;
  }

  .dot-live.buffering {
    background: var(--echo-status-buffering);
    box-shadow: 0 0 8px var(--echo-status-buffering);
  }

  @keyframes pulse-glow {
    0%, 100% { opacity: 0.8; transform: scale(1); }
    50% { opacity: 1; transform: scale(1.15); }
  }

  .pill-codec {
    color: #fff;
    font-weight: 600;
  }

  .pill-divider {
    color: var(--echo-text-muted);
  }

  .pill-specs {
    color: var(--echo-text-secondary);
  }

  .lossless-badge {
    background: var(--echo-lossless-gold);
    color: #000;
    font-weight: 800;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 8px;
    letter-spacing: 0.08em;
  }

  .window-controls {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .win-btn {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    background: transparent;
    border: none;
    color: var(--echo-text-secondary);
    cursor: pointer;
    border-radius: 6px;
    font-size: 11px;
    transition: all 0.15s ease;
  }

  .win-btn:hover {
    background: var(--echo-glass-subtle);
    color: #fff;
  }

  .win-btn.close:hover {
    background: var(--echo-color-accent);
    color: #fff;
  }
</style>
