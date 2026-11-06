<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { player } from './stores/playerState.svelte';
  import type { PlaybackProgress, TrackMeta, Lyrics, PlaybackState } from './types';
  import TopBar from './components/TopBar.svelte';
  import SideNav from './components/SideNav.svelte';
  import MiniPlayer from './components/MiniPlayer.svelte';
  import QueueDrawer from './components/QueueDrawer.svelte';
  import FullScreenPlayer from './components/FullScreenPlayer.svelte';
  import MusicFlow from './components/MusicFlow.svelte';
  import SearchResultsView from './components/SearchResultsView.svelte';
  import DynamicAmbientMesh from './components/DynamicAmbientMesh.svelte';
  import YouTubeAudioBridge from './components/YouTubeAudioBridge.svelte';

  let unlistenProgress: UnlistenFn | null = null;
  let unlistenTrack: UnlistenFn | null = null;
  let unlistenStatus: UnlistenFn | null = null;
  let unlistenLyrics: UnlistenFn | null = null;

  onMount(async () => {
    try {
      unlistenProgress = await listen<PlaybackProgress>('playback://progress', (event) => {
        player.positionMs = event.payload.position_ms;
        if (event.payload.duration_ms > 0) {
          player.durationMs = event.payload.duration_ms;
        }
      });

      unlistenTrack = await listen<TrackMeta>('playback://track_change', (event) => {
        player.setTrack(event.payload);
      });

      unlistenStatus = await listen<PlaybackState>('playback://status', (event) => {
        player.isPlaying = event.payload === 'Playing';
        player.isBuffering = event.payload === 'Buffering';
      });

      unlistenLyrics = await listen<Lyrics>('lyrics://ready', (event) => {
        player.currentLyrics = event.payload;
      });
    } catch (err) {
      console.warn('Tauri event listeners not active in web preview mode:', err);
    }
  });

  onDestroy(() => {
    if (unlistenProgress) unlistenProgress();
    if (unlistenTrack) unlistenTrack();
    if (unlistenStatus) unlistenStatus();
    if (unlistenLyrics) unlistenLyrics();
  });
</script>

<div class="echo-app-root">
  <!-- Dynamic Ambient Lighting Ground -->
  <DynamicAmbientMesh colors={player.palette} />

  <!-- Window Top Bar with Tauri Drag Region -->
  <TopBar />

  <!-- Core Two-Column Layout -->
  <div class="echo-main-body">
    <SideNav />

    <main class="echo-content-viewport" aria-label="Main Viewport">
      {#if player.activeTab === 'flow'}
        <MusicFlow />
      {:else if player.activeTab === 'search'}
        <SearchResultsView />
      {:else if player.activeTab === 'library'}
        <div class="library-placeholder">
          <header class="section-header">
            <h2 class="echo-font-matrix text-xl tracking-wider text-white">ARCHIVE</h2>
            <span class="echo-font-matrix text-[10px] text-echo-text-muted">LOCAL & CACHED LOSSLESS LIBRARY</span>
          </header>
          <div class="mt-8 p-12 text-center text-echo-text-muted border border-white/5 rounded-echo-xl bg-echo-surface-base/40">
            <span class="text-4xl block mb-2">◫</span>
            <p class="text-sm font-medium">Local library indexing standby</p>
            <p class="text-xs text-echo-text-secondary mt-1">Tracks you stream and favorite are indexed in SQLite cache</p>
          </div>
        </div>
      {:else if player.activeTab === 'favorites'}
        <div class="library-placeholder">
          <header class="section-header">
            <h2 class="echo-font-matrix text-xl tracking-wider text-white">LOVED AUDIO</h2>
            <span class="echo-font-matrix text-[10px] text-echo-text-muted">HIGH FIDELITY FAVORITES</span>
          </header>
          <div class="mt-8 p-12 text-center text-echo-text-muted border border-white/5 rounded-echo-xl bg-echo-surface-base/40">
            <span class="text-4xl block mb-2 text-echo-accent">♥</span>
            <p class="text-sm font-medium">No loved tracks yet</p>
            <p class="text-xs text-echo-text-secondary mt-1">Tap the heart icon on any playing track to archive here</p>
          </div>
        </div>
      {/if}
    </main>
  </div>

  <!-- Docked MiniPlayer -->
  <MiniPlayer />

  <!-- Slide-out Queue Drawer -->
  <QueueDrawer />

  <!-- FullScreen Liquid Stage -->
  {#if player.isFullScreenLyrics}
    <FullScreenPlayer />
  {/if}

  <!-- Online YouTube Audio Playback Bridge -->
  <YouTubeAudioBridge />
</div>

<style>
  .echo-app-root {
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    position: relative;
    background: radial-gradient(circle at 50% 0%, #101216 0%, #060608 100%);
    overflow: hidden;
  }

  .echo-main-body {
    display: flex;
    flex: 1;
    overflow: hidden;
    position: relative;
    z-index: 10;
  }

  .echo-content-viewport {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: var(--echo-spacing-lg);
    padding-bottom: 120px; /* Clearance for docked MiniPlayer */
    contain: layout style;
  }

  .library-placeholder {
    display: flex;
    flex-direction: column;
    gap: var(--echo-spacing-lg);
  }
</style>
